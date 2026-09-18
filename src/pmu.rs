use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};

use crate::os::{Unfinished, capture_with_status};

pub const PROFILER: &str = "perf";
pub const PARANOID_KEY: &str = "kernel.perf_event_paranoid";
pub const PARANOID_PATH: &str = "/proc/sys/kernel/perf_event_paranoid";
pub const OPEN_PARANOID: i64 = 1;
pub const RUNS: u32 = 5;
pub const FULL_RUN: f64 = 100.0;
pub const USER_ONLY: &str = ":u";
pub const INSTRUCTIONS: &str = "instructions";
pub const CYCLES: &str = "cycles";
pub const BRANCH_INSTRUCTIONS: &str = "branch-instructions";
pub const BRANCH_MISSES: &str = "branch-misses";
pub const L1_LOADS: &str = "L1-dcache-loads";
pub const L1_MISSES: &str = "L1-dcache-load-misses";
pub const CACHE_REFERENCES: &str = "cache-references";
pub const CACHE_MISSES: &str = "cache-misses";
pub const DTLB_MISSES: &str = "dTLB-load-misses";
pub const ITLB_MISSES: &str = "iTLB-load-misses";
pub const PAGE_FAULTS: &str = "page-faults";
pub const CONTEXT_SWITCHES: &str = "context-switches";
/// Four events a pass, since a Zen 5 thread holds six programmable counters and the NMI watchdog
/// takes one. A fifth event in a pass is time-sliced and the count comes back scaled up.
pub const PASSES: [[&str; 4]; 3] = [
    [INSTRUCTIONS, CYCLES, BRANCH_INSTRUCTIONS, BRANCH_MISSES],
    [L1_LOADS, L1_MISSES, CACHE_REFERENCES, CACHE_MISSES],
    [DTLB_MISSES, ITLB_MISSES, PAGE_FAULTS, CONTEXT_SWITCHES],
];
const STAT: &str = "stat";
const SEPARATOR_FLAG: &str = "-x";
const RUNS_FLAG: &str = "-r";
const OUTPUT_FLAG: &str = "-o";
const EVENTS_FLAG: &str = "-e";
const END_OF_FLAGS: &str = "--";
const VERSION_FLAG: &str = "--version";
/// A tab, since perf quotes nothing and prints three of its fields through the locale, so a comma
/// separator would gain a field of its own wherever the decimal mark is a comma.
const SEPARATOR: &str = "\t";
const PROBE_TARGET: &str = "true";
const PROBE_EVENTS: &str = "instructions,cycles";
const RAW_PREFIX: &str = "pmu-";
const RAW_SUFFIX: &str = "tsv";
const COMMENT: char = '#';
const REFUSED_MARK: char = '<';
const PERCENT: char = '%';
const EVENT_SEPARATOR: &str = ",";
const VALUE_COLUMN: usize = 0;
const EVENT_COLUMN: usize = 2;
const NOISE_COLUMN: usize = 3;
const RUNNING_COLUMN: usize = 5;
const LEAST_COLUMNS: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Profiling {
    Ready(String),
    NotThere,
    Shut(i64),
    Refused,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reading {
    pub event: String,
    pub count: Option<u64>,
    pub noise_pct: f64,
    pub running_pct: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pmu {
    pub instance: String,
    pub table: String,
    pub lines: Option<u64>,
    pub wall_ms: u64,
    pub readings: Vec<Reading>,
}

pub fn find_profiler(probe: &Path, privileged: bool, paranoid: Option<i64>) -> Profiling {
    let version = match capture_with_status(PROFILER, &[VERSION_FLAG]) {
        Ok((true, answer)) => answer.lines().next().map(|line| line.trim().to_string()),
        Err(Unfinished::NotFound) => return Profiling::NotThere,
        _ => return Profiling::Refused,
    };
    let Some(version) = version else {
        return Profiling::Refused;
    };
    // Below root and above the open level perf drops the kernel and renames the event, which
    // would publish user-space counts under the name of the whole run.
    if !privileged
        && let Some(level) = paranoid
        && level > OPEN_PARANOID
    {
        return Profiling::Shut(level);
    }
    let path = probe.to_string_lossy().into_owned();
    let asked = [
        STAT,
        SEPARATOR_FLAG,
        SEPARATOR,
        EVENTS_FLAG,
        PROBE_EVENTS,
        OUTPUT_FLAG,
        &path,
        END_OF_FLAGS,
        PROBE_TARGET,
    ];
    match capture_with_status(PROFILER, &asked) {
        Ok((true, _)) => {}
        _ => return Profiling::Refused,
    }
    let counted = fs::read_to_string(probe).unwrap_or_default();
    match parse_counts(&counted)
        .iter()
        .any(|reading| reading.count.is_some())
    {
        true => Profiling::Ready(version),
        false => Profiling::Refused,
    }
}

pub fn read_paranoid() -> Option<i64> {
    fs::read_to_string(PARANOID_PATH).ok()?.trim().parse().ok()
}

pub fn count_one_pass(
    instance: &str,
    pass: usize,
    binary: &Path,
    args: &[String],
    scrub: &[String],
    into: &Path,
) -> Result<Vec<Reading>, String> {
    let events = PASSES
        .get(pass)
        .ok_or_else(|| format!("there is no pass {} to count", pass + 1))?;
    let path = build_raw_path(into, instance, pass);
    let mut command = Command::new(PROFILER);
    command
        .arg(STAT)
        .arg(SEPARATOR_FLAG)
        .arg(SEPARATOR)
        .arg(RUNS_FLAG)
        .arg(RUNS.to_string())
        .arg(EVENTS_FLAG)
        .arg(events.join(EVENT_SEPARATOR))
        .arg(OUTPUT_FLAG)
        .arg(&path)
        .arg(END_OF_FLAGS)
        .arg(binary)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    for name in scrub {
        command.env_remove(name);
    }
    let finished = command
        .output()
        .map_err(|error| format!("{instance} could not be profiled, {error}"))?;
    if !finished.status.success() {
        return Err(format!(
            "{instance} under {PROFILER} ended with {}, {}",
            finished.status,
            String::from_utf8_lossy(&finished.stderr).trim()
        ));
    }
    let text = fs::read_to_string(&path)
        .map_err(|error| format!("{} could not be read, {error}", path.display()))?;
    let read = parse_counts(&text);
    if read.len() != events.len() {
        return Err(format!(
            "{} of the {} events of pass {} came back for {instance}",
            read.len(),
            events.len(),
            pass + 1
        ));
    }
    Ok(read)
}

pub fn remove_raw_files(into: &Path, instance: &str) {
    for pass in 0..PASSES.len() {
        let _ = fs::remove_file(build_raw_path(into, instance, pass));
    }
}

pub fn find_reading<'a>(counted: &'a Pmu, event: &str) -> Option<&'a Reading> {
    counted
        .readings
        .iter()
        .find(|reading| reading.event == event)
}

pub fn get_count(counted: &Pmu, event: &str) -> Option<u64> {
    find_reading(counted, event).and_then(|reading| reading.count)
}

// The extension is written into the name, since an instance carries its tag and with_extension
// would cut mezura@3.1.1 back to mezura@3.1.
fn build_raw_path(into: &Path, instance: &str, pass: usize) -> PathBuf {
    into.join(format!("{RAW_PREFIX}{instance}-{}.{RAW_SUFFIX}", pass + 1))
}

/// perf writes the noise only when more than one run was asked for, and everything after it moves
/// a column when it is absent. It is the one field carrying a percent sign, which is how it is found.
fn parse_counts(text: &str) -> Vec<Reading> {
    let mut readings = Vec::new();
    for line in text.lines() {
        let line = line.trim_end();
        if line.is_empty() || line.starts_with(COMMENT) {
            continue;
        }
        let fields: Vec<&str> = line.split(SEPARATOR).collect();
        if fields.len() < LEAST_COLUMNS {
            continue;
        }
        let event = fields[EVENT_COLUMN].trim();
        if event.is_empty() {
            continue;
        }
        let value = fields[VALUE_COLUMN].trim();
        let noised = fields
            .get(NOISE_COLUMN)
            .is_some_and(|field| field.trim().ends_with(PERCENT));
        let running = match noised {
            true => RUNNING_COLUMN,
            false => RUNNING_COLUMN - 1,
        };
        readings.push(Reading {
            event: event.to_string(),
            count: match value.starts_with(REFUSED_MARK) {
                true => None,
                false => read_decimal(value).map(|read| read as u64),
            },
            noise_pct: match noised {
                true => fields
                    .get(NOISE_COLUMN)
                    .and_then(|field| read_decimal(field.trim().trim_end_matches(PERCENT)))
                    .unwrap_or_default(),
                false => 0.0,
            },
            running_pct: fields
                .get(running)
                .and_then(|field| read_decimal(field.trim()))
                .unwrap_or(FULL_RUN),
        });
    }
    readings
}

fn read_decimal(column: &str) -> Option<f64> {
    match column.contains(',') {
        true => column.replace(',', ".").parse().ok(),
        false => column.parse().ok(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PASS: &str = "# started on Wed Sep 17 12:34:56 2026\n\
                        \n\
                        14923456789\t\tinstructions\t0.01%\t1234567890\t100.00\t2.12\tinsn per cycle\n\
                        7012345678\t\tcycles\t0.23%\t1234567890\t100.00\t\t\n\
                        <not supported>\t\tLLC-loads\t0.00%\t0\t100.00\t\t\n\
                        2345678\t\tdTLB-load-misses\t3.10%\t1012345678\t82.00\t\t\n";

    #[test]
    fn a_row_is_read_by_its_columns_whether_or_not_it_carries_a_metric() {
        let readings = parse_counts(PASS);
        assert_eq!(readings.len(), 4, "{readings:?}");
        assert_eq!(
            readings[0],
            Reading {
                event: "instructions".to_string(),
                count: Some(14_923_456_789),
                noise_pct: 0.01,
                running_pct: 100.0,
            }
        );
        assert_eq!(readings[1].event, "cycles");
        assert_eq!(readings[1].count, Some(7_012_345_678));
        assert_eq!(readings[1].noise_pct, 0.23);
    }

    #[test]
    fn an_event_the_kernel_would_not_open_comes_back_with_no_count_of_its_own() {
        let readings = parse_counts(PASS);
        let refused = &readings[2];
        assert_eq!(refused.event, "LLC-loads");
        assert_eq!(refused.count, None);
        assert_eq!(refused.running_pct, 100.0);
    }

    #[test]
    fn an_event_that_did_not_count_for_the_whole_run_carries_its_share() {
        let readings = parse_counts(PASS);
        let sliced = &readings[3];
        assert_eq!(sliced.event, "dTLB-load-misses");
        assert_eq!(sliced.count, Some(2_345_678));
        assert_eq!(sliced.running_pct, 82.0);
        assert!(
            readings[..3].iter().all(|r| r.running_pct == FULL_RUN),
            "{readings:?}"
        );
    }

    #[test]
    fn a_pass_printed_under_a_locale_that_writes_decimals_with_a_comma_is_read_the_same() {
        let greek = PASS.replace("0.01%", "0,01%").replace("82.00", "82,00");
        let readings = parse_counts(&greek);
        assert_eq!(readings.len(), 4, "{readings:?}");
        assert_eq!(readings[0].noise_pct, 0.01);
        assert_eq!(readings[0].count, Some(14_923_456_789));
        assert_eq!(readings[3].running_pct, 82.0);
    }

    #[test]
    fn a_pass_asked_for_once_writes_no_noise_and_its_running_share_is_still_found() {
        let once = "1234\t\tinstructions\t567890\t100.00\t2.12\tinsn per cycle\n\
                    4321\t\tcycles\t567890\t64.00\t\t\n";
        let readings = parse_counts(once);
        assert_eq!(readings.len(), 2, "{readings:?}");
        assert_eq!(readings[0].noise_pct, 0.0);
        assert_eq!(readings[0].running_pct, 100.0);
        assert_eq!(readings[1].count, Some(4321));
        assert_eq!(readings[1].running_pct, 64.0);
    }

    #[test]
    fn the_header_and_the_blank_line_of_the_output_file_are_no_events() {
        assert!(parse_counts("# started on Wed Sep 17 12:34:56 2026\n\n").is_empty());
        assert!(parse_counts("").is_empty());
        assert!(parse_counts("1234\t\t\n").is_empty());
    }

    #[test]
    fn no_event_is_asked_for_in_two_passes_and_every_pass_fills_its_counters() {
        let mut seen: Vec<&str> = Vec::new();
        for events in PASSES {
            for event in events {
                assert!(!seen.contains(&event), "{event} is asked for twice");
                seen.push(event);
            }
        }
        assert_eq!(seen.len(), PASSES.len() * PASSES[0].len());
    }

    #[test]
    fn the_raw_file_of_a_pass_is_named_for_its_instance_and_its_place() {
        let path = build_raw_path(Path::new("res"), "mezura", 0);
        assert!(path.ends_with("pmu-mezura-1.tsv"), "{}", path.display());
        let tagged = build_raw_path(Path::new("res"), "mezura@3.1.1", PASSES.len() - 1);
        assert!(
            tagged.ends_with("pmu-mezura@3.1.1-3.tsv"),
            "{}",
            tagged.display()
        );
    }
}
