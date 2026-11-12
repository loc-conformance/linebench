use std::process::{Command, Stdio};
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::machine::Platform;
use crate::measure::Table;
use crate::os::{apply_execution_env, reap};
use crate::pmu::Subject;

/// Even, so that rounds of alternating order put every instance in every place equally often.
pub const COLD_RUNS: usize = 4;
pub const WARM_RUNS: usize = 4;
/// One round in each order.
pub const DISCARDED_ROUNDS: usize = 2;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cold {
    pub instance: String,
    pub table: String,
    pub cold_ms: Vec<f64>,
    pub cold_read_bytes: Vec<u64>,
    pub warm_ms: Vec<f64>,
    pub warm_read_bytes: Vec<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimedRun {
    pub ms: f64,
    pub disk_read_bytes: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Rounds {
    pub measured: Vec<Cold>,
    /// Why each instance whose run failed left the rounds.
    pub dropped: Vec<String>,
    /// The page cache could not be emptied, so every later cold run would find a warm cache, and
    /// the rows end incomplete and are dropped.
    pub halted: Option<String>,
}

/// A cold run comes out faster the closer it sits to the start of a session, for a cause not yet
/// found, so each round runs every instance once, the next round runs them in the opposite order,
/// and the first rounds of cold runs and of warm runs are thrown away.
pub fn measure_cold(
    instances: &[&str],
    table: Table,
    purge: &mut dyn FnMut() -> Result<(), String>,
    run: &mut dyn FnMut(usize) -> Result<TimedRun, String>,
) -> Rounds {
    let mut rows: Vec<Cold> = instances
        .iter()
        .map(|instance| Cold {
            instance: instance.to_string(),
            table: table.as_str().to_string(),
            cold_ms: Vec::new(),
            cold_read_bytes: Vec::new(),
            warm_ms: Vec::new(),
            warm_read_bytes: Vec::new(),
        })
        .collect();
    let mut running = vec![true; instances.len()];
    let mut dropped = Vec::new();
    for round in 0..DISCARDED_ROUNDS + COLD_RUNS {
        for at in arrange_round(instances.len(), round) {
            if !running[at] {
                continue;
            }
            if let Err(reason) = purge() {
                return Rounds {
                    measured: Vec::new(),
                    dropped,
                    halted: Some(reason),
                };
            }
            match run(at) {
                Ok(timed) if round >= DISCARDED_ROUNDS => {
                    rows[at].cold_ms.push(timed.ms);
                    rows[at].cold_read_bytes.push(timed.disk_read_bytes);
                }
                Ok(_) => {}
                Err(reason) => {
                    running[at] = false;
                    dropped.push(reason);
                }
            }
        }
    }
    for round in 0..DISCARDED_ROUNDS + WARM_RUNS {
        for at in arrange_round(instances.len(), round) {
            if !running[at] {
                continue;
            }
            match run(at) {
                Ok(timed) if round >= DISCARDED_ROUNDS => {
                    rows[at].warm_ms.push(timed.ms);
                    rows[at].warm_read_bytes.push(timed.disk_read_bytes);
                }
                Ok(_) => {}
                Err(reason) => {
                    running[at] = false;
                    dropped.push(reason);
                }
            }
        }
    }
    Rounds {
        measured: rows
            .into_iter()
            .zip(running)
            .filter_map(|(row, kept)| kept.then_some(row))
            .collect(),
        dropped,
        halted: None,
    }
}

/// Timed by linebench itself from the spawn to the reaping, since the time and the bytes read from
/// disk have to come from the same run, and hyperfine reports times and leaves the bytes out.
pub fn time_run(platform: Platform, subject: &Subject) -> Result<TimedRun, String> {
    let instance = subject.instance;
    let mut command = Command::new(subject.binary);
    command
        .args(subject.args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    apply_execution_env(&mut command, subject.env, subject.scrub);
    let started = Instant::now();
    let mut child = command
        .spawn()
        .map_err(|error| format!("{instance} could not be run: {error}"))?;
    let (status, usage) = reap(platform, &mut child)
        .map_err(|error| format!("{instance} could not be waited on: {error}"))?;
    let ms = started.elapsed().as_secs_f64() * 1000.0;
    if !status.success() {
        return Err(format!("{instance} ended with {status}"));
    }
    Ok(TimedRun {
        ms,
        disk_read_bytes: usage.disk_read_bytes,
    })
}

pub fn calculate_median(values: &[f64]) -> Option<f64> {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let middle = sorted.len() / 2;
    match sorted.len() {
        0 => None,
        count if count % 2 == 1 => Some(sorted[middle]),
        _ => Some((sorted[middle - 1] + sorted[middle]) / 2.0),
    }
}

pub fn calculate_median_bytes(values: &[u64]) -> Option<u64> {
    let as_floats: Vec<f64> = values.iter().map(|&bytes| bytes as f64).collect();
    calculate_median(&as_floats).map(|median| median.round() as u64)
}

fn arrange_round(count: usize, round: usize) -> Vec<usize> {
    match round % 2 {
        0 => (0..count).collect(),
        _ => (0..count).rev().collect(),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    #[cfg(target_os = "linux")]
    use std::collections::BTreeMap;
    #[cfg(target_os = "linux")]
    use std::path::Path;

    use super::*;

    #[test]
    fn the_rounds_alternate_their_order_and_the_first_two_of_each_kind_are_thrown_away() {
        let steps = RefCell::new(Vec::new());
        let mut given = 0.0;
        let rounds = measure_cold(
            &["a", "b"],
            Table::SameWork,
            &mut || {
                steps.borrow_mut().push("purge");
                Ok(())
            },
            &mut |at| {
                steps.borrow_mut().push(["a", "b"][at]);
                given += 1.0;
                Ok(TimedRun {
                    ms: given,
                    disk_read_bytes: 4096,
                })
            },
        );
        let order = ["a", "b", "b", "a"].repeat(3);
        let cold: Vec<&str> = order.iter().flat_map(|name| ["purge", name]).collect();
        assert_eq!(*steps.borrow(), [&cold[..], &order[..]].concat());
        let [a, b] = &rounds.measured[..] else {
            panic!("two rows: {rounds:?}");
        };
        assert_eq!(a.cold_ms, [5.0, 8.0, 9.0, 12.0]);
        assert_eq!(b.cold_ms, [6.0, 7.0, 10.0, 11.0]);
        assert_eq!(a.warm_ms, [17.0, 20.0, 21.0, 24.0]);
        assert_eq!(b.warm_ms, [18.0, 19.0, 22.0, 23.0]);
        assert_eq!(a.cold_read_bytes, [4096; COLD_RUNS]);
        assert_eq!(a.table, "t1");
        assert_eq!((rounds.dropped.len(), rounds.halted), (0, None));
    }

    #[test]
    fn a_purge_that_fails_halfway_drops_every_row() {
        let mut purges = 0;
        let rounds = measure_cold(
            &["mezura", "scc"],
            Table::SameWork,
            &mut || {
                purges += 1;
                match purges {
                    3 => Err("sudo: a password is required".to_string()),
                    _ => Ok(()),
                }
            },
            &mut |_| {
                Ok(TimedRun {
                    ms: 10.0,
                    disk_read_bytes: 0,
                })
            },
        );
        assert!(rounds.measured.is_empty());
        assert_eq!(
            rounds.halted.as_deref(),
            Some("sudo: a password is required")
        );
    }

    #[test]
    fn a_run_that_fails_drops_its_instance_and_the_others_go_on() {
        let mut cloc_runs = 0;
        let rounds = measure_cold(
            &["cloc", "scc"],
            Table::SameWork,
            &mut || Ok(()),
            &mut |at| {
                if at == 0 {
                    cloc_runs += 1;
                    if cloc_runs == 3 {
                        return Err("cloc ended with exit status: 2".to_string());
                    }
                }
                Ok(TimedRun {
                    ms: 10.0,
                    disk_read_bytes: 0,
                })
            },
        );
        assert_eq!(cloc_runs, 3);
        assert_eq!(rounds.dropped, ["cloc ended with exit status: 2"]);
        let [scc] = &rounds.measured[..] else {
            panic!("one row: {rounds:?}");
        };
        assert_eq!(
            (scc.instance.as_str(), scc.cold_ms.len(), scc.warm_ms.len()),
            ("scc", COLD_RUNS, WARM_RUNS)
        );
    }

    #[test]
    fn the_median_is_the_middle_value_or_the_mean_of_the_two_middle_ones() {
        assert_eq!(calculate_median(&[5.0, 1.0, 3.0]), Some(3.0));
        assert_eq!(calculate_median(&[4.0, 1.0, 3.0, 2.0]), Some(2.5));
        assert_eq!(calculate_median(&[]), None);
        assert_eq!(calculate_median_bytes(&[10, 30, 20, 40]), Some(25));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_timed_run_reports_its_time_and_a_failing_one_its_exit_status() {
        let env = BTreeMap::new();
        let run = |binary: &str| {
            time_run(
                Platform::Linux,
                &Subject {
                    instance: binary,
                    binary: Path::new(binary),
                    args: &[],
                    env: &env,
                    scrub: &[],
                },
            )
        };
        let timed = run("true").expect("true succeeds");
        assert!(timed.ms > 0.0);
        let refused = run("false").expect_err("false fails");
        assert!(refused.starts_with("false ended with"), "{refused}");
    }
}
