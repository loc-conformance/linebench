use std::cmp::Reverse;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::corpus::shorten_hash;
use crate::defender::DefenderState;
use crate::machine::{Machine, Platform};
use crate::measure::TABLES;
use crate::measure::{Style, Table};
use crate::record::LOCAL_DIR;
use crate::record::{CorpusRecord, InstanceRecord, Measurement};
use crate::record::{format_thousands, format_utc_minute, format_versions};
use crate::sample::Curve;
use crate::sample::LEAST_SAMPLES;
use crate::sample::fold_into_columns;
use crate::syscalls::get_family;
use crate::syscalls::{Call, Syscalls};
use crate::syscalls::{FAMILIES, OTHER_FAMILY};

pub const FLOOR_WARMUP: u32 = 5;
pub const FLOOR_RUNS: u32 = 30;
pub const VERSION_SET: &str = "floor-version";
pub const INSIGHTS_DIR: &str = "insights";
pub const INSIGHTS_FILE: &str = "insights.json";
pub const INSIGHTS_PAGE: &str = "insights.md";
pub const INSIGHTS_FORMAT: u32 = 1;
pub const SYSCALLS_HEADING: &str = "family / call";
const PAGE_INTRO: &str = "Written by `linebench insights` when the session ends. What a run cannot \
                          measure about itself, since watching a process closely enough disturbs \
                          the times it would report. The tables are the ones the command printed, \
                          kept as they were laid out.";
const FLOOR_MEANS: &str = "What a counter costs before it has counted anything.";
const MEMORY_MEANS: &str = "What each counter held while it counted, sampled while it ran. The \
                            axis under each curve is the wall time of that run.";
const SYSCALLS_MEANS: &str = "What each counter asked of the kernel, counted by the tracer.";
const NO_SYSCALLS: &str = "The system calls were not counted in this session.";
const FLOOR_PREFIX: &str = "floor-";
const VERSION_HEADING: &str = "--version";
const FIRST_HEADINGS: [&str; 2] = ["instance", VERSION_HEADING];
const VERSION_MEANS: &str = "the binary answering its version flag and quitting";
const READY_MEANS: &str = "the same binary over a target with no files, its report printed";
const CHART_ROWS: usize = 6;
const CHART_COLUMNS: usize = 30;
const LABEL_EVERY: usize = 2;
const BLOCKS: [char; 8] = [
    '\u{2581}', '\u{2582}', '\u{2583}', '\u{2584}', '\u{2585}', '\u{2586}', '\u{2587}', '\u{2588}',
];
const EIGHTHS: usize = 8;
const AXIS_TICKS: usize = 5;
const KILOBYTE: f64 = 1024.0;
const MEGABYTE: u64 = 1024 * 1024;
const LADDER: [u64; 7] = [
    10 * MEGABYTE,
    20 * MEGABYTE,
    50 * MEGABYTE,
    100 * MEGABYTE,
    200 * MEGABYTE,
    500 * MEGABYTE,
    1024 * MEGABYTE,
];
const MICROSECONDS_IN_MS: f64 = 1000.0;
const SHADES: [[u8; 3]; 8] = [
    [42, 75, 181],
    [48, 112, 220],
    [16, 184, 216],
    [22, 196, 124],
    [154, 214, 54],
    [242, 167, 42],
    [232, 64, 47],
    [238, 78, 224],
];
const SYSCALLS_HEADINGS: [&str; 4] = ["instance", "syscalls", "per file", "errors"];
const MEMBER_INDENT: &str = "  ";
const REST_LABEL: &str = "rest";
const SHOWN_PART: u64 = 100;
const LABEL_WIDTH: usize = 16;
const INDENT: &str = "   ";
const COLUMN_GAP: usize = 2;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Insights {
    pub format: u32,
    pub stamp: String,
    pub date: String,
    pub machine: Machine,
    pub defender: DefenderState,
    pub unequal_exclusions: Option<String>,
    pub corpus: CorpusRecord,
    pub instances: Vec<InstanceRecord>,
    pub floor: Vec<Measurement>,
    pub curves: Vec<Curve>,
    pub tracer: Option<String>,
    #[serde(default)]
    pub unmeasured: Option<String>,
    pub syscalls: Vec<Syscalls>,
}

pub fn build_insights_path(
    out: &Path,
    corpus: &str,
    platform: Platform,
    stamp: &str,
    is_local: bool,
) -> PathBuf {
    let mut path = out.join(INSIGHTS_DIR);
    if is_local {
        path = path.join(LOCAL_DIR);
    }
    path.join(corpus).join(platform.as_str()).join(stamp)
}

pub fn write_insights(res: &Path, insights: &Insights) -> Result<(), String> {
    let path = res.join(INSIGHTS_FILE);
    let text = serde_json::to_string(insights)
        .map_err(|error| format!("the insights could not be written out: {error}"))?;
    fs::write(&path, text)
        .map_err(|error| format!("{} could not be written: {error}", path.display()))
}

pub fn write_insights_page(
    res: &Path,
    insights: &Insights,
    versions: &[(String, String)],
) -> Result<(), String> {
    let path = res.join(INSIGHTS_PAGE);
    let mut text = build_insights_page(insights, versions).join("\n");
    text.push('\n');
    fs::write(&path, text)
        .map_err(|error| format!("{} could not be written: {error}", path.display()))
}

pub fn build_insights_page(insights: &Insights, versions: &[(String, String)]) -> Vec<String> {
    let machine = &insights.machine;
    let head = insights
        .corpus
        .head
        .as_deref()
        .map(shorten_hash)
        .unwrap_or_else(|| "no commit".to_string());
    let mut lines = vec![
        "# Insights".to_string(),
        String::new(),
        PAGE_INTRO.to_string(),
        String::new(),
        format!("## {}, {}", machine.platform.describe(), machine.cpu),
        String::new(),
        machine.describe(),
        String::new(),
    ];
    let mut measured = Vec::new();
    if let Some(when) = format_utc_minute(&insights.date) {
        measured.push(format!("measured {when}"));
    }
    if !machine.linebench.is_empty() {
        measured.push(format!("by linebench {}", machine.linebench));
    }
    if !measured.is_empty() {
        lines.push(format!("{}  ", measured.join(" ")));
    }
    lines.push(format!(
        "{} corpus at `{head}` on {}, {}  ",
        insights.corpus.name, machine.corpus_fs, machine.corpus_device
    ));
    lines.push(format_versions(&insights.instances));
    lines.push(String::new());
    lines.extend(wrap_in_fence(
        "Floor",
        FLOOR_MEANS,
        format_floor(&insights.floor, versions),
    ));
    lines.extend(wrap_in_fence(
        "Memory",
        MEMORY_MEANS,
        format_memory(&insights.curves, Style::Hidden),
    ));
    let mut counted = format_syscalls_summary(&insights.syscalls, insights.corpus.files);
    if !counted.is_empty() {
        counted.push(String::new());
        counted.extend(format_syscalls(&insights.syscalls, Style::Hidden));
    }
    match counted.is_empty() {
        true => lines.extend([
            "## System calls".to_string(),
            String::new(),
            insights
                .unmeasured
                .clone()
                .unwrap_or_else(|| NO_SYSCALLS.to_string()),
            String::new(),
        ]),
        false => lines.extend(wrap_in_fence("System calls", SYSCALLS_MEANS, counted)),
    }
    while lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    lines
}

pub fn get_floor_set_name(table: Table) -> String {
    format!("{FLOOR_PREFIX}{}", table.as_str())
}

pub fn format_floor_time(mean_s: f64, stddev_s: f64) -> String {
    if mean_s <= 0.0 {
        return String::new();
    }
    let text = format!("{:.1} ms", mean_s * 1000.0);
    if stddev_s > 0.0 {
        format!("{text} ± {:.1}", stddev_s * 1000.0)
    } else {
        text
    }
}

pub fn format_floor(measurements: &[Measurement], instances: &[(String, String)]) -> Vec<String> {
    let of_command = |command: &str| {
        measurements
            .iter()
            .find(|m| m.set == VERSION_SET && m.command == command)
    };
    let of_set = |set: &str, instance: &str| {
        measurements
            .iter()
            .find(|m| m.set == set && m.instance == instance)
    };
    let describe = |found: Option<&Measurement>| {
        found.map_or_else(String::new, |m| format_floor_time(m.mean_s, m.stddev_s))
    };
    let headings: Vec<String> = FIRST_HEADINGS
        .iter()
        .map(|heading| (*heading).to_string())
        .chain(TABLES.map(|table| format!("ready {}", table.as_str())))
        .collect();
    let mut rows: Vec<Vec<String>> = Vec::new();
    for (instance, command) in instances {
        let version = of_command(command);
        let mut row = vec![instance.clone(), describe(version)];
        row.extend(TABLES.map(|table| describe(of_set(&get_floor_set_name(table), instance))));
        rows.push(row);
    }
    let widths = measure_widths(&headings, &rows);
    let mut lines = Vec::new();
    lines.push(lay_out_row(&headings, &widths));
    lines.extend(rows.iter().map(|row| lay_out_row(row, &widths)));
    lines.push(String::new());
    for (label, means) in [
        (VERSION_HEADING.to_string(), VERSION_MEANS),
        (
            format!("ready {}, {}", TABLES[0].as_str(), TABLES[1].as_str()),
            READY_MEANS,
        ),
    ] {
        lines.push(format!(
            "{INDENT}{label:<width$}{means}",
            width = LABEL_WIDTH
        ));
    }
    lines
}

pub fn format_memory(curves: &[Curve], style: Style) -> Vec<String> {
    if curves.is_empty() {
        return Vec::new();
    }
    let mut lines = Vec::new();
    for curve in curves {
        lines.push(String::new());
        lines.push(format!(
            "{INDENT}{}   peak {}   {} samples, {} ms apart",
            curve.instance,
            format_bytes(curve.peak_bytes),
            curve.polls,
            format_spacing(curve.spacing_us)
        ));
        if curve.samples.len() < LEAST_SAMPLES {
            lines.push(format!(
                "{INDENT}the execution was too short to draw, it wants {LEAST_SAMPLES}"
            ));
            continue;
        }
        let top = get_axis_top(curve.peak_bytes);
        let gutter = (0..CHART_ROWS)
            .step_by(LABEL_EVERY)
            .map(|index| get_label(top, index).chars().count())
            .max()
            .unwrap_or_default();
        let columns = fold_into_columns(&curve.samples, CHART_COLUMNS);
        for (index, row) in draw_rows(&columns, top as f64, CHART_ROWS)
            .into_iter()
            .enumerate()
        {
            let edge = top * (CHART_ROWS - index) as u64 / CHART_ROWS as u64;
            let side = format!("{:>gutter$}", get_label(top, index));
            lines.push(format!(
                "{INDENT}{} \u{2502} {}",
                tint(&side, edge, style),
                tint(&row, edge, style)
            ));
        }
        let (axis, ticks) = lay_out_ticks(curve.wall_ms, CHART_COLUMNS * 2);
        lines.push(format!("{INDENT}{:>gutter$} {axis}", "0"));
        lines.push(format!("{INDENT}{:>gutter$} {ticks}", ""));
    }
    lines
}

pub fn format_syscalls_summary(counted: &[Syscalls], files: Option<u64>) -> Vec<String> {
    if counted.is_empty() {
        return Vec::new();
    }
    let headings: Vec<String> = SYSCALLS_HEADINGS[1..]
        .iter()
        .map(|heading| (*heading).to_string())
        .collect();
    let rows: Vec<Vec<String>> = counted
        .iter()
        .map(|counts| {
            vec![
                format_thousands(counts.total_calls),
                format_per_file(counts.total_calls, files),
                format_thousands(counts.total_errors),
            ]
        })
        .collect();
    let gutter = measure_gutter(
        SYSCALLS_HEADINGS[0],
        counted.iter().map(|counts| counts.instance.as_str()),
    );
    let widths = measure_columns(&headings, &rows);
    let mut lines = vec![lay_out_numbers(
        SYSCALLS_HEADINGS[0],
        &headings,
        gutter,
        &widths,
    )];
    for (counts, row) in counted.iter().zip(&rows) {
        lines.push(lay_out_numbers(&counts.instance, row, gutter, &widths));
    }
    lines
}

pub fn format_syscalls(counted: &[Syscalls], style: Style) -> Vec<String> {
    if counted.is_empty() {
        return Vec::new();
    }
    let names: Vec<String> = counted
        .iter()
        .map(|counts| counts.instance.clone())
        .collect();
    let mut table: Vec<(String, Vec<String>)> = vec![(SYSCALLS_HEADING.to_string(), names)];
    for family in FAMILIES
        .map(|(family, _)| family)
        .into_iter()
        .chain([OTHER_FAMILY])
    {
        let totals: Vec<u64> = counted
            .iter()
            .map(|counts| add_up_family(counts, family, &[]))
            .collect();
        if totals.iter().all(|total| *total == 0) {
            continue;
        }
        table.push((
            family.to_string(),
            totals.iter().copied().map(describe_count).collect(),
        ));
        let (shown, hidden): (Vec<String>, Vec<String>) = collect_names(counted, family)
            .into_iter()
            .partition(|name| is_shown(name, counted));
        for name in &shown {
            let cells = counted
                .iter()
                .map(|counts| {
                    find_call(counts, name)
                        .map_or(String::new(), |call| format_thousands(call.calls))
                })
                .collect();
            table.push((format!("{MEMBER_INDENT}{name}"), cells));
        }
        if !hidden.is_empty() && !shown.is_empty() {
            let cells = counted
                .iter()
                .map(|counts| describe_count(add_up_family(counts, family, &shown)))
                .collect();
            table.push((
                format!("{MEMBER_INDENT}{REST_LABEL} ({})", hidden.len()),
                cells,
            ));
        }
    }
    let gutter = measure_gutter("", table.iter().map(|(label, _)| label.as_str()));
    let rows: Vec<Vec<String>> = table
        .iter()
        .skip(1)
        .map(|(_, cells)| cells.clone())
        .collect();
    let widths = measure_columns(&table[0].1, &rows);
    table
        .into_iter()
        .map(|(label, cells)| {
            let line = lay_out_numbers(&label, &cells, gutter, &widths);
            if label.starts_with(MEMBER_INDENT) {
                fade(&line, style)
            } else {
                line
            }
        })
        .collect()
}

/// The tables are laid out for a terminal, so the page keeps them in a fence as they are.
fn wrap_in_fence(heading: &str, means: &str, body: Vec<String>) -> Vec<String> {
    if body.is_empty() {
        return Vec::new();
    }
    let mut lines = vec![
        format!("## {heading}"),
        String::new(),
        means.to_string(),
        String::new(),
        "```".to_string(),
    ];
    lines.extend(body.into_iter().skip_while(String::is_empty));
    lines.push("```".to_string());
    lines.push(String::new());
    lines
}

fn get_label(top: u64, index: usize) -> String {
    if index.is_multiple_of(LABEL_EVERY) {
        format_bytes(top * (CHART_ROWS - index) as u64 / CHART_ROWS as u64)
    } else {
        String::new()
    }
}

fn lay_out_ticks(wall_ms: u64, width: usize) -> (String, String) {
    let places: Vec<usize> = (0..AXIS_TICKS)
        .map(|tick| tick * (width - 1) / (AXIS_TICKS - 1))
        .collect();
    let mut axis = String::from('\u{2514}');
    for place in 0..width {
        axis.push(if places.contains(&place) {
            '\u{252c}'
        } else {
            '\u{2500}'
        });
    }
    let mut labels = String::new();
    for (tick, place) in places.iter().enumerate() {
        let at = wall_ms * tick as u64 / (AXIS_TICKS - 1) as u64;
        let text = if tick + 1 == AXIS_TICKS {
            format!("{at} ms")
        } else {
            at.to_string()
        };
        let start = (place + 1).saturating_sub(text.chars().count() / 2);
        let written = labels.chars().count();
        if written > start {
            labels.push(' ');
        } else {
            labels.push_str(&" ".repeat(start - written));
        }
        labels.push_str(&text);
    }
    (axis, labels)
}

fn draw_rows(columns: &[u64], top: f64, rows: usize) -> Vec<String> {
    let mut drawn = Vec::new();
    for row in (0..rows).rev() {
        let mut line = String::new();
        for value in columns {
            let eighths = if top > 0.0 {
                (*value as f64 / top * (rows * EIGHTHS) as f64).round() as usize
            } else {
                0
            };
            let base = row * EIGHTHS;
            line.push(if eighths >= base + EIGHTHS {
                BLOCKS[EIGHTHS - 1]
            } else if eighths <= base {
                ' '
            } else {
                BLOCKS[eighths - base - 1]
            });
            line.push(' ');
        }
        drawn.push(line.trim_end().to_string());
    }
    drawn
}

fn get_axis_top(peak: u64) -> u64 {
    LADDER
        .into_iter()
        .find(|rung| *rung >= peak)
        .unwrap_or_else(|| round_up(peak))
}

fn round_up(bytes: u64) -> u64 {
    let unit = pick_unit(bytes);
    let value = bytes as f64 / unit;
    if value <= 0.0 {
        return bytes;
    }
    let step = 10f64.powi(value.log10().floor() as i32 - 1);
    ((value / step).ceil() * step * unit) as u64
}

fn pick_unit(bytes: u64) -> f64 {
    let mut unit = KILOBYTE;
    while bytes as f64 >= unit * KILOBYTE {
        unit *= KILOBYTE;
    }
    unit
}

fn format_bytes(bytes: u64) -> String {
    let unit = pick_unit(bytes);
    let value = bytes as f64 / unit;
    let name = if unit >= KILOBYTE.powi(3) {
        "GB"
    } else if unit >= KILOBYTE.powi(2) {
        "MB"
    } else {
        "KB"
    };
    if value < 10.0 {
        format!("{value:.1} {name}")
    } else {
        format!("{value:.0} {name}")
    }
}

fn format_spacing(microseconds: u64) -> String {
    format!("{:.1}", microseconds as f64 / MICROSECONDS_IN_MS)
}

fn pick_color(level: u64) -> [u8; 3] {
    let mut under = 0;
    for (index, rung) in LADDER.into_iter().enumerate() {
        if level <= rung {
            let along = (level - under) as f64 / (rung - under) as f64;
            let mix = |channel: usize| {
                let (from, to) = (
                    SHADES[index][channel] as f64,
                    SHADES[index + 1][channel] as f64,
                );
                (from + (to - from) * along).round() as u8
            };
            return [mix(0), mix(1), mix(2)];
        }
        under = rung;
    }
    SHADES[SHADES.len() - 1]
}

fn tint(text: &str, level: u64, style: Style) -> String {
    if style != Style::Colored {
        return text.to_string();
    }
    let [red, green, blue] = pick_color(level);
    format!("\u{1b}[38;2;{red};{green};{blue}m{text}\u{1b}[0m")
}

fn collect_names(counted: &[Syscalls], family: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for counts in counted {
        for call in &counts.calls {
            if get_family(&call.name) == family && !names.contains(&call.name) {
                names.push(call.name.clone());
            }
        }
    }
    names.sort_by_key(|name| (Reverse(get_highest(counted, name)), name.clone()));
    names
}

fn get_highest(counted: &[Syscalls], name: &str) -> u64 {
    counted
        .iter()
        .filter_map(|counts| find_call(counts, name))
        .map(|call| call.calls)
        .max()
        .unwrap_or_default()
}

fn find_call<'a>(counts: &'a Syscalls, name: &str) -> Option<&'a Call> {
    counts.calls.iter().find(|call| call.name == name)
}

fn is_shown(name: &str, counted: &[Syscalls]) -> bool {
    counted.iter().any(|counts| {
        find_call(counts, name).is_some_and(|call| call.calls * SHOWN_PART >= counts.total_calls)
    })
}

fn add_up_family(counts: &Syscalls, family: &str, without: &[String]) -> u64 {
    counts
        .calls
        .iter()
        .filter(|call| get_family(&call.name) == family && !without.contains(&call.name))
        .map(|call| call.calls)
        .sum()
}

fn describe_count(count: u64) -> String {
    if count == 0 {
        return String::new();
    }
    format_thousands(count)
}

fn format_per_file(total: u64, files: Option<u64>) -> String {
    match files {
        Some(files) if files > 0 => format!("{:.1}", total as f64 / files as f64),
        _ => String::new(),
    }
}

fn lay_out_numbers(label: &str, cells: &[String], gutter: usize, widths: &[usize]) -> String {
    let mut line = format!("{INDENT}{label:<gutter$}");
    for (cell, width) in cells.iter().zip(widths) {
        line.push_str(&format!("{cell:>width$}"));
    }
    line.truncate(line.trim_end().len());
    line
}

fn measure_gutter<'a>(heading: &str, labels: impl Iterator<Item = &'a str>) -> usize {
    labels
        .map(|label| label.chars().count())
        .chain([heading.chars().count()])
        .max()
        .unwrap_or_default()
}

fn measure_columns(headings: &[String], rows: &[Vec<String>]) -> Vec<usize> {
    headings
        .iter()
        .enumerate()
        .map(|(column, heading)| {
            rows.iter()
                .filter_map(|row| row.get(column))
                .map(|cell| cell.chars().count())
                .chain([heading.chars().count()])
                .max()
                .unwrap_or_default()
                + COLUMN_GAP
        })
        .collect()
}

fn fade(text: &str, style: Style) -> String {
    if style != Style::Colored {
        return text.to_string();
    }
    format!("\u{1b}[2m{text}\u{1b}[0m")
}

fn lay_out_row(cells: &[String], widths: &[usize]) -> String {
    let mut line = String::from(INDENT);
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

fn measure_widths(headings: &[String], rows: &[Vec<String>]) -> Vec<usize> {
    let mut widths: Vec<usize> = headings
        .iter()
        .map(|heading| heading.chars().count())
        .collect();
    for row in rows {
        for (width, cell) in widths.iter_mut().zip(row) {
            *width = (*width).max(cell.chars().count());
        }
    }
    widths
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::fetch::{Identity, Origin};

    use super::*;

    const TOKEI_COMMAND: &str = "tokei --version";

    fn measured(
        set: &str,
        instance: &str,
        command: &str,
        mean_ms: f64,
        stddev_ms: f64,
    ) -> Measurement {
        Measurement {
            set: set.to_string(),
            instance: instance.to_string(),
            command: command.to_string(),
            mean_s: mean_ms / 1000.0,
            stddev_s: stddev_ms / 1000.0,
            median_s: mean_ms / 1000.0,
            min_s: mean_ms / 1000.0,
            max_s: mean_ms / 1000.0,
            user_s: 0.0,
            system_s: 0.0,
            runs: 30,
            counted_files: None,
            counted_lines: None,
            lines_per_sec: None,
            parallelism: None,
            lines_per_cpu_s: None,
        }
    }

    fn build_floor() -> Vec<Measurement> {
        vec![
            measured(VERSION_SET, "tokei", TOKEI_COMMAND, 6.1, 0.4),
            measured("floor-t1", "tokei", "tokei floor -t Rust", 13.3, 1.2),
            measured("floor-t2", "tokei", "tokei floor", 13.9, 1.1),
        ]
    }

    fn only_tokei() -> Vec<(String, String)> {
        vec![("tokei".to_string(), TOKEI_COMMAND.to_string())]
    }

    fn build_curve(instance: &str, peak_bytes: u64) -> Curve {
        let last = LEAST_SAMPLES as u64 - 1;
        Curve {
            instance: instance.to_string(),
            table: Table::SameWork.as_str().to_string(),
            step_ms: 2,
            spacing_us: 2500,
            polls: LEAST_SAMPLES,
            wall_ms: 100,
            peak_bytes,
            samples: (0..=last).map(|step| peak_bytes * step / last).collect(),
        }
    }

    fn build_syscalls(instance: &str, counted: &[(&str, u64)]) -> Syscalls {
        let calls: Vec<Call> = counted
            .iter()
            .map(|(name, calls)| Call {
                name: (*name).to_string(),
                calls: *calls,
                errors: 0,
            })
            .collect();
        Syscalls {
            instance: instance.to_string(),
            table: Table::SameWork.as_str().to_string(),
            wall_ms: 1000,
            total_calls: calls.iter().map(|call| call.calls).sum(),
            total_errors: 0,
            calls,
        }
    }

    fn build_insights(syscalls: Vec<Syscalls>) -> Insights {
        Insights {
            format: INSIGHTS_FORMAT,
            stamp: "20260911-005944".to_string(),
            date: "2026-09-11T00:59:44Z".to_string(),
            machine: Machine {
                platform: Platform::Linux,
                arch: "x86_64".to_string(),
                os: "Debian GNU/Linux 13".to_string(),
                kernel: "6.12".to_string(),
                cpu: "a cpu".to_string(),
                logical_cores: 16,
                ram_bytes: None,
                cpu_scaling: "performance".to_string(),
                corpus_fs: "ext4".to_string(),
                corpus_device: "nvme0".to_string(),
                global_gitignore: "none".to_string(),
                linebench: "0.1.0".to_string(),
                hyperfine: "hyperfine 1.19.0".to_string(),
            },
            defender: DefenderState {
                realtime: "not applicable".to_string(),
                counters: BTreeMap::new(),
            },
            unequal_exclusions: None,
            corpus: CorpusRecord {
                name: "linux".to_string(),
                checkout: PathBuf::from("/bench/linux"),
                commit: "0".repeat(40),
                head: Some("0".repeat(40)),
                clean: Some(true),
                pinned: true,
                extensions: vec!["c".to_string()],
                files: Some(100),
            },
            instances: vec![InstanceRecord {
                identity: Identity {
                    instance: "tokei".to_string(),
                    counter: "tokei".to_string(),
                    binary: PathBuf::from("/tools/tokei"),
                    sha256: "0".repeat(64),
                    version: "tokei 15.0.0".to_string(),
                    origin: Origin::Built {
                        version: "15.0.0".to_string(),
                        built_with: "rustc".to_string(),
                    },
                },
                languages: Vec::new(),
                same_work: Vec::new(),
                same_work_note: String::new(),
                scrub_env: Vec::new(),
                args: Vec::new(),
            }],
            floor: build_floor(),
            curves: vec![build_curve("tokei", 30 * MEGABYTE)],
            tracer: None,
            unmeasured: Some("strace runs on linux alone".to_string()),
            syscalls,
        }
    }

    fn read_colors(lines: &[String]) -> Vec<[u8; 3]> {
        lines
            .iter()
            .flat_map(|line| line.split("38;2;").skip(1))
            .filter_map(|rest| rest.split_once('m'))
            .filter_map(|(code, _)| {
                let mut channels = code.split(';').filter_map(|value| value.parse().ok());
                Some([channels.next()?, channels.next()?, channels.next()?])
            })
            .collect()
    }

    #[test]
    fn the_page_closes_every_fence_and_writes_none_around_a_table_that_was_never_measured() {
        let bare = build_insights_page(&build_insights(Vec::new()), &only_tokei()).join("\n");
        assert_eq!(bare.matches("```").count(), 4, "{bare}");
        assert!(
            bare.contains("measured 2026-09-11 00:59 UTC by linebench 0.1.0"),
            "{bare}"
        );
        assert!(
            bare.contains("linux corpus at `000000000` on ext4"),
            "{bare}"
        );
        assert!(bare.contains("tokei 15.0.0"), "{bare}");
        assert!(
            bare.contains("## Floor") && bare.contains("## Memory"),
            "{bare}"
        );
        assert!(
            bare.contains("## System calls\n\nstrace runs on linux alone"),
            "{bare}"
        );
        let mut silent = build_insights(Vec::new());
        silent.unmeasured = None;
        let silent = build_insights_page(&silent, &only_tokei()).join("\n");
        assert!(
            silent.contains(&format!("## System calls\n\n{NO_SYSCALLS}")),
            "{silent}"
        );
        let counted = build_insights_page(
            &build_insights(vec![build_syscalls("tokei", &[("openat", 10)])]),
            &only_tokei(),
        )
        .join("\n");
        assert_eq!(counted.matches("```").count(), 6, "{counted}");
        assert!(counted.contains("opening"), "{counted}");
        assert!(!counted.contains(NO_SYSCALLS), "{counted}");
    }

    #[test]
    fn a_call_too_small_to_stand_on_its_own_is_folded_into_the_rest_of_its_family() {
        let counts = build_syscalls("tokei", &[("openat", 10_000), ("openat2", 5)]);
        let lines = format_syscalls(std::slice::from_ref(&counts), Style::Plain);
        assert!(
            !lines.iter().any(|line| line.contains("openat2")),
            "{lines:?}"
        );
        let rest = lines
            .iter()
            .find(|line| line.contains("rest (1)"))
            .expect("the small call is folded into a rest row");
        assert!(rest.ends_with('5'), "{rest}");
        let family = lines
            .iter()
            .find(|line| line.starts_with("   opening"))
            .expect("the family carries them both");
        assert!(family.ends_with("10,005"), "{family}");
    }

    #[test]
    fn a_call_one_counter_never_makes_leaves_that_counter_s_cell_empty() {
        let lines = format_syscalls(
            &[
                build_syscalls("tokei", &[("statx", 1_000)]),
                build_syscalls("cloc", &[("lstat", 2_000)]),
            ],
            Style::Plain,
        );
        let of = |name: &str| {
            lines
                .iter()
                .find(|line| line.contains(name))
                .unwrap_or_else(|| panic!("{name} has a row in {lines:?}"))
                .clone()
        };
        assert!(of("statx").ends_with("1,000"), "{}", of("statx"));
        assert!(of("lstat").ends_with("2,000"), "{}", of("lstat"));
    }

    #[test]
    fn the_summary_divides_the_calls_by_the_files_the_corpus_declares() {
        let counts = build_syscalls("scc", &[("openat", 1_000)]);
        let with_files = format_syscalls_summary(std::slice::from_ref(&counts), Some(500));
        assert_eq!(
            with_files[1].split_whitespace().collect::<Vec<&str>>(),
            ["scc", "1,000", "2.0", "0"]
        );
        let without = format_syscalls_summary(&[counts], None);
        assert_eq!(
            without[1].split_whitespace().collect::<Vec<&str>>(),
            ["scc", "1,000", "0"]
        );
    }

    #[test]
    fn a_floor_set_is_never_read_as_one_of_the_run_s_tables() {
        assert_eq!(Table::of_set(VERSION_SET), None);
        for table in TABLES {
            assert_eq!(Table::of_set(&get_floor_set_name(table)), None);
        }
    }

    #[test]
    fn the_numbers_of_a_session_sit_under_their_own_directory_by_corpus_and_platform() {
        let out = Path::new("results");
        let stamp = "20260909-104000";
        assert_eq!(
            build_insights_path(out, "linux", Platform::Windows, stamp, false),
            Path::new("results/insights/linux/windows/20260909-104000")
        );
        assert_eq!(
            build_insights_path(out, "linux", Platform::Windows, stamp, true),
            Path::new("results/insights/local/linux/windows/20260909-104000")
        );
    }

    #[test]
    fn the_floor_is_printed_in_tenths_of_a_millisecond() {
        assert_eq!(format_floor_time(0.0057, 0.0003), "5.7 ms ± 0.3");
        assert_eq!(format_floor_time(0.0061, 0.0), "6.1 ms");
        assert_eq!(format_floor_time(0.0, 0.0003), "");
    }

    #[test]
    fn a_column_stands_at_the_height_of_its_share_of_the_axis() {
        assert_eq!(
            draw_rows(&[100, 50, 6, 0], 100.0, 2),
            ["\u{2588}", "\u{2588} \u{2588} \u{2581}"]
        );
    }

    #[test]
    fn the_axis_carries_a_time_at_every_tick_and_the_last_one_sits_on_the_end_of_the_line() {
        let (axis, labels) = lay_out_ticks(400, 20);
        assert_eq!(axis.chars().count(), 21);
        assert_eq!(
            axis.chars().filter(|c| *c == '\u{252c}').count(),
            AXIS_TICKS
        );
        assert!(labels.starts_with(" 0  100"), "{labels}");
        let last = labels.rfind("400 ms").expect("the wall closes the axis");
        assert_eq!(last + "400 ms".len() / 2, axis.chars().count() - 1);
    }

    #[test]
    fn the_side_of_the_chart_carries_the_top_of_the_axis_and_its_thirds() {
        let lines = format_memory(&[build_curve("scc", 400_000_000)], Style::Plain);
        let sides: Vec<String> = lines
            .iter()
            .filter_map(|line| line.split_once('\u{2502}'))
            .map(|(side, _)| side.trim().to_string())
            .collect();
        assert_eq!(sides, ["500 MB", "", "333 MB", "", "167 MB", ""]);
    }

    #[test]
    fn every_row_of_a_chart_hangs_its_bars_at_the_same_column() {
        let lines = format_memory(&[build_curve("cloc", 7 * MEGABYTE)], Style::Plain);
        let places: Vec<usize> = lines
            .iter()
            .filter_map(|line| line.find('\u{2502}'))
            .collect();
        assert_eq!(places.len(), CHART_ROWS);
        assert!(
            places.windows(2).all(|pair| pair[0] == pair[1]),
            "{places:?}"
        );
    }

    #[test]
    fn the_axis_top_is_the_first_rung_of_the_ladder_that_fits_the_peak() {
        assert_eq!(get_axis_top(200 * MEGABYTE), 200 * MEGABYTE);
        assert_eq!(get_axis_top(200 * MEGABYTE + 1), 500 * MEGABYTE);
        assert_eq!(get_axis_top(3000 * MEGABYTE), round_up(3000 * MEGABYTE));
    }

    #[test]
    fn a_chart_is_drawn_the_same_whoever_else_ran_and_its_colours_come_from_the_bytes() {
        let small = build_curve("mezura", 127 * MEGABYTE);
        let alone = format_memory(std::slice::from_ref(&small), Style::Colored);
        let beside = format_memory(&[small, build_curve("scc", 415 * MEGABYTE)], Style::Colored);
        let bars = |lines: &[String]| -> Vec<String> {
            lines
                .iter()
                .filter(|line| line.contains('\u{2502}'))
                .take(CHART_ROWS)
                .cloned()
                .collect()
        };
        assert_eq!(bars(&alone), bars(&beside));
        assert!(alone.iter().any(|line| line.contains("200 MB")));
        assert!(beside.iter().any(|line| line.contains("500 MB")));
        assert_eq!(read_colors(&alone)[0], SHADES[5]);
    }

    #[test]
    fn the_top_of_the_axis_is_the_peak_rounded_up_in_the_unit_it_is_printed_in() {
        assert_eq!(format_bytes(195 * 1024 * 1024), "195 MB");
        assert_eq!(format_bytes(round_up(195 * 1024 * 1024)), "200 MB");
        assert_eq!(format_bytes(round_up(1234 * 1024 * 1024)), "1.3 GB");
        assert_eq!(format_bytes(812 * 1024), "812 KB");
    }

    #[test]
    fn two_instances_of_one_binary_both_carry_its_version_answer() {
        let mut floor = build_floor();
        floor.push(measured(
            "floor-t1",
            "tokei@fast",
            "tokei floor -t Rust --fast",
            12.0,
            0.9,
        ));
        let instances = vec![
            ("tokei".to_string(), TOKEI_COMMAND.to_string()),
            ("tokei@fast".to_string(), TOKEI_COMMAND.to_string()),
        ];
        let lines = format_floor(&floor, &instances);
        assert_eq!(
            lines.iter().filter(|line| line.contains("6.1 ms")).count(),
            2
        );
        let row = lines
            .iter()
            .find(|line| line.contains("tokei@fast"))
            .expect("the second instance has a row");
        assert!(row.ends_with("12.0 ms ± 0.9"), "{row}");
    }

    #[test]
    fn a_set_hyperfine_failed_on_leaves_its_cells_empty() {
        let mut floor = build_floor();
        floor.retain(|m| m.set != "floor-t2");
        let lines = format_floor(&floor, &only_tokei());
        let row = lines
            .iter()
            .find(|line| line.contains("tokei"))
            .expect("the counter has a row");
        assert!(row.ends_with("13.3 ms ± 1.2"), "{row}");
    }
}
