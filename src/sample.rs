use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::machine::Platform;
use crate::measure::Table;
use crate::os::read_process_memory;

pub const MEMORY_STEP_MS: u64 = 2;
pub const LEAST_SAMPLES: usize = 30;
pub const KEPT_SAMPLES: usize = 120;
pub const RAW_SAMPLES: usize = 1200;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Curve {
    pub instance: String,
    pub table: String,
    pub step_ms: u64,
    pub spacing_us: u64,
    pub polls: usize,
    pub wall_ms: u64,
    pub peak_bytes: u64,
    pub samples: Vec<u64>,
}

pub fn sample_memory(
    platform: Platform,
    instance: &str,
    table: Table,
    binary: &Path,
    args: &[String],
    scrub: &[String],
) -> Result<Curve, String> {
    let mut command = Command::new(binary);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    for name in scrub {
        command.env_remove(name);
    }
    let refuse = |what: &str, error: &dyn std::fmt::Display| {
        format!("{instance} could not be {what}: {error}")
    };
    let started = Instant::now();
    let mut child = command.spawn().map_err(|error| refuse("run", &error))?;
    let step = Duration::from_millis(MEMORY_STEP_MS);
    let mut samples = Vec::new();
    let mut peak = 0;
    let mut first = started;
    let mut last = started;
    loop {
        let at = Instant::now();
        let reading = read_process_memory(platform, &child);
        match child
            .try_wait()
            .map_err(|error| refuse("waited on", &error))?
        {
            Some(status) if status.success() => break,
            Some(status) => return Err(format!("{instance} ended with {status}")),
            None => {
                if let Some((resident, high)) = reading {
                    if samples.is_empty() {
                        first = at;
                    }
                    last = at;
                    samples.push(resident);
                    peak = peak.max(high);
                }
                thread::sleep(step);
            }
        }
    }
    let wall_ms = started.elapsed().as_millis() as u64;
    if let Some((_, high)) = read_process_memory(platform, &child) {
        peak = peak.max(high);
    }
    let polls = samples.len();
    let spacing_us = if polls > 1 {
        (last - first).as_micros() as u64 / (polls as u64 - 1)
    } else {
        0
    };
    Ok(Curve {
        instance: instance.to_string(),
        table: table.as_str().to_string(),
        step_ms: MEMORY_STEP_MS,
        spacing_us,
        polls,
        wall_ms,
        peak_bytes: peak,
        samples: fold_samples(samples),
    })
}

pub fn fold_into_columns(samples: &[u64], columns: usize) -> Vec<u64> {
    (0..columns)
        .map(|column| {
            let from = column * samples.len() / columns;
            let to = ((column + 1) * samples.len() / columns).max(from + 1);
            let slice = &samples[from..to.min(samples.len())];
            slice.iter().copied().max().unwrap_or_default()
        })
        .collect()
}

fn fold_samples(samples: Vec<u64>) -> Vec<u64> {
    if samples.len() <= RAW_SAMPLES {
        return samples;
    }
    fold_into_columns(&samples, KEPT_SAMPLES)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_column_keeps_the_highest_sample_that_falls_in_it() {
        assert_eq!(fold_into_columns(&[1, 3, 5, 7], 2), [3, 7]);
        assert_eq!(fold_into_columns(&[10], 3), [10, 10, 10]);
    }

    #[test]
    fn a_long_run_is_folded_down_before_it_is_written_and_a_short_one_is_kept_whole() {
        let short: Vec<u64> = (0..RAW_SAMPLES as u64).collect();
        assert_eq!(fold_samples(short.clone()), short);
        let long: Vec<u64> = (0..RAW_SAMPLES as u64 * 4).collect();
        let folded = fold_samples(long);
        assert_eq!(folded.len(), KEPT_SAMPLES);
        assert!(folded.windows(2).all(|pair| pair[0] < pair[1]));
    }
}
