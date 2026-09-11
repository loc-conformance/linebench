use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::measure::Table;
use crate::os::{Unfinished, capture_output, capture_with_status};

pub const TRACER: &str = "strace";
pub const OTHER_FAMILY: &str = "other";
pub const FAMILIES: [(&str, &[&str]); 7] = [
    ("directories", &["getdents64", "getdents"]),
    ("opening", &["openat", "openat2", "open", "close"]),
    (
        "metadata",
        &[
            "statx",
            "newfstatat",
            "fstatat64",
            "fstat",
            "fstat64",
            "stat",
            "stat64",
            "lstat",
            "lstat64",
            "readlink",
            "readlinkat",
            "access",
            "faccessat",
            "faccessat2",
        ],
    ),
    (
        "reading",
        &["read", "pread64", "readv", "preadv", "preadv2"],
    ),
    (
        "memory",
        &[
            "mmap", "mmap2", "munmap", "mprotect", "mremap", "brk", "madvise",
        ],
    ),
    (
        "threads",
        &[
            "clone",
            "clone3",
            "sched_getaffinity",
            "set_robust_list",
            "rseq",
            "membarrier",
            "gettid",
            "tgkill",
        ],
    ),
    (
        "waiting",
        &[
            "clock_nanosleep",
            "nanosleep",
            "futex",
            "sched_yield",
            "poll",
            "ppoll",
            "select",
            "pselect6",
            "epoll_wait",
            "epoll_pwait",
            "epoll_pwait2",
        ],
    ),
];
const PROBE_TARGET: &str = "true";
const SUMMARY_FLAGS: [&str; 3] = ["-c", "-f", "-o"];
const VERSION_FLAG: &str = "--version";
const TOTAL_ROW: &str = "total";
const RULE: &str = "---";
const CALLS_COLUMN: usize = 3;
const ERRORS_COLUMN: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tracing {
    Ready(String),
    NotThere,
    Refused,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Call {
    pub name: String,
    pub calls: u64,
    pub errors: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Syscalls {
    pub instance: String,
    pub table: String,
    pub wall_ms: u64,
    pub total_calls: u64,
    pub total_errors: u64,
    pub calls: Vec<Call>,
}

pub fn find_tracer(probe: &Path) -> Tracing {
    let path = probe.to_string_lossy().into_owned();
    let mut args: Vec<&str> = SUMMARY_FLAGS.to_vec();
    args.push(&path);
    args.push(PROBE_TARGET);
    match capture_with_status(TRACER, &args) {
        Ok((true, _)) => {}
        Err(Unfinished::NotFound) => return Tracing::NotThere,
        _ => return Tracing::Refused,
    }
    match capture_output(TRACER, &[VERSION_FLAG])
        .and_then(|answer| answer.lines().next().map(|line| line.trim().to_string()))
    {
        Some(version) => Tracing::Ready(version),
        None => Tracing::Refused,
    }
}

pub fn count_syscalls(
    instance: &str,
    table: Table,
    binary: &Path,
    args: &[String],
    scrub: &[String],
    into: &Path,
) -> Result<Syscalls, String> {
    let mut command = Command::new(TRACER);
    command
        .args(SUMMARY_FLAGS)
        .arg(into)
        .arg(binary)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    for name in scrub {
        command.env_remove(name);
    }
    let started = Instant::now();
    let finished = command
        .output()
        .map_err(|error| format!("{instance} could not be traced: {error}"))?;
    let wall_ms = started.elapsed().as_millis() as u64;
    if !finished.status.success() {
        return Err(format!(
            "{instance} under {TRACER} ended with {}: {}",
            finished.status,
            String::from_utf8_lossy(&finished.stderr).trim()
        ));
    }
    let text = fs::read_to_string(into)
        .map_err(|error| format!("{} could not be read: {error}", into.display()))?;
    let (mut calls, rows) = parse_summary(&text);
    calls.sort_by(|left, right| {
        right
            .calls
            .cmp(&left.calls)
            .then_with(|| left.name.cmp(&right.name))
    });
    if let Some(rows) = rows
        && rows != calls.len()
    {
        return Err(format!(
            "{} out of {rows} {TRACER} rows read for {instance}",
            calls.len()
        ));
    }
    let total_calls: u64 = calls.iter().map(|call| call.calls).sum();
    Ok(Syscalls {
        instance: instance.to_string(),
        table: table.as_str().to_string(),
        wall_ms,
        total_calls,
        total_errors: calls.iter().map(|call| call.errors).sum(),
        calls,
    })
}

pub fn get_family(name: &str) -> &'static str {
    FAMILIES
        .into_iter()
        .find(|(_, members)| members.contains(&name))
        .map_or(OTHER_FAMILY, |(family, _)| family)
}

fn parse_summary(text: &str) -> (Vec<Call>, Option<usize>) {
    let mut calls = Vec::new();
    let mut rules = 0;
    let mut rows = 0;
    for line in text.lines() {
        if line.starts_with(RULE) {
            rules += 1;
            continue;
        }
        if rules == 1 && !line.trim().is_empty() {
            rows += 1;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        let Some((name, columns)) = fields.split_last() else {
            continue;
        };
        if !name.starts_with(|first: char| first.is_ascii_alphabetic() || first == '_') {
            continue;
        }
        let Some(numbers) = columns
            .iter()
            .map(|column| column.parse::<f64>().ok())
            .collect::<Option<Vec<f64>>>()
        else {
            continue;
        };
        let Some(counted) = numbers.get(CALLS_COLUMN) else {
            continue;
        };
        if *name == TOTAL_ROW {
            continue;
        }
        calls.push(Call {
            name: (*name).to_string(),
            calls: *counted as u64,
            errors: numbers.get(ERRORS_COLUMN).copied().unwrap_or_default() as u64,
        });
    }
    (calls, (rules > 1).then_some(rows))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SUMMARY: &str = "\
% time     seconds  usecs/call     calls    errors syscall
------ ----------- ----------- --------- --------- ------------------
 24.38    2.087356         294      7079           read
 19.67    1.683815        1114      1511        95 futex
 14.40    1.232824         305      4037           openat
 13.87    1.187355         294      4036           close
 12.88    1.102517         309      3557           newfstatat
  6.62    0.566349         294      1922           fcntl
  3.54    0.302792         313       966           getdents64
  1.75    0.149572         311       480       479 epoll_ctl
  1.65    0.141377         355       398           nanosleep
  0.37    0.031990         457        70           epoll_pwait
  0.15    0.013228         236        56           rt_sigprocmask
  0.15    0.012507         694        18           clone
  0.10    0.008501         500        17           tgkill
  0.09    0.008045         473        17           getpid
  0.09    0.008034         163        49           mmap
  0.09    0.007542         198        38           sigaltstack
  0.09    0.007346         198        37           gettid
  0.06    0.004962         291        17           rt_sigreturn
  0.03    0.002554          22       114           rt_sigaction
  0.02    0.001853         264         7           sched_yield
  0.00    0.000175          87         2           prlimit64
  0.00    0.000107          53         2           sched_getaffinity
  0.00    0.000096          48         2           pread64
  0.00    0.000060          60         1           epoll_create1
  0.00    0.000043          43         1           eventfd2
  0.00    0.000036          36         1           write
  0.00    0.000000           0         2           madvise
  0.00    0.000000           0         1           execve
  0.00    0.000000           0         1         1 prctl
  0.00    0.000000           0         1           arch_prctl
------ ----------- ----------- --------- --------- ------------------
100.00    8.561036         350     24440       575 total
";

    #[test]
    fn a_row_of_the_summary_is_read_by_its_columns_whether_or_not_it_carries_errors() {
        let (calls, rows) = parse_summary(SUMMARY);
        let named = |name: &str| calls.iter().find(|call| call.name == name);
        assert_eq!(rows, Some(calls.len()));
        assert_eq!(calls.len(), 30, "{calls:?}");
        assert_eq!(
            named("read"),
            Some(&Call {
                name: "read".to_string(),
                calls: 7079,
                errors: 0
            })
        );
        assert_eq!(
            named("epoll_ctl"),
            Some(&Call {
                name: "epoll_ctl".to_string(),
                calls: 480,
                errors: 479
            })
        );
        assert_eq!(calls.iter().map(|call| call.calls).sum::<u64>(), 24_440);
        assert_eq!(calls.iter().map(|call| call.errors).sum::<u64>(), 575);
    }

    #[test]
    fn a_syscall_whose_name_opens_with_an_underscore_is_read_like_any_other() {
        let (calls, rows) = parse_summary(
            "% time     seconds  usecs/call     calls    errors syscall\n\
             ------ ----------- ----------- --------- --------- ----------------\n\
            \x2050.00    0.000100           1       820           _llseek\n\
            \x2050.00    0.000100           1       820        12 _newselect\n\
             ------ ----------- ----------- --------- --------- ----------------\n\
             100.00    0.000200                  1640        12 total\n",
        );
        assert_eq!(rows, Some(2));
        assert_eq!(calls.len(), 2, "{calls:?}");
        assert_eq!(calls[0].name, "_llseek");
        assert_eq!(calls[1].errors, 12);
    }

    #[test]
    fn the_heading_and_the_rules_of_the_table_are_no_syscalls() {
        let (calls, rows) = parse_summary(
            "% time     seconds  usecs/call     calls    errors syscall\n\
             ------ ----------- ----------- --------- --------- ----------------\n",
        );
        assert!(calls.is_empty(), "{calls:?}");
        assert_eq!(rows, None);
    }

    #[test]
    fn the_dialects_of_one_job_land_in_one_family_and_an_unknown_call_falls_to_the_others() {
        assert_eq!(get_family("statx"), get_family("lstat"));
        assert_eq!(get_family("getdents64"), get_family("getdents"));
        assert_eq!(get_family("openat"), get_family("open"));
        assert_eq!(get_family("statx"), "metadata");
        assert_eq!(get_family("perf_event_open"), OTHER_FAMILY);
    }

    #[test]
    fn no_syscall_is_claimed_by_two_families() {
        let mut seen: Vec<&str> = Vec::new();
        for (_, members) in FAMILIES {
            for member in members {
                assert!(!seen.contains(member), "{member} sits in two families");
                seen.push(member);
            }
        }
        assert!(!seen.contains(&OTHER_FAMILY));
    }
}
