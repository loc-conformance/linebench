use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{ChildStderr, ChildStdout, Command, Stdio};
use std::thread;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::counters::Definition;
use crate::fetch::Identity;
use crate::machine::Platform;
use crate::record::propagate_ratio_stddev;

pub const DEFAULT_WARMUP: u32 = 3;
pub const DEFAULT_RUNS: u32 = 15;
pub const DEFAULT_SETTLE: u32 = 3;
pub const OUT_DIR: &str = "out";
pub const CONTROL_START: &str = "control-start";
pub const CONTROL_END: &str = "control-end";
pub const FORWARD: &str = "fwd";
pub const REVERSE: &str = "rev";
pub const TABLES: [Table; 2] = [Table::SameWork, Table::OutOfTheBox];
const HYPERFINE: &str = "hyperfine";
pub const WARNING_LABEL: &str = "WARNING: ";
const WARNING_PREFIX: &str = "Warning:";
const ERROR_PREFIX: &str = "Error";
const ESCAPE: char = '\x1b';
const PLAIN_IN_A_COMMAND: &str = "_-./:@=+,";
const REPORT_INDENT: &str = "   ";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Table {
    SameWork,
    OutOfTheBox,
}

impl Table {
    pub fn as_str(self) -> &'static str {
        match self {
            Table::SameWork => "t1",
            Table::OutOfTheBox => "t2",
        }
    }

    pub fn describe(self) -> &'static str {
        match self {
            Table::SameWork => "Same work",
            Table::OutOfTheBox => "Out of the box",
        }
    }

    pub fn uses_same_work(self) -> bool {
        self == Table::SameWork
    }

    pub fn of_set(set: &str) -> Option<Table> {
        TABLES
            .into_iter()
            .find(|table| set.starts_with(table.as_str()))
    }
}

#[derive(Debug, Clone)]
pub struct Instance {
    pub definition: Definition,
    pub identity: Identity,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub execution_env: BTreeMap<String, String>,
}

impl Instance {
    pub fn get_name(&self) -> &str {
        &self.identity.instance
    }

    pub fn is_an_experiment(&self) -> bool {
        self.identity.is_a_local_build() || !self.args.is_empty() || !self.env.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub warmup: u32,
    pub runs: u32,
    pub settle: u32,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            warmup: DEFAULT_WARMUP,
            runs: DEFAULT_RUNS,
            settle: DEFAULT_SETTLE,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Hidden,
    Plain,
    Colored,
}

impl Style {
    pub fn as_flag(self) -> &'static str {
        match self {
            Style::Colored => "color",
            Style::Plain | Style::Hidden => "basic",
        }
    }

    pub fn is_shown(self) -> bool {
        self != Style::Hidden
    }
}

pub struct Capture {
    pub path: PathBuf,
    pub stderr: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimedCommand {
    pub instance: String,
    pub command: String,
    pub env: BTreeMap<String, String>,
    pub execution_env: BTreeMap<String, String>,
}

impl TimedCommand {
    pub fn describe_command(&self) -> String {
        if self.env.is_empty() {
            self.command.clone()
        } else {
            format!("{} {}", describe_env(&self.env), self.command)
        }
    }
}

pub struct Runner {
    pub failures: Vec<String>,
    pub warnings: Vec<String>,
    pub capture_failures: Vec<String>,
    pub commands: BTreeMap<String, String>,
    res: PathBuf,
    settings: Settings,
    platform: Platform,
    scrub: Vec<String>,
    style: Style,
}

impl Runner {
    pub fn new(
        res: &Path,
        settings: Settings,
        platform: Platform,
        scrub: Vec<String>,
        style: Style,
    ) -> Runner {
        Runner {
            failures: Vec::new(),
            warnings: Vec::new(),
            capture_failures: Vec::new(),
            commands: BTreeMap::new(),
            res: res.to_path_buf(),
            settings,
            platform,
            scrub,
            style,
        }
    }

    pub fn set_settings(&mut self, settings: Settings) {
        self.settings = settings;
    }

    pub fn run_hyperfine(
        &mut self,
        out: &mut dyn Write,
        name: &str,
        commands: &[TimedCommand],
    ) -> Result<(), String> {
        print_line(out, &format!(">> {name}"))?;
        let export = self.res.join(format!("{name}.json"));
        let markdown = self.res.join(format!("{name}.md"));
        let mut results = Vec::new();
        for (index, timed) in commands.iter().enumerate() {
            if index > 0 && self.style.is_shown() {
                print_line(out, "")?;
            }
            let _ = fs::remove_file(&export);
            let recorded = timed.describe_command();
            self.commands
                .insert(recorded.clone(), timed.instance.clone());
            self.run_one_hyperfine(out, name, timed, &export)?;
            take_json_results(&export, &recorded, &mut results);
        }
        if !results.is_empty()
            && let Ok(compact) = serde_json::to_string(&serde_json::json!({ "results": results }))
        {
            let _ = fs::write(&export, compact);
        }
        if let Some(table) = format_markdown_table(&results) {
            let _ = fs::write(&markdown, table);
        }
        Ok(())
    }

    pub fn capture_output(
        &mut self,
        out: &mut dyn Write,
        name: &str,
        program: &Path,
        args: &[String],
        env: &BTreeMap<String, String>,
        as_json: bool,
    ) -> Result<Capture, String> {
        let suffix = if as_json { "json" } else { "txt" };
        let path = self.res.join(OUT_DIR).join(format!("{name}.{suffix}"));
        let file = File::create(&path)
            .map_err(|error| format!("{} could not be created: {error}", path.display()))?;
        let mut command = Command::new(program);
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(file)
            .stderr(Stdio::piped());
        for name in &self.scrub {
            command.env_remove(name);
        }
        command.envs(env);
        let finished = command
            .output()
            .map_err(|error| format!("{} could not be run: {error}", program.display()))?;
        let stderr = String::from_utf8_lossy(&finished.stderr).into_owned();
        if !as_json
            && !finished.stderr.is_empty()
            && let Err(error) = OpenOptions::new()
                .append(true)
                .open(&path)
                .and_then(|mut file| file.write_all(&finished.stderr))
        {
            print_line(
                out,
                &format!(
                    "{WARNING_LABEL}what {name} printed on stderr could not be kept in {}: {error}",
                    path.display()
                ),
            )?;
        }
        if !finished.status.success() {
            let label = format!("{name}.{suffix}");
            let exit = finished
                .status
                .code()
                .map_or("by a signal".to_string(), |c| c.to_string());
            print_line(
                out,
                &format!(
                    "{WARNING_LABEL}{} exited {exit} while writing {label}",
                    program.display()
                ),
            )?;
            print_line(out, &format!("         {}", join_command(program, args)))?;
            for line in stderr.lines().filter(|line| !line.trim().is_empty()) {
                print_line(out, &format!("         {line}"))?;
            }
            if as_json {
                let program = program.file_name().map_or_else(
                    || program.display().to_string(),
                    |f| f.to_string_lossy().into_owned(),
                );
                self.capture_failures
                    .push(format!("{program} exited {exit} while writing {label}"));
            }
        }
        Ok(Capture { path, stderr })
    }

    fn run_one_hyperfine(
        &mut self,
        out: &mut dyn Write,
        name: &str,
        timed: &TimedCommand,
        export: &Path,
    ) -> Result<(), String> {
        let mut hyperfine = Command::new(HYPERFINE);
        hyperfine.args([
            "-N",
            "--style",
            self.style.as_flag(),
            "--warmup",
            &self.settings.warmup.to_string(),
            "--runs",
            &self.settings.runs.to_string(),
        ]);
        if self.settings.settle > 0 {
            hyperfine.args(["--setup", &build_sleep(self.platform, self.settings.settle)]);
        }
        hyperfine.args(["--export-json", &export.to_string_lossy()]);
        hyperfine.arg(&timed.command);
        for name in &self.scrub {
            hyperfine.env_remove(name);
        }
        hyperfine.envs(&timed.execution_env);
        let report = if self.style.is_shown() {
            Stdio::piped()
        } else {
            Stdio::null()
        };
        let mut child = hyperfine
            .stdin(Stdio::null())
            .stdout(report)
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("{HYPERFINE} could not be run: {error}"))?;
        let relayed = relay_report(out, child.stdout.take(), child.stderr.take(), timed);
        let status = child
            .wait()
            .map_err(|error| format!("{HYPERFINE} could not be waited for: {error}"))?;
        let stderr = relayed?;
        for warning in find_warnings(&stderr) {
            print_line(
                out,
                &format!("{WARNING_LABEL}{HYPERFINE} on {name}: {warning}"),
            )?;
            self.warnings.push(format!("{name}: {warning}"));
        }
        if !status.success() {
            let detail = find_error(&stderr);
            print_line(
                out,
                &format!("{WARNING_LABEL}{HYPERFINE} reported a problem on {name}: {detail}"),
            )?;
            if !self.failures.iter().any(|failed| failed == name) {
                self.failures.push(name.to_string());
            }
        }
        Ok(())
    }
}

pub fn capture_json_outputs(
    out: &mut dyn Write,
    runner: &mut Runner,
    instances: &[Instance],
    corpus: &Path,
    extensions: &[String],
) -> Result<(), String> {
    print_line(out, "\n== JSON captures (also the settling runs)")?;
    capture_every_instance(out, runner, instances, corpus, extensions, true)
}

pub fn run_phases(
    out: &mut dyn Write,
    runner: &mut Runner,
    instances: &[Instance],
    control: usize,
    corpus: &Path,
    extensions: &[String],
) -> Result<(), String> {
    let bare = build_command(&instances[control], corpus, extensions, Table::OutOfTheBox)?;
    print_line(out, "\n== opening control run")?;
    runner.run_hyperfine(out, CONTROL_START, std::slice::from_ref(&bare))?;
    for table in TABLES {
        print_line(out, &format!("\n== {}", table.describe()))?;
        let commands: Vec<TimedCommand> = instances
            .iter()
            .map(|instance| build_command(instance, corpus, extensions, table))
            .collect::<Result<_, _>>()?;
        runner.run_hyperfine(out, &get_set_name(table, FORWARD), &commands)?;
        print_line(out, "")?;
        let reversed: Vec<TimedCommand> = commands.into_iter().rev().collect();
        runner.run_hyperfine(out, &get_set_name(table, REVERSE), &reversed)?;
    }
    print_line(out, "\n== closing control run")?;
    runner.run_hyperfine(out, CONTROL_END, std::slice::from_ref(&bare))
}

pub fn capture_plain_output(
    out: &mut dyn Write,
    runner: &mut Runner,
    instances: &[Instance],
    corpus: &Path,
    extensions: &[String],
) -> Result<(), String> {
    print_line(out, "\n== plain output captures, kept beside the record")?;
    if let Err(refused) = capture_every_instance(out, runner, instances, corpus, extensions, false)
    {
        print_line(
            out,
            &format!("{WARNING_LABEL}the plain output captures stopped: {refused}"),
        )?;
    }
    Ok(())
}

pub fn get_capture_name(table: Table, instance: &str) -> String {
    format!("{}-{instance}", table.as_str())
}

pub fn get_set_name(table: Table, order: &str) -> String {
    format!("{}-{order}", table.as_str())
}

pub fn build_args(
    instance: &Instance,
    corpus: &Path,
    extensions: &[String],
    table: Table,
    as_json: bool,
) -> Result<Vec<String>, String> {
    instance.definition.build_args(
        corpus,
        extensions,
        &instance.args,
        table.uses_same_work(),
        as_json,
    )
}

pub fn build_command(
    instance: &Instance,
    corpus: &Path,
    extensions: &[String],
    table: Table,
) -> Result<TimedCommand, String> {
    let args = build_args(instance, corpus, extensions, table, false)?;
    Ok(TimedCommand {
        instance: instance.get_name().to_string(),
        command: join_command(&instance.identity.binary, &args),
        env: instance.env.clone(),
        execution_env: instance.execution_env.clone(),
    })
}

/// An instance's env as it would be typed, `NAME=value` pairs with a space between them.
pub fn describe_env(env: &BTreeMap<String, String>) -> String {
    env.iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<String>>()
        .join(" ")
}

pub fn join_command(binary: &Path, args: &[String]) -> String {
    let mut parts = vec![quote(&binary.to_string_lossy())];
    parts.extend(args.iter().map(|arg| quote(arg)));
    parts.join(" ")
}

pub fn quote(part: &str) -> String {
    let part = if cfg!(windows) {
        part.replace('\\', "/")
    } else {
        part.to_string()
    };
    let is_plain = |c: char| c.is_ascii_alphanumeric() || PLAIN_IN_A_COMMAND.contains(c);
    if part.starts_with('"') || part.chars().all(is_plain) {
        return part;
    }
    format!("\"{}\"", part.replace('\\', "\\\\").replace('"', "\\\""))
}

pub fn find_warnings(stderr: &str) -> Vec<String> {
    stderr
        .lines()
        .map(|line| strip_ansi(line).trim().to_string())
        .filter_map(|line| {
            line.strip_prefix(WARNING_PREFIX)
                .map(|rest| rest.trim().to_string())
        })
        .collect()
}

pub fn strip_ansi(text: &str) -> String {
    let mut plain = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == ESCAPE && chars.peek() == Some(&'[') {
            chars.next();
            for inside in chars.by_ref() {
                if inside.is_ascii_alphabetic() {
                    break;
                }
            }
            continue;
        }
        plain.push(c);
    }
    plain
}

pub fn print_line(out: &mut dyn Write, message: &str) -> Result<(), String> {
    writeln!(out, "{message}")
        .map_err(|error| format!("this report could not be written: {error}"))?;
    out.flush()
        .map_err(|error| format!("this report could not be written: {error}"))
}

fn relay_report(
    out: &mut dyn Write,
    report: Option<ChildStdout>,
    stderr: Option<ChildStderr>,
    timed: &TimedCommand,
) -> Result<String, String> {
    thread::scope(|scope| {
        let collected = scope.spawn(move || {
            let mut bytes = Vec::new();
            if let Some(mut stderr) = stderr {
                let _ = stderr.read_to_end(&mut bytes);
            }
            String::from_utf8_lossy(&bytes).into_owned()
        });
        if let Some(report) = report {
            let mut blank_pending = false;
            for line in BufReader::new(report).split(b'\n') {
                let line = line
                    .map_err(|error| format!("{HYPERFINE}'s report could not be read: {error}"))?;
                let line = format_report_line(
                    &String::from_utf8_lossy(&line),
                    &timed.instance,
                    &timed.command,
                );
                if line.is_empty() {
                    blank_pending = true;
                    continue;
                }
                if blank_pending {
                    print_line(out, "")?;
                    blank_pending = false;
                }
                print_line(out, &line)?;
            }
        }
        Ok(collected.join().unwrap_or_default())
    })
}

fn format_report_line(line: &str, instance: &str, command: &str) -> String {
    let line = line.trim_end().replace(command, instance);
    if line.is_empty() {
        return line;
    }
    format!("{REPORT_INDENT}{line}")
}

fn find_error(stderr: &str) -> String {
    let lines: Vec<String> = stderr
        .lines()
        .map(|line| strip_ansi(line).trim().to_string())
        .filter(|line| !line.is_empty())
        .collect();
    lines
        .iter()
        .find(|line| line.starts_with(ERROR_PREFIX))
        .or_else(|| lines.first())
        .cloned()
        .unwrap_or_else(|| "no message".to_string())
}

fn build_sleep(platform: Platform, seconds: u32) -> String {
    match platform {
        Platform::Windows => format!("powershell -NoProfile -Command Start-Sleep {seconds}"),
        _ => format!("sleep {seconds}"),
    }
}

fn take_json_results(path: &Path, recorded: &str, results: &mut Vec<Value>) {
    let Ok(text) = fs::read_to_string(path) else {
        return;
    };
    let _ = fs::remove_file(path);
    let Ok(mut document) = serde_json::from_str::<Value>(&text) else {
        return;
    };
    let Some(entries) = document["results"].as_array_mut() else {
        return;
    };
    for mut entry in entries.drain(..) {
        entry["command"] = Value::String(recorded.to_string());
        results.push(entry);
    }
}

fn format_markdown_table(results: &[Value]) -> Option<String> {
    struct Entry<'a> {
        command: &'a str,
        mean: f64,
        stddev: Option<f64>,
        min: f64,
        max: f64,
    }
    let entries: Vec<Entry<'_>> = results
        .iter()
        .filter_map(|value| {
            Some(Entry {
                command: value["command"].as_str()?,
                mean: value["mean"].as_f64()?,
                stddev: value["stddev"].as_f64(),
                min: value["min"].as_f64()?,
                max: value["max"].as_f64()?,
            })
        })
        .collect();
    let (fastest_index, fastest) = entries
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| a.mean.total_cmp(&b.mean))?;
    let in_ms = fastest.mean < 1.0;
    let unit = if in_ms { "ms" } else { "s" };
    let mut lines = vec![
        format!("| Command | Mean [{unit}] | Min [{unit}] | Max [{unit}] | Relative |"),
        "|:---|---:|---:|---:|---:|".to_string(),
    ];
    for (index, entry) in entries.iter().enumerate() {
        let command = entry.command.replace('|', "\\|");
        let mean = match entry.stddev {
            Some(stddev) => format!(
                "{} ± {}",
                format_duration(entry.mean, in_ms),
                format_duration(stddev, in_ms)
            ),
            None => format_duration(entry.mean, in_ms),
        };
        let min = format_duration(entry.min, in_ms);
        let max = format_duration(entry.max, in_ms);
        let relative = if index == fastest_index || fastest.mean <= 0.0 {
            "1.00".to_string()
        } else {
            format_relative(entry.mean, entry.stddev, fastest.mean, fastest.stddev)
        };
        lines.push(format!(
            "| `{command}` | {mean} | {min} | {max} | {relative} |"
        ));
    }
    Some(format!("{}\n", lines.join("\n")))
}

fn format_duration(seconds: f64, in_ms: bool) -> String {
    if in_ms {
        format!("{:.1}", seconds * 1000.0)
    } else {
        format!("{seconds:.3}")
    }
}

fn format_relative(
    mean: f64,
    stddev: Option<f64>,
    fastest_mean: f64,
    fastest_stddev: Option<f64>,
) -> String {
    let ratio = mean / fastest_mean;
    match (stddev, fastest_stddev) {
        (Some(stddev), Some(fastest_stddev)) if mean > 0.0 => {
            let spread = propagate_ratio_stddev(ratio, mean, stddev, fastest_mean, fastest_stddev);
            format!("{ratio:.2} ± {spread:.2}")
        }
        _ => format!("{ratio:.2}"),
    }
}

fn capture_every_instance(
    out: &mut dyn Write,
    runner: &mut Runner,
    instances: &[Instance],
    corpus: &Path,
    extensions: &[String],
    as_json: bool,
) -> Result<(), String> {
    for table in TABLES {
        for instance in instances {
            let name = get_capture_name(table, instance.get_name());
            let args = build_args(instance, corpus, extensions, table, as_json)?;
            runner.capture_output(
                out,
                &name,
                &instance.identity.binary,
                &args,
                &instance.execution_env,
                as_json,
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_command_is_joined_the_way_hyperfine_reads_it_without_a_shell() {
        let binary = Path::new("D:/counters/mezura.exe");
        let args = [
            "D:/bench corpora/linux",
            "--languages",
            "c,h",
            "/home/x/o'brien",
        ]
        .map(String::from);
        assert_eq!(
            join_command(binary, &args),
            "D:/counters/mezura.exe \"D:/bench corpora/linux\" --languages c,h \"/home/x/o'brien\""
        );
        assert_eq!(quote("\"already quoted\""), "\"already quoted\"");
        assert_eq!(quote("say \"hi\""), "\"say \\\"hi\\\"\"");
        if cfg!(windows) {
            assert_eq!(quote("D:\\counters\\mezura.exe"), "D:/counters/mezura.exe");
        } else {
            assert_eq!(quote("/home/x/my\\dir"), "\"/home/x/my\\\\dir\"");
        }
    }

    #[test]
    fn hyperfine_s_warnings_are_lifted_out_of_its_stderr_without_the_colours() {
        let stderr = "\x1b[33mWarning:\x1b[0m Statistical outliers were detected. Consider re-running.\n\
                      some other line\n\
                      Warning: Command took less than 5 ms to complete.\n";
        assert_eq!(
            find_warnings(stderr),
            [
                "Statistical outliers were detected. Consider re-running.",
                "Command took less than 5 ms to complete."
            ]
        );
        assert_eq!(
            find_error("\x1b[31mError:\x1b[0m Command terminated with non-zero exit code\n"),
            "Error: Command terminated with non-zero exit code"
        );
        assert_eq!(find_error("\n"), "no message");
    }

    #[test]
    fn a_set_name_says_which_table_it_belongs_to() {
        assert_eq!(Table::of_set("t1-fwd"), Some(Table::SameWork));
        assert_eq!(Table::of_set("t2-rev"), Some(Table::OutOfTheBox));
        assert_eq!(Table::of_set(CONTROL_START), None);
    }

    #[test]
    fn a_counter_that_exits_non_zero_has_its_command_line_and_its_stderr_repeated() {
        let dir = std::env::temp_dir().join("linebench-a_counter_that_exits_non_zero");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join(OUT_DIR)).unwrap();
        let mut runner = Runner::new(
            &dir,
            Settings::default(),
            Platform::Linux,
            Vec::new(),
            Style::Hidden,
        );
        let mut printed = Vec::new();
        let args = ["--no-such-flag-linebench".to_string()];
        let written = runner
            .capture_output(
                &mut printed,
                "t1-git",
                Path::new("git"),
                &args,
                &BTreeMap::new(),
                false,
            )
            .unwrap();
        let printed = String::from_utf8(printed).unwrap();
        assert!(printed.starts_with("WARNING: git exited 129 while writing t1-git.txt\n"));
        assert!(printed.contains("\n         git --no-such-flag-linebench\n"));
        assert!(printed.contains("no-such-flag-linebench\n"));
        assert!(
            fs::read_to_string(&written.path)
                .unwrap()
                .contains("no-such-flag-linebench")
        );
        assert!(runner.capture_failures.is_empty());
        let as_json = runner
            .capture_output(
                &mut Vec::new(),
                "t1-git",
                Path::new("git"),
                &args,
                &BTreeMap::new(),
                true,
            )
            .unwrap();
        assert!(as_json.stderr.contains("no-such-flag-linebench"));
        assert!(
            !fs::read_to_string(&as_json.path)
                .unwrap()
                .contains("no-such-flag-linebench")
        );
        assert_eq!(
            runner.capture_failures,
            ["git exited 129 while writing t1-git.json"]
        );
        assert!(runner.failures.is_empty());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn hyperfine_s_report_names_the_instance_where_hyperfine_named_the_command() {
        assert_eq!(
            format_report_line(
                "Benchmark 1: /c/mezura /corpus --languages c\r",
                "mezura@next",
                "/c/mezura /corpus --languages c"
            ),
            "   Benchmark 1: mezura@next"
        );
        assert_eq!(
            format_report_line(
                "  \x1b[36m/c/mezura /corpus\x1b[0m ran",
                "mezura",
                "/c/mezura /corpus"
            ),
            "     \x1b[36mmezura\x1b[0m ran"
        );
        assert_eq!(format_report_line(" ", "mezura", "/c/mezura /corpus"), "");
    }

    #[test]
    fn the_markdown_table_shares_one_unit_and_measures_every_row_against_the_fastest() {
        let results = [
            serde_json::json!({
                "command": "cloc /corpus",
                "mean": 1.050,
                "stddev": 0.015,
                "min": 1.030,
                "max": 1.072,
            }),
            serde_json::json!({
                "command": "MEZURA_DATA_DIR=/tmp/a mezura /corpus",
                "mean": 0.0035394,
                "stddev": 0.0001018,
                "min": 0.0034,
                "max": 0.0038,
            }),
        ];
        assert_eq!(
            format_markdown_table(&results).unwrap(),
            "| Command | Mean [ms] | Min [ms] | Max [ms] | Relative |\n\
             |:---|---:|---:|---:|---:|\n\
             | `cloc /corpus` | 1050.0 ± 15.0 | 1030.0 | 1072.0 | 296.66 ± 9.53 |\n\
             | `MEZURA_DATA_DIR=/tmp/a mezura /corpus` | 3.5 ± 0.1 | 3.4 | 3.8 | 1.00 |\n"
        );
        let in_seconds = [serde_json::json!({
            "command": "cloc /linux",
            "mean": 1.42,
            "stddev": null,
            "min": 1.42,
            "max": 1.42,
        })];
        assert_eq!(
            format_markdown_table(&in_seconds).unwrap(),
            "| Command | Mean [s] | Min [s] | Max [s] | Relative |\n\
             |:---|---:|---:|---:|---:|\n\
             | `cloc /linux` | 1.420 | 1.420 | 1.420 | 1.00 |\n"
        );
    }
}
