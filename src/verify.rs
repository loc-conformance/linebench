use std::fs;
use std::path::{Path, PathBuf};

use crate::corpus::{Counted, Parity, judge_parity};
use crate::files::read_text;
use crate::measure::Table;
use crate::measure::{CONTROL_END, CONTROL_START};
use crate::record::{COUNTS_CSV, RECORD_FILE, SUMMARY_CSV, UNKNOWN_INSTANCE};
use crate::record::{
    CountRecord, InstanceRecord, Measurement, Record, RunSettings, Timings, build_counts_csv,
    build_summary_csv, calculate_drift, read_export, read_record,
};

const A_MICROSECOND: f64 = 0.000_001;
const BROKEN_LINES_SHOWN: usize = 5;
const EQUAL: &str = "equal";
const HALF_A_HUNDREDTH: f64 = 0.005;
const HALF_A_MICROSECOND: f64 = 0.000_000_5;
const HALF_A_UNIT: f64 = 0.5;
const HEX_DIGITS: usize = 64;
const LETTERS_AROUND: usize = 8;
const LETTERS_SHOWN: usize = 60;
const THE_EXPORTS: &str = "the hyperfine exports";
const THE_FLAT_FILES: &str = "the flat files";
const THE_RECORD: &str = "the record";

#[derive(Debug, Clone, PartialEq)]
pub struct Verification {
    pub run: PathBuf,
    pub record: Level,
    pub flat: Level,
    pub raw: Level,
}

impl Verification {
    pub fn get_levels(&self) -> [&Level; 3] {
        [&self.record, &self.flat, &self.raw]
    }

    pub fn count_broken(&self) -> usize {
        self.get_levels()
            .iter()
            .map(|level| level.count_broken())
            .sum()
    }

    pub fn describe_reach(&self) -> &'static str {
        match (self.flat.absent.is_empty(), self.raw.absent.is_empty()) {
            (true, true) => "everything published was read, down to the time of every execution",
            (true, false) => "the record and the flat files were read, the raw times were absent",
            (false, true) => "the record and the raw times were read, the flat files were absent",
            (false, false) => {
                "the record was read alone, the flat files and the raw times were absent"
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Level {
    pub name: String,
    pub held: Vec<Held>,
    pub absent: Vec<String>,
}

impl Level {
    pub fn named(name: &str) -> Level {
        Level {
            name: name.to_string(),
            held: Vec::new(),
            absent: Vec::new(),
        }
    }

    pub fn count_broken(&self) -> usize {
        self.held.iter().filter(|held| !held.holds()).count()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Held {
    pub said: String,
    pub broken: Vec<String>,
}

impl Held {
    pub fn of(said: String, broken: Vec<String>) -> Held {
        Held { said, broken }
    }

    pub fn holds(&self) -> bool {
        self.broken.is_empty()
    }
}

pub fn find_run(path: &Path) -> Result<PathBuf, String> {
    if path.is_file() {
        if path.file_name().is_some_and(|name| name == RECORD_FILE) {
            let held = path.parent().filter(|dir| !dir.as_os_str().is_empty());
            return Ok(held.unwrap_or(Path::new(".")).to_path_buf());
        }
        return Err(format!("{} is no {RECORD_FILE}", path.display()));
    }
    if !path.is_dir() {
        return Err(format!("{} is not there", path.display()));
    }
    if path.join(RECORD_FILE).is_file() {
        return Ok(path.to_path_buf());
    }
    Err(format!(
        "no run in {}, since it carries no {RECORD_FILE}: name a run directory further in",
        path.display()
    ))
}

pub fn check_run(dir: &Path) -> Result<Verification, String> {
    let record = read_record(&dir.join(RECORD_FILE))?;
    let mut level = Level::named(THE_RECORD);
    level.held.push(check_measurements(&record.measurements));
    level.held.push(check_derived_columns(&record.measurements));
    level
        .held
        .push(check_counts(&record.counts, &record.measurements));
    if let Some(held) = check_parity(&record) {
        level.held.push(held);
    }
    level.held.push(check_instances(
        &record.settings,
        &record.instances,
        &record.measurements,
    ));
    level.held.push(check_control(
        &record.measurements,
        &record.hyperfine_failures,
    ));
    Ok(Verification {
        run: dir.to_path_buf(),
        record: level,
        flat: check_flat_files(dir, &record),
        raw: check_exports(dir, &record.measurements),
    })
}

fn check_measurements(measurements: &[Measurement]) -> Held {
    let mut broken = Vec::new();
    for m in measurements {
        let row = describe_row(m);
        if m.runs == 0 {
            broken.push(format!("{row}: no execution behind it"));
            continue;
        }
        if m.mean_s <= 0.0 {
            broken.push(format!("{row}: a mean of {}", m.mean_s));
            continue;
        }
        if m.min_s > m.max_s {
            broken.push(format!("{row}: min {} over max {}", m.min_s, m.max_s));
        }
        if m.mean_s < m.min_s || m.mean_s > m.max_s {
            broken.push(format!(
                "{row}: mean {} outside min {} and max {}",
                m.mean_s, m.min_s, m.max_s
            ));
        }
        if m.median_s < m.min_s || m.median_s > m.max_s {
            broken.push(format!(
                "{row}: median {} outside min {} and max {}",
                m.median_s, m.min_s, m.max_s
            ));
        }
        if m.stddev_s < 0.0 || m.stddev_s > m.max_s - m.min_s {
            broken.push(format!(
                "{row}: a spread of {} over a range of {}",
                m.stddev_s,
                m.max_s - m.min_s
            ));
        }
    }
    Held::of(
        format!("{} measurements hold together", measurements.len()),
        broken,
    )
}

fn check_derived_columns(measurements: &[Measurement]) -> Held {
    let mut broken = Vec::new();
    for m in measurements {
        if m.mean_s <= 0.0 {
            continue;
        }
        let row = describe_row(m);
        let cpu = m.user_s + m.system_s;
        if let Some(said) = m.parallelism
            && !is_within_the_rounding(
                said,
                (cpu, A_MICROSECOND),
                (m.mean_s, HALF_A_MICROSECOND),
                HALF_A_HUNDREDTH,
            )
        {
            broken.push(format!(
                "{row}: parallelism {said} against {:.2} out of its user, system and mean",
                cpu / m.mean_s
            ));
        }
        let Some(lines) = m.counted_lines else {
            continue;
        };
        if let Some(said) = m.lines_per_sec
            && !is_within_the_rounding(
                said as f64,
                (lines as f64, 0.0),
                (m.mean_s, HALF_A_MICROSECOND),
                HALF_A_UNIT,
            )
        {
            broken.push(format!(
                "{row}: {said} lines per second against {:.0} out of its lines and mean",
                lines as f64 / m.mean_s
            ));
        }
        if let Some(said) = m.lines_per_cpu_s
            && cpu > 0.0
            && !is_within_the_rounding(
                said as f64,
                (lines as f64, 0.0),
                (cpu, A_MICROSECOND),
                HALF_A_UNIT,
            )
        {
            broken.push(format!(
                "{row}: {said} lines per cpu second against {:.0} out of its lines and cpu time",
                lines as f64 / cpu
            ));
        }
    }
    Held::of(
        "the columns that come off other columns recompute".to_string(),
        broken,
    )
}

fn check_counts(counts: &[CountRecord], measurements: &[Measurement]) -> Held {
    let mut broken = Vec::new();
    for count in counts {
        let added = count.code + count.comments + count.buckets.values().sum::<u64>();
        if added != count.lines {
            broken.push(format!(
                "{} {}: {} lines, and code, comments and buckets add up to {added}",
                count.set, count.instance, count.lines
            ));
        }
    }
    for m in measurements {
        let Some(table) = Table::of_set(&m.set) else {
            continue;
        };
        let Some(count) = counts
            .iter()
            .find(|count| count.set == table.as_str() && count.instance == m.instance)
        else {
            continue;
        };
        if m.counted_files != Some(count.files) || m.counted_lines != Some(count.lines) {
            broken.push(format!(
                "{}: the row carries {} files and {} lines, the counts carry {} and {}",
                describe_row(m),
                describe_number(m.counted_files),
                describe_number(m.counted_lines),
                count.files,
                count.lines
            ));
        }
    }
    Held::of(
        format!("{} counts add up and the rows carry them", counts.len()),
        broken,
    )
}

fn check_parity(record: &Record) -> Option<Held> {
    let said = record.parity.as_ref()?;
    let counted: Vec<Counted> = record
        .counts
        .iter()
        .filter(|count| count.set == Table::SameWork.as_str())
        .map(|count| Counted {
            instance: count.instance.clone(),
            files: count.files,
            lines: count.lines,
        })
        .collect();
    let again = judge_parity(
        said.reference_files,
        &counted,
        &record.settings.instances,
        said.tolerance,
    );
    let mut broken = Vec::new();
    if &again != said {
        broken.push(format!(
            "the counts re-judged come out {}, and the record carries {}",
            describe_parity(&again),
            describe_parity(said)
        ));
    }
    Some(Held::of(
        "equal work re-judged comes out as it is recorded".to_string(),
        broken,
    ))
}

fn check_instances(
    settings: &RunSettings,
    instances: &[InstanceRecord],
    measurements: &[Measurement],
) -> Held {
    let mut broken = Vec::new();
    for name in &settings.instances {
        let Some(instance) = instances
            .iter()
            .find(|instance| instance.identity.instance == *name)
        else {
            broken.push(format!("{name} was measured and carries no identity"));
            continue;
        };
        let sha256 = &instance.identity.sha256;
        if sha256.len() != HEX_DIGITS || !sha256.chars().all(|digit| digit.is_ascii_hexdigit()) {
            broken.push(format!("{name}: {sha256} is no sha256"));
        }
        if instance.identity.version.trim().is_empty() {
            broken.push(format!("{name}: no version"));
        }
    }
    if !settings.instances.contains(&settings.control) {
        broken.push(format!(
            "the control is {} and it was not measured",
            settings.control
        ));
    }
    for m in measurements {
        if m.instance == UNKNOWN_INSTANCE || settings.instances.contains(&m.instance) {
            continue;
        }
        broken.push(format!(
            "{}: {} is no instance of this run",
            m.set, m.instance
        ));
    }
    Held::of(
        format!(
            "{} instances carry a sha256 and a version",
            settings.instances.len()
        ),
        broken,
    )
}

fn check_control(measurements: &[Measurement], failures: &[String]) -> Held {
    let mut broken = Vec::new();
    let mut failed = Vec::new();
    for set in [CONTROL_START, CONTROL_END] {
        if measurements.iter().any(|m| m.set == set) {
            continue;
        }
        match failures.iter().any(|name| name == set) {
            true => failed.push(set),
            false => broken.push(format!("{set} is not there")),
        }
    }
    if !failed.is_empty() {
        return Held::of(
            format!(
                "{} did not run, and the record says hyperfine failed on it",
                failed.join(" and ")
            ),
            broken,
        );
    }
    let said = match calculate_drift(measurements) {
        Some(drift) => format!("both control runs are there, drift {drift}"),
        None => "both control runs are there".to_string(),
    };
    Held::of(said, broken)
}

fn check_flat_files(dir: &Path, record: &Record) -> Level {
    let mut level = Level::named(THE_FLAT_FILES);
    let built = [
        (SUMMARY_CSV, build_summary_csv(record)),
        (COUNTS_CSV, build_counts_csv(record)),
    ];
    for (name, lines) in built {
        let path = dir.join(name);
        if !path.is_file() {
            level.absent.push(name.to_string());
            continue;
        }
        let said = format!("{name} is what the record says");
        match read_text(&path) {
            Ok(found) => level
                .held
                .push(Held::of(said, compare_lines(&lines, &found))),
            Err(refused) => level.held.push(Held::of(said, vec![refused])),
        }
    }
    level
}

fn check_exports(dir: &Path, measurements: &[Measurement]) -> Level {
    let mut level = Level::named(THE_EXPORTS);
    let mut broken = Vec::new();
    let mut read = 0;
    let found = find_exports(dir);
    for set in name_the_sets(measurements) {
        if !found.iter().any(|(named, _)| *named == set) {
            level.absent.push(format!("{set}.json"));
        }
    }
    for (set, path) in &found {
        let timings = match read_export(path) {
            Ok(timings) => timings,
            Err(refused) => {
                broken.push(refused);
                continue;
            }
        };
        for m in measurements.iter().filter(|m| m.set == *set) {
            let Some(timed) = timings.iter().find(|timed| timed.command == m.command) else {
                broken.push(format!(
                    "{}: {set}.json holds no times for what it timed",
                    describe_row(m)
                ));
                continue;
            };
            broken.extend(hold_against_the_times(m, timed));
            read += 1;
        }
        for timed in &timings {
            if measurements
                .iter()
                .any(|m| m.set == *set && m.command == timed.command)
            {
                continue;
            }
            broken.push(format!(
                "{set}.json timed a command the record carries no row for: {}",
                timed.command
            ));
        }
    }
    if read > 0 || !broken.is_empty() {
        level.held.push(Held::of(
            format!("{read} rows recompute from the time of every execution"),
            broken,
        ));
    }
    level
}

fn find_exports(dir: &Path) -> Vec<(String, PathBuf)> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<(String, PathBuf)> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|kind| kind == "json"))
        .filter(|path| path.file_name().is_some_and(|name| name != RECORD_FILE))
        .filter_map(|path| {
            let named = path.file_stem()?.to_string_lossy().into_owned();
            Some((named, path))
        })
        .collect();
    found.sort();
    found
}

fn hold_against_the_times(m: &Measurement, timed: &Timings) -> Vec<String> {
    let row = describe_row(m);
    if timed.times.is_empty() {
        return vec![format!("{row}: the export holds no times")];
    }
    let mut broken = Vec::new();
    if m.runs != timed.times.len() {
        broken.push(format!(
            "{row}: {} runs against {} times",
            m.runs,
            timed.times.len()
        ));
    }
    let least = timed.times.iter().copied().fold(f64::INFINITY, f64::min);
    let most = timed
        .times
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    let out_of_the_times = [
        ("mean", m.mean_s, calculate_mean(&timed.times)),
        ("median", m.median_s, calculate_median(&timed.times)),
        ("min", m.min_s, least),
        ("max", m.max_s, most),
        ("spread", m.stddev_s, calculate_spread(&timed.times)),
    ];
    for (what, said, from_times) in out_of_the_times {
        if (said - from_times).abs() > A_MICROSECOND {
            broken.push(format!(
                "{row}: {what} {said} against {from_times:.6} out of the times"
            ));
        }
    }
    let exported = [
        ("user", m.user_s, timed.user_s),
        ("system", m.system_s, timed.system_s),
    ];
    for (what, said, exported) in exported {
        if let Some(exported) = exported
            && (said - exported).abs() > A_MICROSECOND
        {
            broken.push(format!(
                "{row}: {what} {said} against {exported} in the export"
            ));
        }
    }
    broken
}

pub fn compare_lines(built: &[String], found: &str) -> Vec<String> {
    let found: Vec<&str> = found
        .lines()
        .map(|line| line.trim_end_matches('\r'))
        .collect();
    let mut broken = Vec::new();
    for (at, (built, found)) in built.iter().zip(&found).enumerate() {
        if built.as_str() != *found {
            let (built, found) = describe_difference(built, found);
            broken.push(format!(
                "line {}: the record builds {built}, the file carries {found}",
                at + 1
            ));
        }
    }
    if built.len() != found.len() {
        broken.push(format!(
            "the record builds {} lines and the file carries {}",
            built.len(),
            found.len()
        ));
    }
    if broken.len() > BROKEN_LINES_SHOWN {
        let more = broken.len() - BROKEN_LINES_SHOWN;
        broken.truncate(BROKEN_LINES_SHOWN);
        broken.push(format!("and {more} more"));
    }
    broken
}

fn name_the_sets(measurements: &[Measurement]) -> Vec<String> {
    let mut sets: Vec<String> = Vec::new();
    for m in measurements {
        if !sets.contains(&m.set) {
            sets.push(m.set.clone());
        }
    }
    sets
}

fn describe_difference(built: &str, found: &str) -> (String, String) {
    let one: Vec<char> = built.chars().collect();
    let other: Vec<char> = found.chars().collect();
    let head = one.iter().zip(&other).take_while(|(a, b)| a == b).count();
    let tail = one
        .iter()
        .rev()
        .zip(other.iter().rev())
        .take_while(|(a, b)| a == b)
        .count()
        .min(one.len() - head)
        .min(other.len() - head);
    let head = head.saturating_sub(LETTERS_AROUND);
    let tail = tail.saturating_sub(LETTERS_AROUND);
    (
        shorten(&one[head..one.len() - tail]),
        shorten(&other[head..other.len() - tail]),
    )
}

fn shorten(letters: &[char]) -> String {
    let shown: String = letters.iter().take(LETTERS_SHOWN).collect();
    match letters.len() > LETTERS_SHOWN {
        true => format!("{shown}..."),
        false => shown,
    }
}

fn describe_row(m: &Measurement) -> String {
    format!("{} {}", m.set, m.instance)
}

fn describe_number(number: Option<u64>) -> String {
    number.map_or("none".to_string(), |number| number.to_string())
}

fn describe_parity(parity: &Parity) -> String {
    let said = match parity.problems.is_empty() {
        true => EQUAL.to_string(),
        false => parity.problems.join("; "),
    };
    format!("{said}, over {} instances with counts", parity.counted)
}

/// The record keeps its seconds to six decimals, so a column built out of them has a band of
/// values it can honestly hold, and the stored one has to sit inside that band.
fn is_within_the_rounding(said: f64, over: (f64, f64), divisor: (f64, f64), kept_to: f64) -> bool {
    let ((over, its_slack), (divisor, uncertainty)) = (over, divisor);
    if divisor <= uncertainty {
        return true;
    }
    let most = (over + its_slack) / (divisor - uncertainty) + kept_to;
    let least = (over - its_slack).max(0.0) / (divisor + uncertainty) - kept_to;
    said >= least && said <= most
}

fn calculate_mean(times: &[f64]) -> f64 {
    times.iter().sum::<f64>() / times.len() as f64
}

fn calculate_spread(times: &[f64]) -> f64 {
    if times.len() < 2 {
        return 0.0;
    }
    let mean = calculate_mean(times);
    let sum: f64 = times.iter().map(|time| (time - mean).powi(2)).sum();
    (sum / (times.len() - 1) as f64).sqrt()
}

fn calculate_median(times: &[f64]) -> f64 {
    let mut sorted = times.to_vec();
    sorted.sort_by(f64::total_cmp);
    let middle = sorted.len() / 2;
    match sorted.len() % 2 {
        0 => (sorted[middle - 1] + sorted[middle]) / 2.0,
        _ => sorted[middle],
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::env;
    use std::fs;

    use crate::fetch::{Identity, Origin};

    use super::*;

    fn build_row(set: &str, instance: &str) -> Measurement {
        Measurement {
            set: set.to_string(),
            instance: instance.to_string(),
            command: format!("{instance} tree"),
            mean_s: 0.012,
            stddev_s: 0.002,
            median_s: 0.012,
            min_s: 0.010,
            max_s: 0.014,
            user_s: 0.010,
            system_s: 0.002,
            runs: 3,
            counted_files: Some(27),
            counted_lines: Some(1200),
            lines_per_sec: Some(100_000),
            parallelism: Some(1.0),
            lines_per_cpu_s: Some(100_000),
        }
    }

    fn build_count(lines: u64) -> CountRecord {
        CountRecord {
            set: Table::SameWork.as_str().to_string(),
            instance: "scc".to_string(),
            files: 27,
            lines,
            code: 1000,
            comments: 100,
            buckets: BTreeMap::from([("blanks".to_string(), 100)]),
        }
    }

    fn build_instance(sha256: &str) -> InstanceRecord {
        InstanceRecord {
            identity: Identity {
                instance: "scc".to_string(),
                counter: "scc".to_string(),
                binary: PathBuf::from("scc.exe"),
                sha256: sha256.to_string(),
                version: "4.1.0".to_string(),
                origin: Origin::Given {
                    label: "a build".to_string(),
                },
            },
            languages: Vec::new(),
            same_work: Vec::new(),
            same_work_note: String::new(),
            scrub_env: Vec::new(),
            args: Vec::new(),
        }
    }

    fn build_settings() -> RunSettings {
        RunSettings {
            warmup: 1,
            runs: 3,
            settle: 0,
            instances: vec!["scc".to_string()],
            control: "scc".to_string(),
            unequal_exclusions: None,
            skipped: Vec::new(),
        }
    }

    #[test]
    fn an_edited_mean_leaves_the_columns_that_come_off_it_behind() {
        let mut row = build_row("t1-fwd", "scc");
        assert!(check_derived_columns(std::slice::from_ref(&row)).holds());
        row.mean_s = 0.006;
        let held = check_derived_columns(&[row]);
        let named = |what: &str| held.broken.iter().any(|line| line.contains(what));
        assert!(named("parallelism"), "{held:?}");
        assert!(named("lines per second"), "{held:?}");
        assert!(!named("lines per cpu second"), "{held:?}");
    }

    #[test]
    fn a_mean_of_half_a_millisecond_still_recomputes_its_lines_per_second() {
        let mut row = build_row("t1-fwd", "scc");
        row.min_s = 0.000400;
        row.max_s = 0.000600;
        row.median_s = 0.000500;
        row.mean_s = 0.000500;
        row.user_s = 0.0;
        row.system_s = 0.0;
        row.parallelism = Some(0.0);
        row.counted_lines = Some(15528);
        row.lines_per_sec = Some(31_087_087);
        row.lines_per_cpu_s = None;
        assert!(check_derived_columns(&[row]).holds());
    }

    #[test]
    fn a_mean_outside_the_min_and_max_of_its_own_row_is_caught() {
        let mut row = build_row("t1-fwd", "scc");
        assert!(check_measurements(std::slice::from_ref(&row)).holds());
        row.mean_s = 0.020;
        let held = check_measurements(&[row]);
        assert!(
            held.broken.iter().any(|line| line.contains("mean 0.02")),
            "{held:?}"
        );
    }

    #[test]
    fn a_count_that_stops_adding_up_takes_the_row_that_carries_it_down_too() {
        let row = build_row("t1-fwd", "scc");
        let held = check_counts(&[build_count(1200)], std::slice::from_ref(&row));
        assert!(held.holds(), "{held:?}");
        let held = check_counts(&[build_count(1300)], &[row]);
        assert_eq!(held.broken.len(), 2, "{held:?}");
    }

    #[test]
    fn the_times_of_the_export_rebuild_every_statistic_of_the_row() {
        let row = build_row("t1-fwd", "scc");
        let timed = Timings {
            command: row.command.clone(),
            times: vec![0.010, 0.012, 0.014],
            user_s: Some(0.010),
            system_s: Some(0.002),
        };
        assert!(hold_against_the_times(&row, &timed).is_empty());
        let moved = Timings {
            times: vec![0.010, 0.012, 0.020],
            ..timed
        };
        let broken = hold_against_the_times(&row, &moved);
        let named = |what: &str| broken.iter().any(|line| line.contains(what));
        assert!(
            named("mean") && named("max") && named("spread"),
            "{broken:?}"
        );
        assert!(!named("median"), "{broken:?}");
    }

    #[test]
    fn an_edited_csv_line_is_named_with_the_part_that_moved_and_a_carriage_return_is_not_one() {
        let built = vec![
            "set,instance,mean_s".to_string(),
            "t1-fwd,scc,0.012".to_string(),
        ];
        let found = format!("set,instance,mean_s{0}t1-fwd,scc,0.006{0}", "\r\n");
        let broken = compare_lines(&built, &found);
        assert_eq!(broken.len(), 1, "{broken:?}");
        assert!(
            broken[0].contains("0.012") && broken[0].contains("0.006"),
            "{broken:?}"
        );
    }

    #[test]
    fn an_instance_measured_without_a_sha256_is_caught() {
        let settings = build_settings();
        let whole = build_instance(&"a".repeat(HEX_DIGITS));
        assert!(check_instances(&settings, &[whole], &[]).holds());
        let held = check_instances(&settings, &[build_instance("abc")], &[]);
        assert!(
            held.broken.iter().any(|line| line.contains("is no sha256")),
            "{held:?}"
        );
    }

    #[test]
    fn a_run_is_named_by_its_own_directory_and_a_folder_of_many_is_refused() {
        let dir = env::temp_dir().join("linebench-verify-find-run");
        let _ = fs::remove_dir_all(&dir);
        let run = dir.join("linux").join("windows").join("20260101-000000");
        fs::create_dir_all(&run).unwrap();
        fs::write(run.join(RECORD_FILE), "{}").unwrap();
        assert_eq!(find_run(&run.join(RECORD_FILE)).unwrap(), run);
        assert_eq!(find_run(&run).unwrap(), run);
        let refused = find_run(&dir).unwrap_err();
        assert!(refused.contains("no run in"), "{refused}");
        assert!(refused.contains("further in"), "{refused}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_row_taken_out_of_the_record_is_caught_by_the_export_that_still_times_it() {
        let dir = env::temp_dir().join("linebench-verify-exports");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let document = r#"{"results":[
            {"command":"scc tree","mean":0.012,"stddev":0.002,"median":0.012,"min":0.010,
             "max":0.014,"user":0.010,"system":0.002,"times":[0.010,0.012,0.014]},
            {"command":"cloc tree","mean":0.2,"median":0.2,"min":0.19,"max":0.21,
             "times":[0.19,0.2,0.21]}]}"#;
        fs::write(dir.join("t1-fwd.json"), document).unwrap();
        let level = check_exports(&dir, &[build_row("t1-fwd", "scc")]);
        assert!(level.absent.is_empty(), "{level:?}");
        let broken = &level.held[0].broken;
        assert_eq!(broken.len(), 1, "{broken:?}");
        assert!(broken[0].contains("cloc tree"), "{broken:?}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_control_hyperfine_failed_on_is_a_gap_and_a_control_that_is_simply_gone_is_not() {
        let row = build_row("t1-fwd", "scc");
        let held = check_control(std::slice::from_ref(&row), &[]);
        assert_eq!(held.broken.len(), 2, "{held:?}");
        let failed = [CONTROL_START.to_string(), CONTROL_END.to_string()];
        let held = check_control(&[row], &failed);
        assert!(held.holds(), "{held:?}");
        assert!(held.said.contains("hyperfine failed"), "{held:?}");
    }
}
