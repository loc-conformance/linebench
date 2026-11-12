use std::collections::BTreeMap;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::machine::Platform;
use crate::measure::Table;
use crate::os::{apply_execution_env, read_process_memory, try_reap};

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
    /// The pages the run was given. A platform that cannot report them leaves it at zero.
    #[serde(default)]
    pub faults: u64,
    /// Windows counts what it read from storage inside its page faults, so there it is zero.
    #[serde(default)]
    pub disk_read_bytes: u64,
    pub samples: Vec<u64>,
}

pub fn sample_memory(
    platform: Platform,
    instance: &str,
    table: Table,
    binary: &Path,
    args: &[String],
    env: &BTreeMap<String, String>,
    scrub: &[String],
) -> Result<Curve, String> {
    let mut command = Command::new(binary);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    apply_execution_env(&mut command, env, scrub);
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
    let usage = loop {
        let at = Instant::now();
        let reading = read_process_memory(platform, &child);
        match try_reap(platform, &mut child).map_err(|error| refuse("waited on", &error))? {
            Some((status, usage)) if status.success() => break usage,
            Some((status, _)) => return Err(format!("{instance} ended with {status}")),
            None => {
                if let Some(held) = reading {
                    if samples.is_empty() {
                        first = at;
                    }
                    last = at;
                    samples.push(held.resident);
                    peak = peak.max(held.peak);
                }
                thread::sleep(step);
            }
        }
    };
    let wall_ms = started.elapsed().as_millis() as u64;
    if platform == Platform::Windows
        && let Some(held) = read_process_memory(platform, &child)
    {
        peak = peak.max(held.peak);
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
        faults: usage.faults,
        disk_read_bytes: usage.disk_read_bytes,
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

    #[cfg(target_os = "linux")]
    #[test]
    fn a_run_over_a_file_out_of_the_page_cache_counts_what_it_read_from_disk() {
        use std::env;
        use std::fs::{self, File};
        use std::io::Write;

        use crate::os::drop_from_page_cache;

        const FILE_BYTES: usize = 4 * 1024 * 1024;
        // Beside the test binary, since /tmp is often tmpfs, which keeps every file in memory.
        let file = env::current_exe()
            .expect("the test binary has a path")
            .with_file_name("linebench-a_run_over_a_file_out_of_the_page_cache.txt");
        let mut written = File::create(&file).expect("the file is created");
        written
            .write_all(&vec![b'x'; FILE_BYTES])
            .expect("the file is written");
        written.sync_all().expect("the file reaches the disk");
        drop_from_page_cache(&written).expect("the file leaves the page cache");
        let read = || {
            sample_memory(
                Platform::Linux,
                "cat",
                Table::SameWork,
                Path::new("cat"),
                &[file.to_string_lossy().into_owned()],
                &BTreeMap::new(),
                &[],
            )
            .expect("cat reads the file")
        };
        let cold = read();
        let warm = read();
        fs::remove_file(&file).expect("the file is removed");
        assert!(
            cold.disk_read_bytes >= FILE_BYTES as u64,
            "{} gave {} bytes from disk once out of the page cache",
            file.display(),
            cold.disk_read_bytes
        );
        assert_eq!(warm.disk_read_bytes, 0);
    }
}
