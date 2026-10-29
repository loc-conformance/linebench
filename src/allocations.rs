use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::measure::Table;
use crate::os::{Unfinished, capture_output, capture_with_status};

pub const ALLOCATOR_TRACER: &str = "valgrind";
const PROBE_TARGET: &str = "true";
const VERSION_FLAG: &str = "--version";
const LOG_FILE_FLAG: &str = "--log-file";
const SUMMARY_FLAGS: [&str; 3] = [
    "--leak-check=no",
    "--undef-value-errors=no",
    "--error-limit=no",
];
const END_OF_FLAGS: &str = "--";
const IN_USE_MARK: &str = "in use at exit:";
const TOTAL_HEAP_MARK: &str = "total heap usage:";
const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Valgrind {
    Ready(String),
    NotThere,
    Refused,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Allocations {
    pub instance: String,
    pub table: String,
    pub wall_ms: u64,
    pub allocs: u64,
    #[serde(default)]
    pub floor_allocs: u64,
    pub frees: u64,
    pub bytes_allocated: u64,
    pub at_exit_bytes: u64,
    pub at_exit_blocks: u64,
}

pub fn find_valgrind(probe: &Path) -> Valgrind {
    let log_arg = format!("{LOG_FILE_FLAG}={}", probe.to_string_lossy());
    let mut args = vec![log_arg.as_str()];
    args.extend(SUMMARY_FLAGS);
    args.extend([END_OF_FLAGS, PROBE_TARGET]);
    match capture_with_status(ALLOCATOR_TRACER, &args) {
        Ok((true, _)) => {}
        Err(Unfinished::NotFound) => return Valgrind::NotThere,
        _ => return Valgrind::Refused,
    }
    if fs::read_to_string(probe)
        .ok()
        .and_then(|text| parse_heap_summary(&text))
        .is_none()
    {
        return Valgrind::Refused;
    }
    match capture_output(ALLOCATOR_TRACER, &[VERSION_FLAG])
        .and_then(|answer| answer.lines().next().map(|line| line.trim().to_string()))
    {
        Some(version) => Valgrind::Ready(version),
        None => Valgrind::Refused,
    }
}

/// A script starting with `#!` hands `/usr/bin/env` to valgrind, which stops at `execve` and
/// records the two allocations of `env` instead of the interpreter's.
pub fn is_elf_binary(path: &Path) -> bool {
    let mut magic = [0u8; 4];
    File::open(path)
        .and_then(|mut file| file.read_exact(&mut magic))
        .is_ok_and(|()| magic == ELF_MAGIC)
}

pub fn count_allocations(
    instance: &str,
    table: Table,
    binary: &Path,
    args: &[String],
    env: &BTreeMap<String, String>,
    scrub: &[String],
    into: &Path,
) -> Result<Allocations, String> {
    let log_arg = format!("{LOG_FILE_FLAG}={}", into.to_string_lossy());
    let mut command = Command::new(ALLOCATOR_TRACER);
    command
        .arg(&log_arg)
        .args(SUMMARY_FLAGS)
        .arg(END_OF_FLAGS)
        .arg(binary)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    for name in scrub {
        command.env_remove(name);
    }
    command.envs(env);
    let started = Instant::now();
    let finished = command.output().map_err(|error| {
        format!("{instance} could not be run under {ALLOCATOR_TRACER}: {error}")
    })?;
    let wall_ms = started.elapsed().as_millis() as u64;
    if !finished.status.success() {
        return Err(format!(
            "{instance} under {ALLOCATOR_TRACER} ended with {}: {}",
            finished.status,
            String::from_utf8_lossy(&finished.stderr).trim()
        ));
    }
    let text = fs::read_to_string(into)
        .map_err(|error| format!("{} could not be read: {error}", into.display()))?;
    let Some((allocs, frees, bytes_allocated, at_exit_bytes, at_exit_blocks)) =
        parse_heap_summary(&text)
    else {
        return Err(format!(
            "{ALLOCATOR_TRACER} wrote no heap summary for {instance}"
        ));
    };
    Ok(Allocations {
        instance: instance.to_string(),
        table: table.as_str().to_string(),
        wall_ms,
        allocs,
        floor_allocs: 0,
        frees,
        bytes_allocated,
        at_exit_bytes,
        at_exit_blocks,
    })
}

fn parse_heap_summary(text: &str) -> Option<(u64, u64, u64, u64, u64)> {
    let mut at_exit = None;
    let mut total = None;
    for line in text.lines() {
        if let Some((_, rest)) = line.split_once(IN_USE_MARK) {
            at_exit = parse_in_use_line(rest);
        } else if let Some((_, rest)) = line.split_once(TOTAL_HEAP_MARK) {
            total = parse_total_heap_line(rest);
        }
    }
    let (at_exit_bytes, at_exit_blocks) = at_exit?;
    let (allocs, frees, bytes_allocated) = total?;
    Some((
        allocs,
        frees,
        bytes_allocated,
        at_exit_bytes,
        at_exit_blocks,
    ))
}

fn parse_in_use_line(rest: &str) -> Option<(u64, u64)> {
    let words: Vec<&str> = rest.split_whitespace().collect();
    if words.len() != 5 || words[1] != "bytes" || words[2] != "in" || words[4] != "blocks" {
        return None;
    }
    Some((read_count(words[0])?, read_count(words[3])?))
}

fn parse_total_heap_line(rest: &str) -> Option<(u64, u64, u64)> {
    let words: Vec<&str> = rest.split_whitespace().collect();
    if words.len() != 7
        || words[1] != "allocs,"
        || words[3] != "frees,"
        || words[5] != "bytes"
        || words[6] != "allocated"
    {
        return None;
    }
    Some((
        read_count(words[0])?,
        read_count(words[2])?,
        read_count(words[4])?,
    ))
}

fn read_count(word: &str) -> Option<u64> {
    word.replace(',', "").parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOG: &str = "==18421== Memcheck, a memory error detector\n\
                       ==18421== Copyright (C) 2002-2024, and GNU GPL'd, by Julian Seward et al.\n\
                       ==18421== Using Valgrind-3.24.0 and LibVEX; rerun with -h for copyright info\n\
                       ==18421== Command: /home/petros/.local/share/linebench/counters/mezura /bench/linux\n\
                       ==18421== Parent PID: 18419\n\
                       ==18421==\n\
                       ==18421==\n\
                       ==18421== HEAP SUMMARY:\n\
                       ==18421==     in use at exit: 4,096 bytes in 12 blocks\n\
                       ==18421==   total heap usage: 84,312 allocs, 84,300 frees, 148,521,984 bytes allocated\n\
                       ==18421==\n\
                       ==18421== For lists of detected and suppressed errors, rerun with: -s\n\
                       ==18421== ERROR SUMMARY: 0 errors from 0 contexts (suppressed: 0 from 0)\n";

    #[test]
    fn the_heap_summary_is_read_with_its_commas_and_an_incomplete_log_is_refused() {
        assert_eq!(
            parse_heap_summary(LOG),
            Some((84_312, 84_300, 148_521_984, 4_096, 12))
        );
        let zero = "==99== HEAP SUMMARY:\n\
                    ==99==     in use at exit: 0 bytes in 0 blocks\n\
                    ==99==   total heap usage: 0 allocs, 0 frees, 0 bytes allocated\n";
        assert_eq!(parse_heap_summary(zero), Some((0, 0, 0, 0, 0)));
        assert_eq!(
            parse_heap_summary("==99==     in use at exit: 0 bytes in 0 blocks\n"),
            None
        );
        assert_eq!(
            parse_heap_summary("==99==   total heap usage: 1 allocs, 1 frees, 8 bytes allocated\n"),
            None
        );
    }

    #[test]
    fn an_elf_binary_is_told_apart_from_a_script() {
        let dir = std::env::temp_dir().join("linebench-elf_binary_check");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let elf = dir.join("mezura");
        let script = dir.join("cloc");
        fs::write(&elf, b"\x7fELF\x02\x01\x01\x00").unwrap();
        fs::write(&script, b"#!/usr/bin/env perl\n").unwrap();
        assert!(is_elf_binary(&elf));
        assert!(!is_elf_binary(&script));
        fs::remove_dir_all(&dir).unwrap();
    }
}
