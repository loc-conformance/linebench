use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use linebench::corpus::format_percent;
use linebench::fetch::Origin;
use linebench::machine::Platform;
use linebench::measure::{CONTROL_END, CONTROL_START, TABLES, Table};
use linebench::record::{
    RECORD_FILE, Record, calculate_drift, collect_table_rows, format_thousands, format_wall,
    read_record, shorten_version,
};

pub const PAGE_FILE: &str = "README.md";
pub const LOCAL_DIR: &str = "local";
const CONTROL_MOVED_THRESHOLD: f64 = 0.03;
const RUN_DEPTH: usize = 3;

pub struct FoundRun {
    pub record: Record,
    pub relative: String,
}

impl FoundRun {
    pub fn is_local(&self) -> bool {
        self.record
            .instances
            .iter()
            .any(|i| matches!(i.identity.origin, Origin::Given { .. }))
    }
}

pub fn collect_records(out_root: &Path) -> Vec<FoundRun> {
    let mut found = Vec::new();
    for base in [out_root.to_path_buf(), out_root.join(LOCAL_DIR)] {
        for dir in collect_run_dirs(&base, RUN_DEPTH) {
            let Ok(record) = read_record(&dir.join(RECORD_FILE)) else {
                continue;
            };
            let relative = dir
                .strip_prefix(out_root)
                .unwrap_or(&dir)
                .to_string_lossy()
                .replace('\\', "/");
            found.push(FoundRun { record, relative });
        }
    }
    found.sort_by(|a, b| b.record.stamp.cmp(&a.record.stamp));
    found
}

pub fn write_results_page(out_root: &Path) -> Result<(), String> {
    let found = collect_records(out_root);
    if found.is_empty() {
        return Ok(());
    }
    let mut lines = vec![
        "# Benchmark results".to_string(),
        String::new(),
        "Written by `linebench report` after every run, not edited by hand. What every term \
         means and how this was measured: the two sections at the bottom."
            .to_string(),
        String::new(),
    ];
    let mut latest: BTreeMap<(String, String), &FoundRun> = BTreeMap::new();
    for entry in found.iter().rev().filter(|f| !f.is_local()) {
        let key = (
            entry.record.corpus.name.clone(),
            entry.record.machine.platform.as_str().to_string(),
        );
        latest.insert(key, entry);
    }
    let mut shown: Vec<&FoundRun> = latest.into_values().collect();
    shown.sort_by(|a, b| b.record.stamp.cmp(&a.record.stamp));
    let mut single_order_seen = false;
    for entry in &shown {
        let earlier: Vec<&Record> = found
            .iter()
            .filter(|f| f.record.stamp < entry.record.stamp)
            .map(|f| &f.record)
            .collect();
        lines.extend(format_run_section(
            &entry.record,
            &earlier,
            &mut single_order_seen,
        ));
    }
    let release: Vec<&FoundRun> = found.iter().filter(|f| !f.is_local()).collect();
    if release.len() > shown.len() {
        lines.extend(format_every_run(&release));
    }
    let local: Vec<&FoundRun> = found.iter().filter(|f| f.is_local()).collect();
    if !local.is_empty() {
        lines.extend(format_local_runs(&local));
    }
    if let Some(newest) = shown.first().copied().or(found.first()) {
        lines.extend(format_methodology(&newest.record, single_order_seen));
    }
    let path = out_root.join(PAGE_FILE);
    let mut text = lines.join("\n");
    text.push('\n');
    fs::write(&path, text)
        .map_err(|error| format!("{} could not be written: {error}", path.display()))
}

pub fn format_since(current: &Record, earlier: &[&Record]) -> Vec<String> {
    let comparable = earlier
        .iter()
        .filter(|record| record.stamp < current.stamp)
        .filter(|record| {
            is_the_same_machine(record, current) && is_the_same_corpus(record, current)
        })
        .filter(|record| shares_an_instance(record, current))
        .max_by(|a, b| a.stamp.cmp(&b.stamp));
    let Some(previous) = comparable else {
        return vec![
            "since: no earlier run on this machine at this corpus commit shares an instance \
             with this one"
                .to_string(),
        ];
    };
    let mut lines = vec![format!(
        "since {} (same machine, same corpus commit)",
        previous.stamp
    )];
    let (now_rows, _) = collect_table_rows(&current.measurements, Table::SameWork);
    let (then_rows, _) = collect_table_rows(&previous.measurements, Table::SameWork);
    let mut absent_then = Vec::new();
    for row in &now_rows {
        let Some(then) = then_rows.iter().find(|r| r.instance == row.instance) else {
            absent_then.push(row.instance.as_str());
            continue;
        };
        let mut line = format!(
            "  {:<14} t1   {} -> {}   {}",
            row.instance,
            format_wall(then.mean_s, 0.0),
            format_wall(row.mean_s, 0.0),
            format_change(then.mean_s, row.mean_s)
        );
        if let (Some(before), Some(after)) = (
            find_identity(previous, &row.instance),
            find_identity(current, &row.instance),
        ) && before.sha256 != after.sha256
        {
            line.push_str(&format!(
                "   build {} -> {}",
                &before.sha256[..7],
                &after.sha256[..7]
            ));
        }
        lines.push(line);
    }
    match (find_control_mean(previous), find_control_mean(current)) {
        (Some((then_name, then_sha, then_mean)), Some((now_name, now_sha, now_mean)))
            if then_name == now_name && then_sha == now_sha =>
        {
            lines.push(format!(
                "  {:<14} {:<26} {}   the machine itself",
                "control",
                now_name,
                format_change(then_mean, now_mean)
            ));
            let moved = (now_mean / then_mean - 1.0).abs();
            if moved > CONTROL_MOVED_THRESHOLD {
                lines.push(format!(
                    "  the machine itself moved by {}, read the changes against that",
                    format_percent(moved)
                ));
            }
        }
        _ => lines.push(
            "  no shared control between the two runs, the differences below are all there is"
                .to_string(),
        ),
    }
    let differences = find_context_differences(previous, current);
    for (i, (label, then, now)) in differences.iter().enumerate() {
        let lead = if i == 0 { "differs" } else { "" };
        lines.push(format!("  {lead:<14} {label:<14} {then} -> {now}"));
    }
    if !absent_then.is_empty() {
        lines.push(format!(
            "  not in the earlier run   {}",
            absent_then.join(", ")
        ));
    }
    let absent_now: Vec<&str> = then_rows
        .iter()
        .filter(|r| !now_rows.iter().any(|n| n.instance == r.instance))
        .map(|r| r.instance.as_str())
        .collect();
    if !absent_now.is_empty() {
        lines.push(format!("  not in this run   {}", absent_now.join(", ")));
    }
    lines
}

fn format_run_section(
    record: &Record,
    earlier: &[&Record],
    single_order_seen: &mut bool,
) -> Vec<String> {
    let machine = &record.machine;
    let ram = machine.ram_bytes.map_or("RAM unknown".to_string(), |b| {
        format!("{:.0} GB usable RAM", b as f64 / 2f64.powi(30))
    });
    let head = record
        .corpus
        .head
        .as_deref()
        .map(|h| h.chars().take(9).collect::<String>())
        .unwrap_or_else(|| "no commit".to_string());
    let mut lines = vec![
        format!(
            "## {} corpus, {}, {}",
            record.corpus.name,
            describe_platform(machine.platform),
            record.stamp
        ),
        String::new(),
        format!(
            "{}, {} threads, {ram}, {}  ",
            machine.cpu, machine.logical_cores, machine.os
        ),
        format!(
            "corpus at `{head}` on {}, {}  ",
            machine.corpus_fs, machine.corpus_device
        ),
    ];
    if !record.corpus.pinned {
        lines.push("not pinned, measured as it stands  ".to_string());
    }
    lines.push(format!("{}  ", format_versions(record)));
    let runs = record.settings.runs;
    lines.push(format!(
        "{} warmups, {} timed runs per command ({runs} in the first pass + {runs} in the reverse pass), {} s of pause before each command",
        record.settings.warmup,
        runs * 2,
        record.settings.settle
    ));
    lines.push(String::new());
    let mut order_moves = Vec::new();
    let mut record_single = false;
    for table in TABLES {
        let (rows, moves) = collect_table_rows(&record.measurements, table);
        if rows.is_empty() {
            continue;
        }
        order_moves.extend(moves);
        let title = match table {
            Table::SameWork => {
                "Same work (every counter pinned to the same languages and settings)"
            }
            Table::OutOfTheBox => "Out of the box (each counter at its own defaults)",
        };
        lines.push(format!("#### {title}"));
        lines.push(String::new());
        lines.push("| counter | wall | vs fastest | user cpu | system cpu | parallelism | lines/s | lines per cpu second | files | lines |".to_string());
        lines.push("|---|---|---|---|---|---|---|---|---|---|".to_string());
        for row in rows {
            let mut wall = format_wall(row.mean_s, row.stddev_s);
            if row.single_order {
                *single_order_seen = true;
                record_single = true;
                wall.push_str(" (one order only)");
            }
            let cpu = row.user_s + row.system_s;
            let lines_per_sec = row
                .counted_lines
                .filter(|_| row.mean_s > 0.0)
                .map(|l| l as f64 / row.mean_s);
            let lines_per_cpu = row
                .counted_lines
                .filter(|_| cpu > 0.0)
                .map(|l| l as f64 / cpu);
            lines.push(format!(
                "| {} | {wall} | {:.2}x | {:.2} s | {:.2} s | {:.2} | {} | {} | {} | {} |",
                row.instance,
                row.relative,
                row.user_s,
                row.system_s,
                row.parallelism,
                lines_per_sec.map(format_millions).unwrap_or_default(),
                lines_per_cpu.map(format_millions).unwrap_or_default(),
                row.counted_files.map(format_thousands).unwrap_or_default(),
                row.counted_lines.map(format_thousands).unwrap_or_default(),
            ));
        }
        lines.push(String::new());
    }
    lines.push("Trust checks for this run:".to_string());
    if let Some(drift) = calculate_drift(&record.measurements) {
        lines.push(format!(
            "- **Machine steadiness**: the same binary, timed at the start of the run and again at the end, differed by {}.",
            format_percent(drift - 1.0)
        ));
    }
    if let Some(worst) = order_moves
        .iter()
        .cloned()
        .fold(None, |worst: Option<f64>, m| {
            Some(worst.map_or(m, |w| w.max(m)))
        })
    {
        let scope = if record_single {
            "the tables that ran in both command orders pool the two"
        } else {
            "every table ran in both command orders and the numbers above pool the two"
        };
        lines.push(format!(
            "- **Command order**: {scope}. Swapping the order moved no counter by more than {}.",
            format_percent(worst)
        ));
    }
    lines.push(if record.prepared.is_empty() {
        "- **Power**: the machine was measured as it was, with no settings changed.".to_string()
    } else {
        format!(
            "- **Power**: set for the run and restored after: {}.",
            record.prepared.join(", ")
        )
    });
    if let Some(busy) = record.background_busy_percent {
        let cores = machine.logical_cores;
        lines.push(format!(
            "- **Quiet machine**: everything other than the benchmark was using {busy}% of the cpu when the run started, about {:.1} of {cores} cores.",
            busy * cores as f64 / 100.0
        ));
    }
    if record.defender.realtime != "not applicable" && record.defender.realtime != "not checked" {
        lines.push(format!(
            "- **Antivirus**: real-time protection {}, {}.",
            record.defender.realtime,
            describe_exclusions(record)
        ));
    } else if record.defender.realtime == "not checked" {
        lines.push("- **Antivirus**: not checked under WSL, where a corpus on a Windows drive is still scanned by Windows Defender.".to_string());
    }
    if let Some(parity) = &record.parity {
        lines.push(if parity.problems.is_empty() {
            match parity.reference_files {
                Some(reference) => format!(
                    "- **Equal work**: every file count sat within {} of the corpus, {} tracked files carrying its extensions, and the line counts within {} of each other.",
                    format_percent(parity.tolerance),
                    format_thousands(reference),
                    format_percent(parity.tolerance)
                ),
                None => format!(
                    "- **Equal work**: the file and line counts sat within {} of each other.",
                    format_percent(parity.tolerance)
                ),
            }
        } else {
            format!(
                "- **Equal work**: **not equal, so the same-work table is not a comparison of equal work: {}.**",
                parity.problems.join("; ")
            )
        });
    }
    for warning in &record.hyperfine_warnings {
        let first = warning
            .split(". ")
            .next()
            .unwrap_or(warning)
            .trim_end_matches('.');
        lines.push(format!("- **hyperfine warning**: {first}."));
    }
    if !record.hyperfine_failures.is_empty() {
        lines.push(format!(
            "- **hyperfine**: **failed on {}, so those measurements are missing or partial.**",
            record.hyperfine_failures.join(", ")
        ));
    }
    lines.push(String::new());
    let since = format_since(record, earlier);
    if since.len() > 1 {
        lines.push("```".to_string());
        lines.extend(since);
        lines.push("```".to_string());
        lines.push(String::new());
    }
    lines
}

fn format_every_run(release: &[&FoundRun]) -> Vec<String> {
    let mut instances: BTreeSet<String> = BTreeSet::new();
    for entry in release {
        for row in collect_table_rows(&entry.record.measurements, Table::SameWork).0 {
            instances.insert(row.instance);
        }
    }
    let columns: Vec<String> = instances.into_iter().collect();
    let mut lines = vec![
        "## Every run".to_string(),
        String::new(),
        "Same-work times, the sections above show only the latest run per platform. Commits, machine state and everything else: inside each run's directory.".to_string(),
        String::new(),
        format!("| run | platform | corpus | {} | machine steadiness |", columns.join(" | ")),
        format!("|---|---|---|{}---|", "---|".repeat(columns.len())),
    ];
    for entry in release {
        let (rows, _) = collect_table_rows(&entry.record.measurements, Table::SameWork);
        let mut cells = vec![
            format!("[{}]({}/)", entry.record.stamp, entry.relative),
            describe_platform(entry.record.machine.platform).to_string(),
            entry.record.corpus.name.clone(),
        ];
        for column in &columns {
            cells.push(
                rows.iter()
                    .find(|r| &r.instance == column)
                    .map(|r| format_wall(r.mean_s, 0.0))
                    .unwrap_or_default(),
            );
        }
        cells.push(
            calculate_drift(&entry.record.measurements)
                .map(|d| format_percent(d - 1.0))
                .unwrap_or_default(),
        );
        lines.push(format!("| {} |", cells.join(" | ")));
    }
    lines.push(String::new());
    lines
}

fn format_local_runs(local: &[&FoundRun]) -> Vec<String> {
    let mut lines = vec![
        "## Local builds".to_string(),
        String::new(),
        "Runs holding a build that was given by hand rather than fetched. They compare one build with another on one machine and say nothing about the released counters.".to_string(),
        String::new(),
        "| run | platform | corpus | same-work times | machine steadiness |".to_string(),
        "|---|---|---|---|---|".to_string(),
    ];
    for entry in local {
        let (rows, _) = collect_table_rows(&entry.record.measurements, Table::SameWork);
        let times: Vec<String> = rows
            .iter()
            .map(|r| {
                let mark = match find_identity(&entry.record, &r.instance).map(|i| &i.origin) {
                    Some(Origin::Given { label }) => format!(" (local build {label})"),
                    _ => String::new(),
                };
                format!("{} {}{mark}", r.instance, format_wall(r.mean_s, 0.0))
            })
            .collect();
        lines.push(format!(
            "| [{}]({}/) | {} | {} | {} | {} |",
            entry.record.stamp,
            entry.relative,
            describe_platform(entry.record.machine.platform),
            entry.record.corpus.name,
            times.join(", "),
            calculate_drift(&entry.record.measurements)
                .map(|d| format_percent(d - 1.0))
                .unwrap_or_default()
        ));
    }
    lines.push(String::new());
    lines
}

fn format_methodology(newest: &Record, single_order_seen: bool) -> Vec<String> {
    let mut twice = "- Every table is measured twice, in one command order and then in the reverse. The numbers shown average the two, and how far they disagreed is printed in each run's trust checks.".to_string();
    if single_order_seen {
        twice.push_str(" A row marked \"one order only\" has just the one.");
    }
    let mut lines = vec![
        "## Methodology".to_string(),
        String::new(),
        "- hyperfine, with no shell in between. Each section above states its own warmups, timed runs and pause.".to_string(),
        "- The machine is restarted and otherwise idle, and the run samples the system-wide cpu before it measures anything, which is the quiet machine trust check above. `linebench noise` answers the same question in fifteen seconds, before committing to a run.".to_string(),
        twice,
        "- A corpus definition pins a commit, and a checkout on any other commit refuses to run. A run on an unpinned tree says so beside its corpus line.".to_string(),
        "- Counts come from each counter's own JSON output, and the file counts are checked against the tracked files of the corpus, which is the equal work trust check above.".to_string(),
        "- Same work: one language set for every counter, generated and minified files counted by all, gitignore obeyed by all, and every extra feature turned off. What each one turns off, as its definition declares it:".to_string(),
    ];
    for instance in &newest.instances {
        let note = if instance.same_work_note.is_empty() {
            "nothing to turn off".to_string()
        } else {
            instance.same_work_note.clone()
        };
        lines.push(format!("  - {}: {note}", instance.identity.instance));
    }
    lines.extend([
        "- Out of the box: bare `counter <dir>`, nothing else.".to_string(),
        "- The exact flags: each counter's definition under `counters/` in the linebench repository.".to_string(),
        String::new(),
        "## Terms".to_string(),
        String::new(),
        "- **wall**: how long a run takes on the clock, in milliseconds: the mean of all the timed runs, both command orders together, ± their σ. That σ holds the run-to-run noise plus half the gap between the two orders.".to_string(),
        "- **vs fastest**: this counter's wall divided by the fastest counter's wall in the same table.".to_string(),
        "- **user cpu**: cpu seconds spent running the counter's own code, summed over every thread. 16 threads busy for one second is 16 s.".to_string(),
        "- **system cpu**: cpu seconds spent inside the operating system on the counter's behalf, opening and reading files, plus whatever sits on that path (antivirus, filter drivers).".to_string(),
        "- **parallelism**: user plus system cpu, divided by wall: 4.6 s of cpu inside a 0.35 s run means 13 threads were busy on average.".to_string(),
        "- **lines/s**: the lines this counter itself counted, divided by its wall time.".to_string(),
        "- **lines per cpu second**: the lines this counter counted, divided by its user plus system cpu. How cheaply it counts, with the number of cores taken out of the picture.".to_string(),
        "- **files / lines**: what the counter reported counting. Under \"Same work\" every counter must nearly agree, and the equal work check says whether they did. Out of the box they differ by design.".to_string(),
        "- **machine steadiness**: the same binary timed at the start and at the end of the whole run. The percentage is how far apart the two means came out.".to_string(),
        String::new(),
    ]);
    lines
}

fn find_context_differences(previous: &Record, current: &Record) -> Vec<(String, String, String)> {
    let mut differences = Vec::new();
    let mut note = |label: &str, then: String, now: String| {
        if then != now {
            differences.push((label.to_string(), then, now));
        }
    };
    note(
        "power",
        previous.machine.cpu_scaling.clone(),
        current.machine.cpu_scaling.clone(),
    );
    note(
        "prepared",
        describe_list(&previous.prepared),
        describe_list(&current.prepared),
    );
    note(
        "background",
        format_busy(previous.background_busy_percent),
        format_busy(current.background_busy_percent),
    );
    note(
        "antivirus",
        describe_exclusions(previous),
        describe_exclusions(current),
    );
    note(
        "realtime",
        previous.defender.realtime.clone(),
        current.defender.realtime.clone(),
    );
    note(
        "hyperfine",
        previous.machine.hyperfine.clone(),
        current.machine.hyperfine.clone(),
    );
    note(
        "kernel",
        previous.machine.kernel.clone(),
        current.machine.kernel.clone(),
    );
    note(
        "os",
        previous.machine.os.clone(),
        current.machine.os.clone(),
    );
    note(
        "drift",
        calculate_drift(&previous.measurements)
            .map_or("n/a".to_string(), |d| format_percent(d - 1.0)),
        calculate_drift(&current.measurements)
            .map_or("n/a".to_string(), |d| format_percent(d - 1.0)),
    );
    note(
        "equal work",
        describe_parity(previous),
        describe_parity(current),
    );
    note(
        "corpus clean",
        describe_clean(previous),
        describe_clean(current),
    );
    differences
}

fn find_control_mean(record: &Record) -> Option<(String, String, f64)> {
    let name = record.settings.control.clone();
    let sha = find_identity(record, &name)?.sha256.clone();
    let mean_of = |set: &str| {
        record
            .measurements
            .iter()
            .find(|m| m.set == set && m.mean_s > 0.0)
            .map(|m| m.mean_s)
    };
    let (start, end) = (mean_of(CONTROL_START)?, mean_of(CONTROL_END)?);
    Some((name, sha, (start + end) / 2.0))
}

fn find_identity<'a>(record: &'a Record, instance: &str) -> Option<&'a linebench::fetch::Identity> {
    record
        .instances
        .iter()
        .map(|i| &i.identity)
        .find(|i| i.instance == instance)
}

fn is_the_same_machine(a: &Record, b: &Record) -> bool {
    a.machine.platform == b.machine.platform
        && a.machine.cpu == b.machine.cpu
        && a.machine.logical_cores == b.machine.logical_cores
}

fn is_the_same_corpus(a: &Record, b: &Record) -> bool {
    a.corpus.name == b.corpus.name && a.corpus.head == b.corpus.head
}

fn shares_an_instance(a: &Record, b: &Record) -> bool {
    a.settings
        .instances
        .iter()
        .any(|name| b.settings.instances.contains(name))
}

fn format_change(then: f64, now: f64) -> String {
    if then <= 0.0 {
        return String::new();
    }
    let change = (now / then - 1.0) * 100.0;
    format!("{change:+.1}%")
}

fn format_versions(record: &Record) -> String {
    record
        .instances
        .iter()
        .map(|i| match &i.identity.origin {
            Origin::Given { label } => {
                format!(
                    "{} {} (local build {label})",
                    i.identity.instance,
                    shorten_version(&i.identity.version)
                )
            }
            _ => format!(
                "{} {}",
                i.identity.instance,
                shorten_version(&i.identity.version)
            ),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_millions(value: f64) -> String {
    format!("{:.1}M", value / 1e6)
}

fn format_busy(busy: Option<f64>) -> String {
    busy.map_or("not sampled".to_string(), |b| format!("{b}% busy"))
}

fn describe_platform(platform: Platform) -> &'static str {
    match platform {
        Platform::Windows => "Windows",
        Platform::Linux => "Native Linux",
        Platform::Wsl => "WSL2",
        Platform::Macos => "macOS",
    }
}

fn describe_exclusions(record: &Record) -> String {
    if let Some(unequal) = &record.settings.unequal_exclusions {
        return format!(
            "**unequal MS Defender exclusions ({unequal}), measured with --allow-unequal-exclusions. The results may not be representative of real performance.**"
        );
    }
    let answers: Vec<&str> = record
        .defender
        .counters
        .values()
        .map(|e| e.process.as_str())
        .collect();
    match answers.as_slice() {
        [] => "no counters".to_string(),
        [first, rest @ ..] if rest.iter().all(|a| a == first) => match *first {
            "yes" => "every counter equally excluded from real-time scanning".to_string(),
            "no" => "no counter excluded, all scanned equally".to_string(),
            other => format!(
                "whether the counters are excluded from scanning could not be read ({other})"
            ),
        },
        _ => "**unequal exclusions, the results may not be representative of real performance**"
            .to_string(),
    }
}

fn describe_parity(record: &Record) -> String {
    match &record.parity {
        Some(parity) if parity.problems.is_empty() => "equal".to_string(),
        Some(parity) => parity.problems.join("; "),
        None => "not checked".to_string(),
    }
}

fn describe_clean(record: &Record) -> String {
    match record.corpus.clean {
        Some(true) => "clean".to_string(),
        Some(false) => "dirty".to_string(),
        None => "unknown".to_string(),
    }
}

fn describe_list(items: &[String]) -> String {
    if items.is_empty() {
        "none".to_string()
    } else {
        items.join(", ")
    }
}

fn collect_run_dirs(base: &Path, depth: usize) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(base) else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir() && path.file_name().is_some_and(|name| name != LOCAL_DIR))
        .collect();
    dirs.sort();
    if depth == 1 {
        return dirs;
    }
    dirs.iter()
        .flat_map(|dir| collect_run_dirs(dir, depth - 1))
        .collect()
}
