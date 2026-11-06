use serde::{Deserialize, Serialize};

use crate::os::PhysicalCore;
use crate::record::{Measurement, round_to};

pub const LADDER_PREFIX: &str = "ladder-";
pub const LADDER_WARMUP: u32 = 1;
const NARROW_RUNS: u32 = 3;
const MIDDLE_RUNS: u32 = 5;
const WIDE_RUNS: u32 = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AffinityRung {
    pub label: String,
    pub cores: u32,
    pub logical_cpus: Vec<usize>,
    pub efficiency_cores: bool,
    pub smt: bool,
    pub runs: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LadderPlan {
    pub physical_cores: u32,
    pub strong_cores: u32,
    pub logical_cores: u32,
    pub rungs: Vec<AffinityRung>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ladder {
    pub instance: String,
    pub physical_cores: u32,
    pub strong_cores: u32,
    pub logical_cores: u32,
    pub peak_speedup: f64,
    pub peak_label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub smt_gain_pct: Option<f64>,
    #[serde(default)]
    pub rungs: Vec<LadderRung>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LadderRung {
    pub label: String,
    pub cores: u32,
    pub logical_cpus: Vec<usize>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub efficiency_cores: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub smt: bool,
    pub measurement: Measurement,
    pub speedup: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_prev: Option<f64>,
    pub efficiency: f64,
}

pub fn get_ladder_set_name(label: &str) -> String {
    format!("{LADDER_PREFIX}{label}")
}

pub fn is_ladder_set(set: &str) -> bool {
    set.starts_with(LADDER_PREFIX)
}

pub fn build_ladder_plan(cores: &[PhysicalCore]) -> Result<LadderPlan, String> {
    let valid: Vec<&PhysicalCore> = cores
        .iter()
        .filter(|core| !core.logical_cpus.is_empty())
        .collect();
    if valid.len() < 2 {
        return Err("affinity ladder needs at least two physical cores".to_string());
    }
    let max_class = valid
        .iter()
        .map(|core| core.efficiency_class)
        .max()
        .unwrap_or(0);
    let strong: Vec<&PhysicalCore> = valid
        .iter()
        .copied()
        .filter(|core| core.efficiency_class == max_class)
        .collect();
    let weak: Vec<&PhysicalCore> = valid
        .iter()
        .copied()
        .filter(|core| core.efficiency_class < max_class)
        .collect();
    let strong_cores = strong.len() as u32;
    let physical_cores = valid.len() as u32;
    let logical_cores: u32 = valid
        .iter()
        .map(|core| core.logical_cpus.len() as u32)
        .sum();
    let ordered: Vec<&PhysicalCore> = strong.into_iter().chain(weak).collect();

    let mut rungs = Vec::new();
    let mut count = 1u32;
    while count < strong_cores {
        rungs.push(build_physical_rung(&ordered, count, false));
        count *= 2;
    }
    rungs.push(build_physical_rung(&ordered, strong_cores, false));

    if physical_cores > strong_cores {
        rungs.push(build_physical_rung(&ordered, physical_cores, true));
    }

    if logical_cores > physical_cores {
        let mut all_cpus: Vec<usize> = ordered
            .iter()
            .flat_map(|core| core.logical_cpus.iter().copied())
            .collect();
        all_cpus.sort_unstable();
        all_cpus.dedup();
        rungs.push(AffinityRung {
            label: format!("{logical_cores}t"),
            cores: physical_cores,
            logical_cpus: all_cpus,
            efficiency_cores: physical_cores > strong_cores,
            smt: true,
            runs: WIDE_RUNS,
        });
    }

    let last_index = rungs.len().saturating_sub(1);
    let has_wide_rung = rungs.len() > 2;
    for (index, rung) in rungs.iter_mut().enumerate() {
        rung.runs = if has_wide_rung && index == last_index {
            WIDE_RUNS
        } else if !rung.smt && rung.cores <= 2 {
            NARROW_RUNS
        } else {
            MIDDLE_RUNS
        };
    }

    Ok(LadderPlan {
        physical_cores,
        strong_cores,
        logical_cores,
        rungs,
    })
}

pub fn build_ladders(
    plan: &LadderPlan,
    rung_measurements: &[(AffinityRung, Vec<Measurement>)],
) -> Vec<Ladder> {
    let Some((_, first_measurements)) = rung_measurements.first() else {
        return Vec::new();
    };
    let mut ladders = Vec::new();
    for base in first_measurements {
        if base.median_s <= 0.0 {
            continue;
        }
        let mut rungs: Vec<LadderRung> = Vec::new();
        for (spec, measurements) in rung_measurements {
            let Some(found) = measurements
                .iter()
                .find(|m| m.instance == base.instance && m.median_s > 0.0)
            else {
                continue;
            };
            let raw_speedup = base.median_s / found.median_s;
            let from_prev = rungs
                .last()
                .map(|prev| round_to(prev.measurement.median_s / found.median_s, 2));
            rungs.push(LadderRung {
                label: spec.label.clone(),
                cores: spec.cores,
                logical_cpus: spec.logical_cpus.clone(),
                efficiency_cores: spec.efficiency_cores,
                smt: spec.smt,
                measurement: found.clone(),
                speedup: round_to(raw_speedup, 2),
                from_prev,
                efficiency: round_to(raw_speedup / f64::from(spec.cores), 2),
            });
        }
        if rungs.len() < 2 {
            continue;
        }
        let peak = rungs
            .iter()
            .min_by(|a, b| a.measurement.median_s.total_cmp(&b.measurement.median_s))
            .unwrap_or(&rungs[0]);
        let peak_speedup = peak.speedup;
        let peak_label = peak.label.clone();

        let widest_physical = rungs.iter().rev().find(|r| !r.smt);
        let smt_rung = rungs.iter().find(|r| r.smt);
        let smt_gain_pct = match (widest_physical, smt_rung) {
            (Some(phys), Some(smt)) if smt.measurement.median_s > 0.0 => Some(round_to(
                (phys.measurement.median_s / smt.measurement.median_s - 1.0) * 100.0,
                1,
            )),
            _ => None,
        };

        ladders.push(Ladder {
            instance: base.instance.clone(),
            physical_cores: plan.physical_cores,
            strong_cores: plan.strong_cores,
            logical_cores: plan.logical_cores,
            peak_speedup,
            peak_label,
            smt_gain_pct,
            rungs,
        });
    }
    ladders
}

fn build_physical_rung(
    ordered: &[&PhysicalCore],
    cores: u32,
    efficiency_cores: bool,
) -> AffinityRung {
    let mut logical_cpus: Vec<usize> = ordered
        .iter()
        .take(cores as usize)
        .map(|core| core.logical_cpus[0])
        .collect();
    logical_cpus.sort_unstable();
    AffinityRung {
        label: format!("{cores}c"),
        cores,
        logical_cpus,
        efficiency_cores,
        smt: false,
        runs: MIDDLE_RUNS,
    }
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_measurement(instance: &str, set: &str, median_s: f64) -> Measurement {
        Measurement {
            set: set.to_string(),
            instance: instance.to_string(),
            command: instance.to_string(),
            mean_s: median_s,
            stddev_s: 0.001,
            median_s,
            min_s: median_s,
            max_s: median_s,
            user_s: median_s,
            system_s: 0.01,
            runs: 3,
            counted_files: Some(100),
            counted_lines: Some(500),
            lines_per_sec: Some(500),
            parallelism: Some(1.0),
            lines_per_cpu_s: Some(500),
        }
    }

    #[test]
    fn an_eight_core_smt_topology_builds_doublings_up_to_eight_cores_and_one_smt_rung() {
        let cores: Vec<PhysicalCore> = (0..8)
            .map(|i| PhysicalCore {
                efficiency_class: 0,
                logical_cpus: vec![i * 2, i * 2 + 1],
            })
            .collect();
        let plan = build_ladder_plan(&cores).expect("8c/16t plan");
        assert_eq!(plan.physical_cores, 8);
        assert_eq!(plan.strong_cores, 8);
        assert_eq!(plan.logical_cores, 16);
        let labels: Vec<&str> = plan.rungs.iter().map(|r| r.label.as_str()).collect();
        assert_eq!(labels, vec!["1c", "2c", "4c", "8c", "16t"]);
        let runs: Vec<u32> = plan.rungs.iter().map(|r| r.runs).collect();
        assert_eq!(runs, vec![3, 3, 5, 5, 10]);
        assert_eq!(plan.rungs[0].logical_cpus, vec![0]);
        assert_eq!(plan.rungs[1].logical_cpus, vec![0, 2]);
        assert_eq!(plan.rungs[2].logical_cpus, vec![0, 2, 4, 6]);
        assert_eq!(plan.rungs[3].logical_cpus, vec![0, 2, 4, 6, 8, 10, 12, 14]);
        assert_eq!(plan.rungs[4].logical_cpus, (0..16).collect::<Vec<usize>>());
        assert!(plan.rungs[4].smt);
    }

    #[test]
    fn a_hybrid_six_strong_and_four_weak_topology_adds_strong_then_efficiency_then_smt_rungs() {
        let mut cores = Vec::new();
        for i in 0..6 {
            cores.push(PhysicalCore {
                efficiency_class: 1,
                logical_cpus: vec![i * 2, i * 2 + 1],
            });
        }
        for i in 0..4 {
            cores.push(PhysicalCore {
                efficiency_class: 0,
                logical_cpus: vec![12 + i],
            });
        }
        let plan = build_ladder_plan(&cores).expect("6P+4E plan");
        assert_eq!(plan.strong_cores, 6);
        assert_eq!(plan.physical_cores, 10);
        assert_eq!(plan.logical_cores, 16);
        let labels: Vec<&str> = plan.rungs.iter().map(|r| r.label.as_str()).collect();
        assert_eq!(labels, vec!["1c", "2c", "4c", "6c", "10c", "16t"]);
        assert!(!plan.rungs[3].efficiency_cores);
        assert!(plan.rungs[4].efficiency_cores);
        assert!(!plan.rungs[4].smt);
        assert!(plan.rungs[5].smt);
    }

    #[test]
    fn a_single_core_topology_is_refused_and_speedup_from_prev_efficiency_and_smt_gain_come_out_right()
     {
        let one = vec![PhysicalCore {
            efficiency_class: 0,
            logical_cpus: vec![0, 1],
        }];
        assert!(build_ladder_plan(&one).is_err());

        let cores: Vec<PhysicalCore> = (0..8)
            .map(|i| PhysicalCore {
                efficiency_class: 0,
                logical_cpus: vec![i * 2, i * 2 + 1],
            })
            .collect();
        let plan = build_ladder_plan(&cores).expect("plan");
        let medians = [1.00, 0.52, 0.28, 0.19, 0.16];
        let rung_measurements: Vec<(AffinityRung, Vec<Measurement>)> = plan
            .rungs
            .iter()
            .cloned()
            .zip(medians)
            .map(|(rung, median_s)| {
                let set = get_ladder_set_name(&rung.label);
                (rung, vec![build_measurement("mezura", &set, median_s)])
            })
            .collect();

        let ladders = build_ladders(&plan, &rung_measurements);
        assert_eq!(ladders.len(), 1);
        let ladder = &ladders[0];
        assert_eq!(ladder.peak_speedup, 6.25);
        assert_eq!(ladder.peak_label, "16t");
        assert_eq!(ladder.smt_gain_pct, Some(18.8));
        assert_eq!(ladder.rungs[0].from_prev, None);
        assert_eq!(ladder.rungs[1].from_prev, Some(1.92));
        assert_eq!(ladder.rungs[2].from_prev, Some(1.86));
        assert_eq!(ladder.rungs[3].speedup, 5.26);
        assert_eq!(ladder.rungs[3].from_prev, Some(1.47));
        assert_eq!(ladder.rungs[3].efficiency, 0.66);
        assert_eq!(ladder.rungs[4].from_prev, Some(1.19));
    }
}
