use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::corpus::{Parity, describe_empty_count, shorten_hash};
use crate::defender::{DefenderState, ProcessExclusions, judge_process_exclusions};
use crate::fetch::Identity;
use crate::files::read_text;
use crate::machine::Machine;
use crate::measure::{CONTROL_END, CONTROL_START, FORWARD, REVERSE};
use crate::measure::{Instance, Table, get_set_name};
use crate::read::Counts;

pub const RECORD_FORMAT: u32 = 2;
pub const RECORD_FILE: &str = "run.json";
pub const SUMMARY_CSV: &str = "summary.csv";
pub const COUNTS_CSV: &str = "counts.csv";
pub const NOTES_FILE: &str = "notes.md";
const SECONDS_PER_DAY: u64 = 86_400;
const SUMMARY_COLUMNS: [&str; 16] = [
    "set",
    "instance",
    "command",
    "mean_s",
    "stddev_s",
    "median_s",
    "min_s",
    "max_s",
    "user_s",
    "system_s",
    "runs",
    "counted_files",
    "counted_lines",
    "lines_per_sec",
    "parallelism",
    "lines_per_cpu_s",
];
const COUNT_COLUMNS: [&str; 7] = [
    "set", "instance", "files", "lines", "code", "comments", "buckets",
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Record {
    pub format: u32,
    pub stamp: String,
    pub date: String,
    pub machine: Machine,
    pub defender: DefenderState,
    pub background_busy_percent: Option<f64>,
    pub prepared: Vec<String>,
    pub corpus: CorpusRecord,
    pub settings: RunSettings,
    pub instances: Vec<InstanceRecord>,
    pub counts: Vec<CountRecord>,
    pub measurements: Vec<Measurement>,
    pub parity: Option<Parity>,
    pub hyperfine_failures: Vec<String>,
    pub hyperfine_warnings: Vec<String>,
    #[serde(default)]
    pub capture_failures: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorpusRecord {
    pub name: String,
    pub checkout: PathBuf,
    pub commit: String,
    pub head: Option<String>,
    pub clean: Option<bool>,
    pub pinned: bool,
    #[serde(default)]
    pub extensions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunSettings {
    pub warmup: u32,
    pub runs: u32,
    pub settle: u32,
    pub instances: Vec<String>,
    pub control: String,
    pub unequal_exclusions: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstanceRecord {
    pub identity: Identity,
    pub languages: Vec<String>,
    pub same_work: Vec<String>,
    pub same_work_note: String,
    pub scrub_env: Vec<String>,
    #[serde(default)]
    pub args: Vec<String>,
}

impl InstanceRecord {
    pub fn of(instance: &Instance, extensions: &[String]) -> Result<InstanceRecord, String> {
        Ok(InstanceRecord {
            identity: instance.identity.clone(),
            languages: instance.definition.spell_languages(extensions)?,
            same_work: instance.definition.run.same_work.clone(),
            same_work_note: instance.definition.run.same_work_note.clone(),
            scrub_env: instance.definition.run.scrub_env.clone(),
            args: instance.args.clone(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CountRecord {
    pub set: String,
    pub instance: String,
    pub files: u64,
    pub lines: u64,
    pub code: u64,
    pub comments: u64,
    pub buckets: BTreeMap<String, u64>,
}

impl CountRecord {
    pub fn of(table: Table, instance: &str, counts: &Counts) -> CountRecord {
        CountRecord {
            set: table.as_str().to_string(),
            instance: instance.to_string(),
            files: counts.files,
            lines: counts.lines,
            code: counts.code,
            comments: counts.comments,
            buckets: counts.buckets.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Measurement {
    pub set: String,
    pub instance: String,
    pub command: String,
    pub mean_s: f64,
    pub stddev_s: f64,
    pub median_s: f64,
    pub min_s: f64,
    pub max_s: f64,
    pub user_s: f64,
    pub system_s: f64,
    pub runs: usize,
    pub counted_files: Option<u64>,
    pub counted_lines: Option<u64>,
    pub lines_per_sec: Option<u64>,
    pub parallelism: Option<f64>,
    pub lines_per_cpu_s: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pooled {
    pub instance: String,
    pub mean_s: f64,
    pub stddev_s: f64,
    pub user_s: f64,
    pub system_s: f64,
    pub parallelism: f64,
    pub counted_files: Option<u64>,
    pub counted_lines: Option<u64>,
    pub single_order: bool,
    pub relative: f64,
    pub relative_stddev: f64,
}

pub fn collect_measurements(
    res: &Path,
    commands: &BTreeMap<String, String>,
    counts: &[CountRecord],
) -> Result<(Vec<Measurement>, Vec<String>), String> {
    let mut exports: Vec<PathBuf> = fs::read_dir(res)
        .map_err(|error| format!("{} could not be read: {error}", res.display()))?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .filter(|path| path.file_name().is_some_and(|name| name != RECORD_FILE))
        .collect();
    exports.sort();
    let mut measurements = Vec::new();
    let mut skipped = Vec::new();
    for path in exports {
        let set = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();
        let export: HyperfineExport = match fs::read_to_string(&path)
            .map_err(|error| error.to_string())
            .and_then(|text| serde_json::from_str(&text).map_err(|error| error.to_string()))
        {
            Ok(export) => export,
            Err(error) => {
                skipped.push(format!("skipping {set}.json: {error}"));
                continue;
            }
        };
        for result in export.results {
            let instance = commands
                .get(&result.command)
                .cloned()
                .unwrap_or_else(|| "unknown".to_string());
            let counted = Table::of_set(&set).and_then(|table| {
                counts
                    .iter()
                    .find(|count| count.set == table.as_str() && count.instance == instance)
            });
            measurements.push(build_measurement(&set, &instance, &result, counted));
        }
    }
    Ok((measurements, skipped))
}

pub fn calculate_drift(measurements: &[Measurement]) -> Option<f64> {
    let mean_of = |set: &str| {
        measurements
            .iter()
            .find(|m| m.set == set && m.mean_s > 0.0)
            .map(|m| m.mean_s)
    };
    let (start, end) = (mean_of(CONTROL_START)?, mean_of(CONTROL_END)?);
    Some(round_to(start.max(end) / start.min(end), 4))
}

pub fn pool_orders(one: &Measurement, other: Option<&Measurement>) -> Pooled {
    let Some(other) = other else {
        return Pooled {
            instance: one.instance.clone(),
            mean_s: one.mean_s,
            stddev_s: one.stddev_s,
            user_s: one.user_s,
            system_s: one.system_s,
            parallelism: one.parallelism.unwrap_or(0.0),
            counted_files: one.counted_files,
            counted_lines: one.counted_lines,
            single_order: true,
            relative: 1.0,
            relative_stddev: 0.0,
        };
    };
    let mean = (one.mean_s + other.mean_s) / 2.0;
    let spread = ((one.stddev_s.powi(2) + other.stddev_s.powi(2)) / 2.0
        + (one.mean_s - other.mean_s).powi(2) / 4.0)
        .sqrt();
    let user = (one.user_s + other.user_s) / 2.0;
    let system = (one.system_s + other.system_s) / 2.0;
    Pooled {
        instance: one.instance.clone(),
        mean_s: mean,
        stddev_s: spread,
        user_s: user,
        system_s: system,
        parallelism: if mean > 0.0 {
            round_to((user + system) / mean, 2)
        } else {
            0.0
        },
        counted_files: one.counted_files,
        counted_lines: one.counted_lines,
        single_order: false,
        relative: 1.0,
        relative_stddev: 0.0,
    }
}

pub fn collect_table_rows(measurements: &[Measurement], table: Table) -> (Vec<Pooled>, Vec<f64>) {
    let forward = get_set_name(table, FORWARD);
    let reverse = get_set_name(table, REVERSE);
    let mut rows = Vec::new();
    let mut order_moves = Vec::new();
    for one in measurements.iter().filter(|m| m.set == forward) {
        let other = measurements
            .iter()
            .find(|m| m.set == reverse && m.instance == one.instance);
        rows.push(pool_orders(one, other));
        if let Some(other) = other {
            let least = one.mean_s.min(other.mean_s);
            if least > 0.0 {
                order_moves.push((one.mean_s - other.mean_s).abs() / least);
            }
        }
    }
    rows.sort_by(|a, b| a.mean_s.total_cmp(&b.mean_s));
    if let Some((fastest, fastest_stddev)) = rows
        .first()
        .filter(|row| row.mean_s > 0.0)
        .map(|row| (row.mean_s, row.stddev_s))
    {
        for (index, row) in rows.iter_mut().enumerate() {
            row.relative = row.mean_s / fastest;
            if index > 0 {
                let own = (row.stddev_s / row.mean_s).powi(2);
                let fastest_s = (fastest_stddev / fastest).powi(2);
                row.relative_stddev = row.relative * (own + fastest_s).sqrt();
            }
        }
    }
    (rows, order_moves)
}

const SUMMARY_HEADINGS: [&str; 8] = [
    "instance",
    "wall",
    "vs fastest",
    "user cpu",
    "system cpu",
    "parallelism",
    "files",
    "lines",
];
const COLUMN_GAP: usize = 2;

pub fn format_summary_tables(measurements: &[Measurement]) -> Vec<String> {
    let mut tables = Vec::new();
    for table in crate::measure::TABLES {
        let (rows, _) = collect_table_rows(measurements, table);
        if rows.is_empty() {
            continue;
        }
        let cells: Vec<[String; 8]> = rows.into_iter().map(build_summary_cells).collect();
        tables.push((table.describe(), cells));
    }
    let widths = measure_columns(&tables);
    let mut lines = Vec::new();
    for (title, cells) in &tables {
        lines.push(String::new());
        lines.push(format!("   {title}"));
        lines.push(lay_out_row(&SUMMARY_HEADINGS.map(String::from), &widths));
        for row in cells {
            lines.push(lay_out_row(row, &widths));
        }
    }
    lines
}

fn build_summary_cells(row: Pooled) -> [String; 8] {
    let mut wall = format_wall(row.mean_s, row.stddev_s);
    if row.single_order {
        wall.push_str(" (one order)");
    }
    [
        row.instance,
        wall,
        format_relative(row.relative, row.relative_stddev),
        format!("{:.2} s", row.user_s),
        format!("{:.2} s", row.system_s),
        format!("{:.2}", row.parallelism),
        row.counted_files.map(format_thousands).unwrap_or_default(),
        row.counted_lines.map(format_thousands).unwrap_or_default(),
    ]
}

/// The widest cell in a column sets its width, over every table at once so the tables that are
/// printed together line up with each other.
fn measure_columns(tables: &[(&str, Vec<[String; 8]>)]) -> [usize; 8] {
    let mut widths = SUMMARY_HEADINGS.map(|heading| heading.chars().count());
    for (_, cells) in tables {
        for row in cells {
            for (width, cell) in widths.iter_mut().zip(row) {
                *width = (*width).max(cell.chars().count());
            }
        }
    }
    widths
}

fn lay_out_row(cells: &[String; 8], widths: &[usize; 8]) -> String {
    let mut line = String::from("   ");
    for (index, (cell, width)) in cells.iter().zip(widths).enumerate() {
        if index > 0 {
            line.push_str(&" ".repeat(COLUMN_GAP));
        }
        line.push_str(cell);
        if index + 1 < cells.len() {
            line.push_str(&" ".repeat(width.saturating_sub(cell.chars().count())));
        }
    }
    line.truncate(line.trim_end().len());
    line
}

pub fn format_wall(mean_s: f64, stddev_s: f64) -> String {
    if mean_s <= 0.0 {
        return String::new();
    }
    let text = format!("{} ms", format_thousands((mean_s * 1000.0).round() as u64));
    if stddev_s > 0.0 {
        format!("{text} ± {:.0}", stddev_s * 1000.0)
    } else {
        text
    }
}

pub fn format_relative(relative: f64, stddev: f64) -> String {
    if stddev > 0.0 {
        format!("{relative:.2}x ± {stddev:.2}")
    } else {
        format!("{relative:.2}x")
    }
}

pub fn shorten_version(printed: &str) -> String {
    let mut words = printed.split_whitespace().peekable();
    while let Some(word) = words.next() {
        let bare = word.strip_prefix('v').unwrap_or(word);
        if bare.starts_with(|c: char| c.is_ascii_digit()) && bare.contains('.') {
            let mut shortened = word.to_string();
            if let Some(next) = words.peek()
                && next.starts_with('(')
            {
                let mut tail = Vec::new();
                for word in words.by_ref() {
                    tail.push(word);
                    if word.ends_with(')') {
                        break;
                    }
                }
                shortened.push(' ');
                shortened.push_str(&tail.join(" "));
            }
            return shortened;
        }
    }
    printed.to_string()
}

pub fn format_thousands(number: u64) -> String {
    let digits = number.to_string();
    let mut grouped = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(c);
    }
    grouped
}

pub fn format_busy(busy: Option<f64>) -> String {
    busy.map_or("not sampled".to_string(), |b| format!("{b}% busy"))
}

pub fn describe_empty_bare_counts(counts: &[CountRecord]) -> Vec<String> {
    counts
        .iter()
        .filter(|count| count.set == Table::OutOfTheBox.as_str())
        .filter_map(|count| {
            describe_empty_count(count.files, count.lines)
                .map(|what| format!("{} {what}", count.instance))
        })
        .collect()
}

pub fn write_record(res: &Path, record: &Record) -> Result<(), String> {
    let path = res.join(RECORD_FILE);
    let text = serde_json::to_string(record)
        .map_err(|error| format!("the record could not be written out: {error}"))?;
    fs::write(&path, text)
        .map_err(|error| format!("{} could not be written: {error}", path.display()))
}

pub fn read_record(path: &Path) -> Result<Record, String> {
    let text = read_text(path)?;
    serde_json::from_str(&text).map_err(|error| {
        let written_by = serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|value| value.get("format").and_then(Value::as_u64));
        match written_by {
            Some(format) if format != u64::from(RECORD_FORMAT) => format!(
                "{}: written as record format {format}, and this build reads format \
                 {RECORD_FORMAT}: {error}",
                path.display()
            ),
            _ => format!("{}: {error}", path.display()),
        }
    })
}

pub fn write_csvs(res: &Path, record: &Record) -> Result<(), String> {
    let mut summary = vec![SUMMARY_COLUMNS.join(",")];
    for m in &record.measurements {
        let cells = [
            m.set.clone(),
            m.instance.clone(),
            m.command.clone(),
            m.mean_s.to_string(),
            m.stddev_s.to_string(),
            m.median_s.to_string(),
            m.min_s.to_string(),
            m.max_s.to_string(),
            m.user_s.to_string(),
            m.system_s.to_string(),
            m.runs.to_string(),
            m.counted_files.map(|v| v.to_string()).unwrap_or_default(),
            m.counted_lines.map(|v| v.to_string()).unwrap_or_default(),
            m.lines_per_sec.map(|v| v.to_string()).unwrap_or_default(),
            m.parallelism.map(|v| v.to_string()).unwrap_or_default(),
            m.lines_per_cpu_s.map(|v| v.to_string()).unwrap_or_default(),
        ];
        summary.push(
            cells
                .iter()
                .map(|cell| format_csv_field(cell))
                .collect::<Vec<_>>()
                .join(","),
        );
    }
    write_lines(&res.join(SUMMARY_CSV), &summary)?;
    let mut counts = vec![COUNT_COLUMNS.join(",")];
    for c in &record.counts {
        let buckets: Vec<String> = c
            .buckets
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect();
        let cells = [
            c.set.clone(),
            c.instance.clone(),
            c.files.to_string(),
            c.lines.to_string(),
            c.code.to_string(),
            c.comments.to_string(),
            buckets.join(" "),
        ];
        counts.push(
            cells
                .iter()
                .map(|cell| format_csv_field(cell))
                .collect::<Vec<_>>()
                .join(","),
        );
    }
    write_lines(&res.join(COUNTS_CSV), &counts)
}

pub fn write_notes(res: &Path, record: &Record) -> Result<(), String> {
    let pin = if record.corpus.pinned {
        "pinned and verified"
    } else {
        "not pinned, measured as it stands"
    };
    let prepared = if record.prepared.is_empty() {
        "not prepared, measured as it was".to_string()
    } else {
        record.prepared.join(", ")
    };
    let drift = calculate_drift(&record.measurements).map_or("n/a".to_string(), |d| d.to_string());
    let head = record
        .corpus
        .head
        .as_deref()
        .map(shorten_hash)
        .unwrap_or_else(|| "no commit".to_string());
    let mut lines = vec![
        format!("# Benchmark session notes {}", record.stamp),
        String::new(),
        format!("corpus:   {} @ {head}, {pin}", record.corpus.name),
        format!("          {}", record.corpus.checkout.display()),
        format!(
            "          {}, {}",
            record.machine.corpus_fs, record.machine.corpus_device
        ),
        format!("machine:  {prepared}"),
        format!("          control drift start to end: {drift}"),
        format!(
            "          background before the run: {}",
            format_busy(record.background_busy_percent)
        ),
        format!(
            "MS Defender: realtime {}, {}",
            record.defender.realtime,
            describe_exclusions(record)
        ),
    ];
    for instance in &record.instances {
        lines.push(format!(
            "{:<10}{} {}",
            format!("{}:", instance.identity.instance),
            instance.identity.version,
            instance.identity.describe_origin()
        ));
    }
    if let Some(parity) = &record.parity {
        lines.push(format!("parity:   {}", parity.describe()));
    }
    if !record.capture_failures.is_empty() {
        lines.push(format!("counters: {}", record.capture_failures.join("; ")));
    }
    lines.extend([
        String::new(),
        "- [ ] machine quiet during the run".to_string(),
        String::new(),
        "observations:".to_string(),
        "-".to_string(),
    ]);
    write_lines(&res.join(NOTES_FILE), &lines)
}

pub fn append_to_notes(res: &Path, lines: &[String]) -> Result<(), String> {
    let path = res.join(NOTES_FILE);
    let mut text = String::from("\n");
    text.push_str(&lines.join("\n"));
    text.push('\n');
    OpenOptions::new()
        .append(true)
        .open(&path)
        .and_then(|mut file| file.write_all(text.as_bytes()))
        .map_err(|error| format!("{} could not be written: {error}", path.display()))
}

pub fn format_utc_stamp(seconds_since_epoch: u64) -> String {
    let (date, time) = split_utc(seconds_since_epoch);
    format!("{}-{}", date.replace('-', ""), time.replace(':', ""))
}

pub fn format_utc_date(seconds_since_epoch: u64) -> String {
    let (date, time) = split_utc(seconds_since_epoch);
    format!("{date}T{time}Z")
}

pub fn read_seconds_since_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

fn build_measurement(
    set: &str,
    instance: &str,
    result: &HyperfineResult,
    counted: Option<&CountRecord>,
) -> Measurement {
    let user = result.user.unwrap_or(0.0);
    let system = result.system.unwrap_or(0.0);
    let cpu = user + system;
    let lines = counted.map(|c| c.lines);
    Measurement {
        set: set.to_string(),
        instance: instance.to_string(),
        command: result.command.clone(),
        mean_s: round_to(result.mean, 6),
        stddev_s: round_to(result.stddev.unwrap_or(0.0), 6),
        median_s: round_to(result.median, 6),
        min_s: round_to(result.min, 6),
        max_s: round_to(result.max, 6),
        user_s: round_to(user, 6),
        system_s: round_to(system, 6),
        runs: result.times.len(),
        counted_files: counted.map(|c| c.files),
        counted_lines: lines,
        lines_per_sec: lines
            .filter(|_| result.mean > 0.0)
            .map(|l| (l as f64 / result.mean).round() as u64),
        parallelism: (result.mean > 0.0).then(|| round_to(cpu / result.mean, 2)),
        lines_per_cpu_s: lines
            .filter(|_| cpu > 0.0)
            .map(|l| (l as f64 / cpu).round() as u64),
    }
}

#[derive(Deserialize)]
struct HyperfineExport {
    results: Vec<HyperfineResult>,
}

#[derive(Deserialize)]
struct HyperfineResult {
    command: String,
    mean: f64,
    #[serde(default)]
    stddev: Option<f64>,
    median: f64,
    min: f64,
    max: f64,
    #[serde(default)]
    user: Option<f64>,
    #[serde(default)]
    system: Option<f64>,
    times: Vec<f64>,
}

fn describe_exclusions(record: &Record) -> String {
    if let Some(unequal) = &record.settings.unequal_exclusions {
        return format!(
            "process exclusions UNEQUAL, measured with --allow-unequal-exclusions: {unequal}"
        );
    }
    match judge_process_exclusions(&record.defender) {
        ProcessExclusions::NoCounters => "no counters".to_string(),
        ProcessExclusions::AllExcluded => "every process excluded".to_string(),
        ProcessExclusions::NoneExcluded => "no process excluded".to_string(),
        ProcessExclusions::Unknown(answer) => format!("process exclusions {}", answer.as_str()),
        ProcessExclusions::Unequal => "process exclusions UNEQUAL".to_string(),
    }
}

fn split_utc(seconds_since_epoch: u64) -> (String, String) {
    let z = seconds_since_epoch / SECONDS_PER_DAY + 719_468;
    let era = z / 146_097;
    let doe = z % 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + u64::from(month <= 2);
    let of_day = seconds_since_epoch % SECONDS_PER_DAY;
    (
        format!("{year:04}-{month:02}-{day:02}"),
        format!(
            "{:02}:{:02}:{:02}",
            of_day / 3600,
            of_day % 3600 / 60,
            of_day % 60
        ),
    )
}

fn round_to(value: f64, decimals: i32) -> f64 {
    let scale = 10f64.powi(decimals);
    (value * scale).round() / scale
}

fn format_csv_field(cell: &str) -> String {
    if cell.contains([',', '"', '\n']) {
        format!("\"{}\"", cell.replace('"', "\"\""))
    } else {
        cell.to_string()
    }
}

fn write_lines(path: &Path, lines: &[String]) -> Result<(), String> {
    let mut text = lines.join("\n");
    text.push('\n');
    fs::write(path, text)
        .map_err(|error| format!("{} could not be written: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use std::env;

    use super::*;

    #[test]
    fn the_drift_is_the_ratio_of_the_two_control_means_whichever_is_larger() {
        let measurements = [
            measure(CONTROL_START, "mezura", 0.3600),
            measure(CONTROL_END, "mezura", 0.3754),
        ];
        assert_eq!(calculate_drift(&measurements), Some(1.0428));
        let reversed = [
            measure(CONTROL_START, "mezura", 0.40),
            measure(CONTROL_END, "mezura", 0.38),
        ];
        assert_eq!(calculate_drift(&reversed), Some(1.0526));
        assert_eq!(calculate_drift(&measurements[..1]), None);
    }

    #[test]
    fn both_orders_are_pooled_and_the_rows_are_ranked_against_the_fastest() {
        let measurements = [
            measure("t1-fwd", "mezura", 0.34),
            measure("t1-fwd", "scc", 0.50),
            measure("t1-rev", "mezura", 0.36),
            measure("t1-rev", "scc", 0.54),
            measure("t1-fwd", "tokei", 0.40),
        ];
        let (rows, order_moves) = collect_table_rows(&measurements, Table::SameWork);
        let names: Vec<&str> = rows.iter().map(|r| r.instance.as_str()).collect();
        assert_eq!(names, ["mezura", "tokei", "scc"]);
        assert!((rows[0].mean_s - 0.35).abs() < 1e-9 && !rows[0].single_order);
        assert!((rows[2].relative - 0.52 / 0.35).abs() < 1e-9);
        assert!(rows[1].single_order && rows[1].relative > 1.0);
        assert_eq!(rows[0].relative_stddev, 0.0);
        let scc_spread = ((0.01f64.powi(2) + 0.01f64.powi(2)) / 2.0 + 0.04f64.powi(2) / 4.0).sqrt();
        let mezura_spread =
            ((0.01f64.powi(2) + 0.01f64.powi(2)) / 2.0 + 0.02f64.powi(2) / 4.0).sqrt();
        let spreads = ((scc_spread / 0.52).powi(2) + (mezura_spread / 0.35).powi(2)).sqrt();
        let ratio_spread = rows[2].relative * spreads;
        assert!((rows[2].relative_stddev - ratio_spread).abs() < 1e-9);
        assert_eq!(order_moves.len(), 2);
        assert!((order_moves[0] - 0.02 / 0.34).abs() < 1e-9);

        let one = measure("t1-fwd", "scc", 0.50);
        let other = measure("t1-rev", "scc", 0.54);
        let pooled = pool_orders(&one, Some(&other));
        let expected = ((0.01f64.powi(2) + 0.01f64.powi(2)) / 2.0 + 0.04f64.powi(2) / 4.0).sqrt();
        assert!((pooled.stddev_s - expected).abs() < 1e-9);
        assert!((pooled.parallelism - 4.0).abs() < 1e-9);
    }

    #[test]
    fn a_hyperfine_export_becomes_measurements_named_by_the_command_that_ran() {
        let res = env::temp_dir().join("linebench-a_hyperfine_export_becomes_measurements");
        let _ = fs::remove_dir_all(&res);
        fs::create_dir_all(&res).unwrap();
        fs::write(
            res.join("t1-fwd.json"),
            r#"{"results":[{"command":"D:/c/mezura.exe D:/linux --languages c","mean":0.344,"stddev":0.0238,"median":0.335,"user":2.325,"system":2.204,"min":0.318,"max":0.396,"times":[0.37,0.32,0.33]},{"command":"D:/c/scc.exe D:/linux -i c","mean":0.5,"stddev":0.01,"median":0.5,"user":3.0,"system":1.0,"min":0.49,"max":0.51,"times":[0.5,0.5]}]}"#,
        )
        .unwrap();
        fs::write(res.join("broken.json"), "{not json").unwrap();
        fs::write(res.join(RECORD_FILE), "{}").unwrap();
        let commands = BTreeMap::from([(
            "D:/c/mezura.exe D:/linux --languages c".to_string(),
            "mezura".to_string(),
        )]);
        let counts = [CountRecord {
            set: "t1".to_string(),
            instance: "mezura".to_string(),
            files: 63864,
            lines: 36_036_878,
            code: 0,
            comments: 0,
            buckets: BTreeMap::new(),
        }];
        let (measurements, skipped) = collect_measurements(&res, &commands, &counts).unwrap();
        fs::remove_dir_all(&res).unwrap();
        assert_eq!(skipped.len(), 1, "{skipped:?}");
        assert_eq!(measurements.len(), 2);
        let mezura = &measurements[0];
        assert_eq!(
            (mezura.set.as_str(), mezura.instance.as_str(), mezura.runs),
            ("t1-fwd", "mezura", 3)
        );
        assert_eq!(mezura.counted_lines, Some(36_036_878));
        assert_eq!(
            mezura.lines_per_sec,
            Some((36_036_878.0f64 / 0.344).round() as u64)
        );
        assert_eq!(mezura.parallelism, Some(13.17));
        assert_eq!(measurements[1].instance, "unknown");
        assert_eq!(measurements[1].counted_lines, None);
    }

    #[test]
    fn a_stamp_and_a_date_come_from_utc_seconds() {
        assert_eq!(format_utc_stamp(1_788_000_000), "20260829-104000");
        assert_eq!(format_utc_date(1_788_000_000), "2026-08-29T10:40:00Z");
        assert_eq!(format_utc_date(0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn walls_and_thousands_and_csv_fields_are_formatted_for_reading() {
        assert_eq!(format_wall(1.2345, 0.0123), "1,235 ms ± 12");
        assert_eq!(format_wall(0.344, 0.0), "344 ms");
        assert_eq!(format_wall(0.0, 0.0), "");
        assert_eq!(format_relative(1.3421, 0.1149), "1.34x ± 0.11");
        assert_eq!(format_relative(1.0, 0.0), "1.00x");
        assert_eq!(shorten_version("scc version 4.0.0"), "4.0.0");
        assert_eq!(
            shorten_version("v3.0.0 (2026-09-02)"),
            "v3.0.0 (2026-09-02)"
        );
        assert_eq!(
            shorten_version("tokei 14.0.0 compiled with serialization support: json"),
            "14.0.0"
        );
        assert_eq!(shorten_version("cloc 2.10"), "2.10");
        assert_eq!(shorten_version("no number here"), "no number here");
        assert_eq!(format_thousands(36_036_878), "36,036,878");
        assert_eq!(format_thousands(999), "999");
        assert_eq!(format_csv_field("a, b"), "\"a, b\"");
        assert_eq!(format_csv_field("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(format_csv_field("plain"), "plain");
    }

    #[test]
    fn every_summary_column_is_as_wide_as_its_widest_cell_and_two_spaces_apart() {
        let measurements = [
            measure("t1-fwd", "mezura", 0.5),
            measure("t1-rev", "mezura", 0.5),
            measure("t1-fwd", "mezura@v3.1.0-a-very-long-tag", 0.25),
            measure("t2-fwd", "mezura", 0.75),
            measure("t2-rev", "mezura", 0.75),
            measure("t2-fwd", "mezura@v3.1.0-a-very-long-tag", 0.25),
        ];
        let heading = "   instance                       wall                     vs fastest    \
                       user cpu  system cpu  parallelism  files  lines";
        assert_eq!(
            format_summary_tables(&measurements),
            [
                "",
                "   Same work",
                heading,
                "   mezura@v3.1.0-a-very-long-tag  250 ms ± 10 (one order)  1.00x         \
                 1.00 s    0.00 s      4.00",
                "   mezura                         500 ms ± 10              2.00x ± 0.09  \
                 1.00 s    1.00 s      4.00",
                "",
                "   Out of the box",
                heading,
                "   mezura@v3.1.0-a-very-long-tag  250 ms ± 10 (one order)  1.00x         \
                 1.00 s    0.00 s      4.00",
                "   mezura                         750 ms ± 10              3.00x ± 0.13  \
                 1.00 s    2.00 s      4.00",
            ]
        );
    }

    fn measure(set: &str, instance: &str, mean_s: f64) -> Measurement {
        Measurement {
            set: set.to_string(),
            instance: instance.to_string(),
            command: format!("{instance} corpus"),
            mean_s,
            stddev_s: 0.01,
            median_s: mean_s,
            min_s: mean_s,
            max_s: mean_s,
            user_s: 1.0,
            system_s: mean_s * 4.0 - 1.0,
            runs: 15,
            counted_files: None,
            counted_lines: None,
            lines_per_sec: None,
            parallelism: Some(4.0),
            lines_per_cpu_s: None,
        }
    }
}
