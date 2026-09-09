use crate::measure::TABLES;
use crate::measure::Table;
use crate::record::Measurement;

pub const FLOOR_WARMUP: u32 = 5;
pub const FLOOR_RUNS: u32 = 30;
pub const VERSION_SET: &str = "floor-version";
const FLOOR_PREFIX: &str = "floor-";
const VERSION_HEADING: &str = "--version";
const FIRST_HEADINGS: [&str; 2] = ["instance", VERSION_HEADING];
const VERSION_MEANS: &str = "the binary answering its version flag and quitting";
const READY_MEANS: &str = "the same binary over a target with no files, its report printed";
const LABEL_WIDTH: usize = 16;
const INDENT: &str = "   ";
const COLUMN_GAP: usize = 2;

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
    let mut lines = Vec::new();
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

    #[test]
    fn a_floor_set_is_never_read_as_one_of_the_run_s_tables() {
        assert_eq!(Table::of_set(VERSION_SET), None);
        for table in TABLES {
            assert_eq!(Table::of_set(&get_floor_set_name(table)), None);
        }
    }

    #[test]
    fn the_floor_is_printed_in_tenths_of_a_millisecond() {
        assert_eq!(format_floor_time(0.0057, 0.0003), "5.7 ms ± 0.3");
        assert_eq!(format_floor_time(0.0061, 0.0), "6.1 ms");
        assert_eq!(format_floor_time(0.0, 0.0003), "");
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
