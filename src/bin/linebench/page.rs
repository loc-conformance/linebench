use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use linebench::corpus::{Parity, Verdict, format_percent, shorten_hash};
use linebench::defender::{ProcessExclusions, judge_process_exclusions};
use linebench::fetch::Origin;
use linebench::machine::Platform;
use linebench::machine::UNKNOWN;
use linebench::measure::Table;
use linebench::measure::{CONTROL_END, CONTROL_START, TABLES};
use linebench::record::RECORD_FILE;
use linebench::record::{
    InstanceRecord, Pooled, Record, calculate_drift, collect_table_rows,
    describe_empty_bare_counts, format_busy, format_relative, format_thousands, format_wall,
    read_record, shorten_version,
};

pub const PAGE_FILE: &str = "README.md";
pub const LOCAL_DIR: &str = "local";
const CONTROL_MOVED_THRESHOLD: f64 = 0.03;
const RUN_DEPTH: usize = 3;
const LONG_CONTEXT_VALUE: usize = 60;
const CONTEXT: [(&str, ReadContext); 11] = [
    ("power", |r| r.machine.cpu_scaling.clone()),
    ("prepared", |r| describe_list(&r.prepared)),
    ("background", |r| format_busy(r.background_busy_percent)),
    ("antivirus", describe_exclusions),
    ("realtime", |r| r.defender.realtime.clone()),
    ("hyperfine", |r| r.machine.hyperfine.clone()),
    ("kernel", |r| r.machine.kernel.clone()),
    ("os", |r| r.machine.os.clone()),
    ("drift", describe_drift),
    ("equal work", describe_parity),
    ("corpus clean", describe_clean),
];

pub struct FoundRun {
    pub record: Record,
    pub relative: String,
}

impl FoundRun {
    pub fn is_local(&self) -> bool {
        self.record
            .instances
            .iter()
            .any(|i| i.identity.is_a_local_build() || !i.args.is_empty())
    }
}

pub struct Collected {
    pub found: Vec<FoundRun>,
    pub skipped: Vec<String>,
}

pub fn collect_records(out_root: &Path) -> Collected {
    let mut found = Vec::new();
    let mut skipped = Vec::new();
    for base in [out_root.to_path_buf(), out_root.join(LOCAL_DIR)] {
        for dir in collect_run_dirs(&base, RUN_DEPTH) {
            let file = dir.join(RECORD_FILE);
            if !file.is_file() {
                continue;
            }
            let record = match read_record(&file) {
                Ok(record) => record,
                Err(refused) => {
                    skipped.push(format!("left out of the page: {refused}"));
                    continue;
                }
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
    Collected { found, skipped }
}

pub fn write_results_page(out_root: &Path, found: &[FoundRun]) -> Result<bool, String> {
    if found.is_empty() {
        return Ok(false);
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
            .filter(|f| !f.is_local() && f.record.stamp < entry.record.stamp)
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
        .map_err(|error| format!("{} could not be written: {error}", path.display()))?;
    Ok(true)
}

pub fn format_since(current: &Record, earlier: &[&Record]) -> Vec<String> {
    let mut comparable: Vec<&Record> = Vec::new();
    let mut set_aside: Vec<SetAside> = Vec::new();
    for record in earlier
        .iter()
        .copied()
        .filter(|record| record.stamp < current.stamp)
        .filter(|record| record.machine.platform == current.machine.platform)
        .filter(|record| record.corpus.name == current.corpus.name)
    {
        let reasons = find_hard_differences(record, current);
        if reasons.is_empty() {
            comparable.push(record);
        } else {
            set_aside.push(SetAside { record, reasons });
        }
    }
    let tables: Vec<(&Record, Vec<Pooled>)> = comparable
        .iter()
        .map(|record| {
            (
                *record,
                collect_table_rows(&record.measurements, Table::SameWork).0,
            )
        })
        .collect();
    let (now_rows, _) = collect_table_rows(&current.measurements, Table::SameWork);
    let mut never_measured = Vec::new();
    let mut compared: Vec<Comparison> = Vec::new();
    for now in &now_rows {
        let latest = tables
            .iter()
            .filter_map(|(record, rows)| {
                rows.iter()
                    .find(|row| row.instance == now.instance)
                    .map(|then| (*record, then.mean_s))
            })
            .max_by(|a, b| a.0.stamp.cmp(&b.0.stamp));
        match latest {
            Some((anchor, then)) => compared.push(Comparison { anchor, then, now }),
            None => never_measured.push(now.instance.as_str()),
        }
    }
    let mut anchors: Vec<&Record> = compared
        .iter()
        .map(|comparison| comparison.anchor)
        .collect();
    anchors.sort_by(|a, b| b.stamp.cmp(&a.stamp));
    anchors.dedup_by(|a, b| a.stamp == b.stamp);
    let Some(main) = anchors.first().copied() else {
        let mut lines = vec![
            if set_aside.is_empty() {
                "since: no earlier run on this machine over this corpus shares an instance with \
                 this one"
            } else {
                "since: no earlier run comparable with this one shares an instance with it"
            }
            .to_string(),
        ];
        lines.extend(describe_runs_set_aside(&set_aside));
        return lines;
    };
    let mut lines = vec![format!(
        "since {} ({})",
        main.stamp,
        if current.corpus.head.is_some() {
            "same machine, same corpus commit"
        } else {
            "same machine; the corpus is not a git checkout, so whether it changed is unknown"
        }
    )];
    for Comparison { anchor, then, now } in &compared {
        let before = find_instance_record(anchor, &now.instance);
        let after = find_instance_record(current, &now.instance);
        let same_flags = before.map(|i| (&i.languages, &i.same_work, &i.args))
            == after.map(|i| (&i.languages, &i.same_work, &i.args));
        let mut line = if same_flags {
            format!(
                "  {:<14} t1   {} -> {}   {}",
                now.instance,
                format_wall(*then, 0.0),
                format_wall(now.mean_s, 0.0),
                format_change(*then, now.mean_s)
            )
        } else {
            format!(
                "  {:<14} t1   the languages, the same-work flags or the instance's own \
                 arguments changed, so the times do not compare",
                now.instance
            )
        };
        if let (Some(before), Some(after)) = (before, after)
            && before.identity.sha256 != after.identity.sha256
        {
            line.push_str(&format!(
                "   build {} -> {}",
                shorten_hash(&before.identity.sha256),
                shorten_hash(&after.identity.sha256)
            ));
        }
        if anchor.stamp != main.stamp {
            line.push_str(&format!("   (from {})", anchor.stamp));
        }
        lines.push(line);
    }
    lines.extend(format_control_shift(current, &comparable, main));
    for anchor in &anchors {
        let differences = find_context_differences(anchor, current);
        if differences.is_empty() {
            continue;
        }
        if anchor.stamp != main.stamp {
            lines.push(format!("  differs from {}", anchor.stamp));
        }
        for (i, (label, then, now)) in differences.iter().enumerate() {
            let lead = if i == 0 && anchor.stamp == main.stamp {
                "differs"
            } else {
                ""
            };
            if then.len() + now.len() > LONG_CONTEXT_VALUE {
                lines.push(format!("  {lead:<14} {label:<14} was  {then}"));
                lines.push(format!("  {:<14} {:<14} now  {now}", "", ""));
            } else {
                lines.push(format!("  {lead:<14} {label:<14} {then} -> {now}"));
            }
        }
    }
    if !never_measured.is_empty() {
        lines.push(format!(
            "  {}   {}",
            if set_aside.is_empty() {
                "never measured before on this machine"
            } else {
                "not measured in any comparable earlier run"
            },
            never_measured.join(", ")
        ));
    }
    let mut absent_now: Vec<&str> = Vec::new();
    for anchor in &anchors {
        let Some((_, rows)) = tables
            .iter()
            .find(|(record, _)| record.stamp == anchor.stamp)
        else {
            continue;
        };
        for row in rows {
            if !now_rows.iter().any(|n| n.instance == row.instance)
                && !absent_now.contains(&row.instance.as_str())
            {
                absent_now.push(&row.instance);
            }
        }
    }
    if !absent_now.is_empty() {
        lines.push(format!(
            "  measured in the runs above and not in this run   {}",
            absent_now.join(", ")
        ));
    }
    lines.extend(describe_runs_set_aside(&set_aside));
    lines
}

struct Comparison<'a> {
    anchor: &'a Record,
    then: f64,
    now: &'a Pooled,
}

struct SetAside<'a> {
    record: &'a Record,
    reasons: Vec<String>,
}

type ReadContext = fn(&Record) -> String;

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
        .map(shorten_hash)
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
                "| {} | {wall} | {} | {:.2} s | {:.2} s | {:.2} | {} | {} | {} | {} |",
                row.instance,
                format_relative(row.relative, row.relative_stddev),
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
        let tolerance = format_percent(parity.tolerance);
        let mut bullet = match parity.judge() {
            Verdict::NotEqual {
                problems,
                nothing_compared,
            } => {
                let mut bullet = format!(
                    "- **Equal work**: **not equal, so the same-work table is not a comparison of equal work: {}.**",
                    problems.join("; ")
                );
                if let Some(why) = nothing_compared {
                    bullet.push_str(&format!(" **{why}.**"));
                }
                bullet
            }
            Verdict::NotCompared(why) => format!("- **Equal work**: **{why}.**"),
            Verdict::Equal {
                reference: Some(reference),
                instances: 1,
            } => format!(
                "- **Equal work**: the one file count sat within {tolerance} of the {} files the corpus declares; with one instance there is no line comparison.",
                format_thousands(reference)
            ),
            Verdict::Equal {
                reference: Some(reference),
                ..
            } => format!(
                "- **Equal work**: every file count sat within {tolerance} of the {} files the corpus declares, and the line counts within {tolerance} of each other.",
                format_thousands(reference)
            ),
            Verdict::Equal {
                reference: None, ..
            } => format!(
                "- **Equal work**: the file and line counts sat within {tolerance} of each other."
            ),
        };
        if !parity.missing.is_empty() {
            bullet.push_str(&format!(
                " **{} gave no readable counts and is not in this check.**",
                parity.missing.join(", ")
            ));
        }
        lines.push(bullet);
    }
    let empty_bare = describe_empty_bare_counts(&record.counts);
    if !empty_bare.is_empty() {
        lines.push(format!(
            "- **Out of the box**: **{}, so that bare time is of a run that did no work.**",
            empty_bare.join("; ")
        ));
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
    if !record.capture_failures.is_empty() {
        lines.push(format!(
            "- **Counters**: **{}, so those counts are missing or partial.**",
            record.capture_failures.join("; ")
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
                let mark = match find_instance_record(&entry.record, &r.instance)
                    .map(|i| &i.identity.origin)
                {
                    Some(Origin::Given { label }) => format!(" (local build {label})"),
                    _ => String::new(),
                };
                format!("{} {}{mark}", r.instance, format_wall(r.mean_s, 0.0))
            })
            .collect();
        lines.push(format!(
            "| {} | {} | {} | {} | {} |",
            entry.record.stamp,
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
        "- The machine is restarted and otherwise idle, and the run samples the system-wide cpu before it measures anything, which is the quiet machine trust check above. `linebench noise` answers the same question with five runs of the control, before committing to a run.".to_string(),
        twice,
        "- A corpus definition pins a commit, and a checkout on any other commit refuses to run. A run on an unpinned tree says so beside its corpus line.".to_string(),
        "- Counts come from each counter's own JSON output, and the file counts are checked against the count the corpus definition declares for its commit, which is the equal work trust check above.".to_string(),
        "- Same work: one language set for every counter, generated and minified files counted by all, gitignore obeyed by all, and every extra feature turned off. What each one turns off, as its definition declares it:".to_string(),
    ];
    for instance in &newest.instances {
        let note = if instance.same_work_note.is_empty() {
            "nothing to turn off".to_string()
        } else {
            instance.same_work_note.clone()
        };
        let with = if instance.args.is_empty() {
            String::new()
        } else {
            format!(", run with `{}`", instance.args.join(" "))
        };
        lines.push(format!("  - {}: {note}{with}", instance.identity.instance));
    }
    lines.extend([
        "- Out of the box: bare `counter <dir>`, nothing else, plus an instance's own arguments where it has them.".to_string(),
        "- The exact flags: each counter's definition under `counters/` in the linebench repository.".to_string(),
        String::new(),
        "## Terms".to_string(),
        String::new(),
        "- **wall**: how long a run takes on the clock, in milliseconds: the mean of all the timed runs, both command orders together, ± their σ. That σ holds the run-to-run noise plus half the gap between the two orders.".to_string(),
        "- **vs fastest**: this counter's wall divided by the fastest counter's wall in the same table, ± the σ of that ratio, worked out from the two walls' σ. A ratio whose interval reaches 1.00 is within the noise of the fastest.".to_string(),
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

fn format_control_shift(current: &Record, comparable: &[&Record], main: &Record) -> Vec<String> {
    let Some((name, sha, now_mean)) = find_control_mean(current) else {
        return vec![
            "  this run has no control measurement, so the machine's own shift is not known"
                .to_string(),
        ];
    };
    let mut same_name: Vec<(&Record, String, f64)> = comparable
        .iter()
        .filter_map(|record| {
            find_control_mean(record)
                .filter(|(then_name, _, _)| *then_name == name)
                .map(|(_, then_sha, then_mean)| (*record, then_sha, then_mean))
        })
        .collect();
    same_name.sort_by(|a, b| b.0.stamp.cmp(&a.0.stamp));
    let same_build = same_name
        .iter()
        .find(|(_, then_sha, _)| *then_sha == sha)
        .map(|(record, _, then_mean)| (*record, *then_mean));
    let mut lines = Vec::new();
    match (same_build, same_name.first()) {
        (Some((record, then_mean)), _) => {
            let mut line = format!(
                "  {:<14} {:<26} {}   the machine itself",
                "control",
                name,
                format_change(then_mean, now_mean)
            );
            if record.stamp != main.stamp {
                line.push_str(&format!("   (from {})", record.stamp));
            }
            lines.push(line);
            let moved = (now_mean / then_mean - 1.0).abs();
            if moved > CONTROL_MOVED_THRESHOLD {
                lines.push(format!(
                    "  the machine itself moved by {}, read the changes against that",
                    format_percent(moved)
                ));
            }
        }
        (None, Some((record, _, _))) => lines.push(format!(
            "  {:<14} {:<26} timed under another build in {}, so the machine's own shift is \
             not known",
            "control", name, record.stamp
        )),
        (None, None) => lines.push(
            "  no earlier run shares this run's control, so the machine's own shift is not known"
                .to_string(),
        ),
    }
    lines
}

fn find_hard_differences(then: &Record, now: &Record) -> Vec<String> {
    let mut reasons = Vec::new();
    if then.machine.cpu != now.machine.cpu
        || then.machine.logical_cores != now.machine.logical_cores
    {
        reasons.push(format!(
            "on {}, {} threads",
            then.machine.cpu, then.machine.logical_cores
        ));
    }
    if then.corpus.head != now.corpus.head {
        reasons.push(match &then.corpus.head {
            Some(head) => format!("at corpus commit {}", shorten_hash(head)),
            None => "with the corpus not a git checkout".to_string(),
        });
    }
    if !then.corpus.extensions.is_empty()
        && !now.corpus.extensions.is_empty()
        && then.corpus.extensions != now.corpus.extensions
    {
        reasons.push(format!(
            "counting {} files",
            then.corpus.extensions.join(", ")
        ));
    }
    reasons.extend(find_disk_difference(then, now));
    reasons
}

fn find_context_differences(previous: &Record, current: &Record) -> Vec<(String, String, String)> {
    CONTEXT
        .iter()
        .map(|(label, read)| (label.to_string(), read(previous), read(current)))
        .filter(|(_, then, now)| then != now)
        .collect()
}

fn find_control_mean(record: &Record) -> Option<(String, String, f64)> {
    let name = record.settings.control.clone();
    let sha = find_instance_record(record, &name)?.identity.sha256.clone();
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

fn find_instance_record<'a>(record: &'a Record, instance: &str) -> Option<&'a InstanceRecord> {
    record
        .instances
        .iter()
        .find(|i| i.identity.instance == instance)
}

fn find_disk_difference(then: &Record, now: &Record) -> Option<String> {
    let (fs_then, fs_now) = (&then.machine.corpus_fs, &now.machine.corpus_fs);
    let (device_then, device_now) = (&then.machine.corpus_device, &now.machine.corpus_device);
    let on_disk = || {
        let known: Vec<&str> = [fs_then.as_str(), device_then.as_str()]
            .into_iter()
            .filter(|part| *part != UNKNOWN)
            .collect();
        format!("with the corpus on {}", known.join(", "))
    };
    if fs_then != UNKNOWN && fs_now != UNKNOWN && fs_then != fs_now {
        return Some(on_disk());
    }
    if device_then == UNKNOWN || device_now == UNKNOWN {
        return (then.corpus.checkout != now.corpus.checkout)
            .then(|| format!("with the corpus at {}", then.corpus.checkout.display()));
    }
    (device_then != device_now).then(on_disk)
}

fn describe_runs_set_aside(set_aside: &[SetAside]) -> Vec<String> {
    let mut by_reason: BTreeMap<String, Vec<&str>> = BTreeMap::new();
    for SetAside { record, reasons } in set_aside {
        by_reason
            .entry(reasons.join(", "))
            .or_default()
            .push(record.stamp.as_str());
    }
    by_reason
        .into_iter()
        .map(|(reason, mut stamps)| {
            stamps.sort_unstable();
            let newest = stamps.last().copied().unwrap_or_default();
            match stamps.len() {
                1 => format!("  not compared with {newest}: that run was {reason}"),
                many => format!(
                    "  not compared with {many} earlier runs, the newest {newest}: those runs \
                     were {reason}"
                ),
            }
        })
        .collect()
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
    match judge_process_exclusions(&record.defender) {
        ProcessExclusions::NoCounters => "no counters".to_string(),
        ProcessExclusions::AllExcluded => {
            "every counter equally excluded from real-time scanning".to_string()
        }
        ProcessExclusions::NoneExcluded => "no counter excluded, all scanned equally".to_string(),
        ProcessExclusions::Unknown(answer) => format!(
            "whether the counters are excluded from scanning could not be read ({})",
            answer.as_str()
        ),
        ProcessExclusions::Unequal => {
            "**unequal exclusions, the results may not be representative of real performance**"
                .to_string()
        }
    }
}

fn describe_drift(record: &Record) -> String {
    calculate_drift(&record.measurements).map_or("n/a".to_string(), |d| format_percent(d - 1.0))
}

fn describe_parity(record: &Record) -> String {
    record
        .parity
        .as_ref()
        .map_or("not checked".to_string(), Parity::describe)
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::env;

    use linebench::counters::Channel;
    use linebench::defender::{Answer, DefenderState};
    use linebench::fetch::Identity;
    use linebench::machine::Machine;
    use linebench::measure::{FORWARD, REVERSE, get_set_name};
    use linebench::record::RECORD_FORMAT;
    use linebench::record::{CorpusRecord, Measurement, RunSettings};

    use super::*;

    #[test]
    fn each_instance_is_compared_with_its_own_latest_earlier_run() {
        let monday = build_record(
            "20260901-100000",
            &[("mezura", 0.30), ("scc", 0.50)],
            "nvme0",
        );
        let tuesday = build_record("20260902-100000", &[("mezura", 0.31)], "nvme0");
        let wednesday = build_record(
            "20260903-100000",
            &[("mezura", 0.32), ("scc", 0.55)],
            "nvme0",
        );
        let since = format_since(&wednesday, &[&monday, &tuesday]);
        assert_eq!(
            since[0],
            "since 20260902-100000 (same machine, same corpus commit)"
        );
        assert!(since[1].starts_with("  mezura         t1   310 ms -> 320 ms   +3.2%"));
        assert!(!since[1].contains("(from"), "{}", since[1]);
        assert!(since[2].starts_with("  scc            t1   500 ms -> 550 ms   +10.0%"));
        assert!(since[2].ends_with("(from 20260901-100000)"), "{}", since[2]);
        assert!(
            since[3].starts_with("  control        mezura"),
            "{}",
            since[3]
        );
        assert!(
            since[3].contains("+3.2%   the machine itself"),
            "{}",
            since[3]
        );
    }

    #[test]
    fn changed_languages_or_flags_make_the_times_incomparable() {
        let then = build_record("20260901-100000", &[("mezura", 0.30)], "nvme0");
        let mut now = build_record("20260902-100000", &[("mezura", 0.20)], "nvme0");
        now.instances[0].languages = vec!["--languages".to_string(), "c".to_string()];
        let since = format_since(&now, &[&then]);
        assert!(since[1].contains("do not compare"), "{}", since[1]);
        assert!(!since[1].contains("->"), "{}", since[1]);
    }

    #[test]
    fn a_corpus_on_another_disk_is_not_compared_and_an_unknown_disk_goes_by_the_path() {
        let other_disk = build_record("20260901-100000", &[("mezura", 0.30)], "sda");
        let unknown_disk = build_record("20260902-100000", &[("mezura", 0.31)], UNKNOWN);
        let now = build_record("20260903-100000", &[("mezura", 0.32)], "nvme0");
        let since = format_since(&now, &[&other_disk, &unknown_disk]);
        assert_eq!(
            since[0],
            "since 20260902-100000 (same machine, same corpus commit)"
        );
        assert!(
            since.iter().any(|line| line
                == "  not compared with 20260901-100000: that run was with the corpus on ext4, sda"),
            "{since:?}"
        );
        let mut moved = unknown_disk;
        moved.corpus.checkout = PathBuf::from("E:/linux");
        let alone = format_since(&now, &[&other_disk, &moved]);
        assert!(
            alone[0].starts_with("since: no earlier run"),
            "{}",
            alone[0]
        );
        assert_eq!(
            alone[0],
            "since: no earlier run comparable with this one shares an instance with it"
        );
        assert_eq!(
            alone[1],
            "  not compared with 20260902-100000: that run was with the corpus at E:/linux"
        );
        assert_eq!(
            alone[2],
            "  not compared with 20260901-100000: that run was with the corpus on ext4, sda"
        );
        let mut blind_fs = build_record("20260901-100000", &[("mezura", 0.30)], "sda");
        blind_fs.machine.corpus_fs = UNKNOWN.to_string();
        let since = format_since(&now, &[&blind_fs]);
        assert_eq!(
            since[1],
            "  not compared with 20260901-100000: that run was with the corpus on sda"
        );
    }

    #[test]
    fn a_long_context_value_is_printed_as_was_and_now_on_two_lines() {
        let mut then = build_record("20260901-100000", &[("mezura", 0.30)], "nvme0");
        then.prepared = vec![
            "cpu governor on 16 cpus: ondemand/powersave/schedutil -> performance".to_string(),
        ];
        let now = build_record("20260902-100000", &[("mezura", 0.31)], "nvme0");
        let since = format_since(&now, &[&then]);
        let was = since
            .iter()
            .position(|line| {
                line.starts_with("  differs        prepared       was  cpu governor on 16 cpus")
            })
            .unwrap_or_else(|| panic!("{since:?}"));
        assert_eq!(since[was + 1].trim_start(), "now  none");
        assert_eq!(since[was + 1].find("now"), since[was].find("was"));
    }

    #[test]
    fn runs_set_aside_for_one_reason_fold_into_one_line() {
        let mut earlier = Vec::new();
        for day in 1..=3 {
            let mut record = build_record(
                &format!("2026090{day}-100000"),
                &[("mezura", 0.30)],
                "nvme0",
            );
            record.corpus.head = Some("1".repeat(40));
            earlier.push(record);
        }
        let mut other_cpu = build_record("20260904-100000", &[("mezura", 0.30)], "nvme0");
        other_cpu.machine.cpu = "another cpu".to_string();
        let now = build_record("20260905-100000", &[("mezura", 0.32)], "nvme0");
        let refs: Vec<&Record> = earlier.iter().chain([&other_cpu]).collect();
        let since = format_since(&now, &refs);
        assert!(
            since[0].starts_with("since: no earlier run"),
            "{}",
            since[0]
        );
        assert_eq!(
            since[1],
            "  not compared with 3 earlier runs, the newest 20260903-100000: those runs were at \
             corpus commit 111111111"
        );
        assert_eq!(
            since[2],
            "  not compared with 20260904-100000: that run was on another cpu, 16 threads"
        );
        let mut rust_only = build_record("20260904-120000", &[("mezura", 0.30)], "nvme0");
        rust_only.corpus.extensions = vec!["rs".to_string()];
        let mut before_the_field = build_record("20260904-130000", &[("mezura", 0.30)], "nvme0");
        before_the_field.corpus.extensions.clear();
        let since = format_since(&now, &[&rust_only, &before_the_field]);
        assert!(
            since.iter().any(|line| line
                == "  not compared with 20260904-120000: that run was counting rs files"),
            "{since:?}"
        );
        assert!(
            since
                .iter()
                .any(|line| line.starts_with("since 20260904-130000")),
            "{since:?}"
        );
    }

    #[test]
    fn context_differences_are_listed_for_every_anchor() {
        let mut monday = build_record("20260901-100000", &[("scc", 0.50)], "nvme0");
        monday.machine.cpu_scaling = "High performance".to_string();
        let tuesday = build_record("20260902-100000", &[("mezura", 0.31)], "nvme0");
        let now = build_record(
            "20260903-100000",
            &[("mezura", 0.32), ("scc", 0.55)],
            "nvme0",
        );
        let since = format_since(&now, &[&monday, &tuesday]);
        let differs = since
            .iter()
            .position(|line| line == "  differs from 20260901-100000")
            .expect("a block for the older anchor");
        assert!(
            since[differs + 1].contains("power          High performance -> Balanced"),
            "{}",
            since[differs + 1]
        );
        assert!(
            !since.iter().any(|line| line.starts_with("  differs  ")),
            "{since:?}"
        );
    }

    #[test]
    fn a_control_timed_under_another_build_is_named_as_such() {
        let then = build_record("20260901-100000", &[("mezura", 0.30)], "nvme0");
        let mut now = build_record("20260902-100000", &[("mezura", 0.31)], "nvme0");
        now.instances[0].identity.sha256 = "f".repeat(64);
        let since = format_since(&now, &[&then]);
        assert!(
            since[1].contains("build mezura000 -> fffffffff"),
            "{}",
            since[1]
        );
        assert!(
            since[2].starts_with("  control        mezura")
                && since[2].contains("timed under another build in 20260901-100000"),
            "{}",
            since[2]
        );
    }

    #[test]
    fn a_release_section_never_anchors_on_a_local_run() {
        let release = build_record("20260901-100000", &[("mezura", 0.30)], "nvme0");
        let mut local = build_record("20260902-100000", &[("mezura", 0.31)], "nvme0");
        local.instances[0].identity.origin = Origin::Given {
            label: "dev".to_string(),
        };
        let latest = build_record("20260903-100000", &[("mezura", 0.32)], "nvme0");
        let out_root = env::temp_dir().join("linebench-a_release_section_never_anchors");
        let _ = fs::remove_dir_all(&out_root);
        fs::create_dir_all(&out_root).unwrap();
        let found: Vec<FoundRun> = [latest, local, release]
            .into_iter()
            .map(|record| FoundRun {
                relative: format!("linux/windows/{}", record.stamp),
                record,
            })
            .collect();
        assert!(write_results_page(&out_root, &found).unwrap());
        let page = fs::read_to_string(out_root.join(PAGE_FILE)).unwrap();
        fs::remove_dir_all(&out_root).unwrap();
        assert!(
            page.contains("since 20260901-100000 (same machine, same corpus commit)"),
            "{page}"
        );
        assert!(!page.contains("since 20260902-100000"), "{page}");
        assert!(page.contains("## Local builds"), "{page}");
    }

    fn build_record(stamp: &str, rows: &[(&str, f64)], device: &str) -> Record {
        let instances: Vec<InstanceRecord> = rows
            .iter()
            .map(|(name, _)| InstanceRecord {
                identity: Identity {
                    instance: name.to_string(),
                    counter: name.to_string(),
                    binary: PathBuf::from(format!("D:/counters/{name}.exe")),
                    sha256: format!("{name}0000000000"),
                    version: "1.0.0".to_string(),
                    origin: Origin::Fetched {
                        channel: Channel::GithubReleaseAsset,
                        version: "1.0.0".to_string(),
                        source: format!("{name}.zip"),
                    },
                },
                languages: vec!["--languages".to_string(), "c,h".to_string()],
                same_work: vec!["--plain".to_string()],
                same_work_note: String::new(),
                scrub_env: Vec::new(),
                args: Vec::new(),
            })
            .collect();
        let mut measurements = Vec::new();
        for (name, mean) in rows {
            for order in [FORWARD, REVERSE] {
                let set = get_set_name(Table::SameWork, order);
                measurements.push(build_measurement(&set, name, *mean));
            }
        }
        let (control, control_mean) = rows[0];
        for set in [CONTROL_START, CONTROL_END] {
            measurements.push(build_measurement(set, control, control_mean));
        }
        Record {
            format: RECORD_FORMAT,
            stamp: stamp.to_string(),
            date: String::new(),
            machine: Machine {
                platform: Platform::Windows,
                arch: "x86_64".to_string(),
                os: "Windows".to_string(),
                kernel: "10".to_string(),
                cpu: "a cpu".to_string(),
                logical_cores: 16,
                ram_bytes: None,
                cpu_scaling: "Balanced".to_string(),
                corpus_fs: "ext4".to_string(),
                corpus_device: device.to_string(),
                global_gitignore: "none".to_string(),
                hyperfine: "hyperfine 1.20.0".to_string(),
            },
            defender: DefenderState {
                realtime: "not applicable".to_string(),
                corpus_excluded: Answer::NotApplicable,
                counters: BTreeMap::new(),
            },
            background_busy_percent: Some(1.0),
            prepared: Vec::new(),
            corpus: CorpusRecord {
                name: "linux".to_string(),
                checkout: PathBuf::from("D:/linux"),
                commit: "0".repeat(40),
                head: Some("0".repeat(40)),
                clean: Some(true),
                pinned: true,
                extensions: vec!["c".to_string(), "h".to_string()],
            },
            settings: RunSettings {
                warmup: 3,
                runs: 15,
                settle: 3,
                instances: rows.iter().map(|(name, _)| name.to_string()).collect(),
                control: control.to_string(),
                unequal_exclusions: None,
            },
            instances,
            counts: Vec::new(),
            measurements,
            parity: None,
            hyperfine_failures: Vec::new(),
            hyperfine_warnings: Vec::new(),
            capture_failures: Vec::new(),
        }
    }

    fn build_measurement(set: &str, instance: &str, mean_s: f64) -> Measurement {
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
            system_s: 0.5,
            runs: 15,
            counted_files: Some(100),
            counted_lines: Some(1000),
            lines_per_sec: None,
            parallelism: Some(4.0),
            lines_per_cpu_s: None,
        }
    }
}
