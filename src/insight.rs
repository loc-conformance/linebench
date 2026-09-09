use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::defender::DefenderState;
use crate::machine::{Machine, Platform};
use crate::measure::TABLES;
use crate::measure::{Style, Table};
use crate::record::LOCAL_DIR;
use crate::record::{CorpusRecord, InstanceRecord, Measurement};
use crate::sample::Curve;
use crate::sample::LEAST_SAMPLES;
use crate::sample::fold_into_columns;

pub const FLOOR_WARMUP: u32 = 5;
pub const FLOOR_RUNS: u32 = 30;
pub const VERSION_SET: &str = "floor-version";
pub const INSIGHTS_DIR: &str = "insights";
pub const INSIGHTS_FILE: &str = "insights.json";
pub const INSIGHTS_FORMAT: u32 = 2;
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
