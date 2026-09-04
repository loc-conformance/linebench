use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::counters::Definition;
use crate::fetch::Identity;
use crate::machine::Platform;

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
const WARNING_PREFIX: &str = "Warning:";
const ERROR_PREFIX: &str = "Error";
const ESCAPE: char = '\x1b';

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
}

impl Instance {
    pub fn name(&self) -> &str {
        &self.identity.instance
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

pub struct Runner {
    res: PathBuf,
    settings: Settings,
    platform: Platform,
    scrub: Vec<String>,
    pub failures: Vec<String>,
    pub warnings: Vec<String>,
    pub commands: BTreeMap<String, String>,
}

impl Runner {
    pub fn new(res: &Path, settings: Settings, platform: Platform, scrub: Vec<String>) -> Runner {
        Runner {
            res: res.to_path_buf(),
            settings,
            platform,
            scrub,
            failures: Vec::new(),
            warnings: Vec::new(),
            commands: BTreeMap::new(),
        }
    }

    pub fn run_hyperfine(
        &mut self,
        out: &mut dyn Write,
        name: &str,
        commands: &[(String, String)],
    ) -> Result<(), String> {
        print_line(out, &format!(">> {name}"))?;
        let export = self.res.join(format!("{name}.json"));
        let markdown = self.res.join(format!("{name}.md"));
        let mut hyperfine = Command::new(HYPERFINE);
        hyperfine.args([
            "-N",
            "--warmup",
            &self.settings.warmup.to_string(),
            "--runs",
            &self.settings.runs.to_string(),
        ]);
        if self.settings.settle > 0 {
            hyperfine.args(["--setup", &build_sleep(self.platform, self.settings.settle)]);
        }
        hyperfine.args(["--export-json", &export.to_string_lossy()]);
        hyperfine.args(["--export-markdown", &markdown.to_string_lossy()]);
        for (instance, command) in commands {
            hyperfine.arg(command);
            self.commands.insert(command.clone(), instance.clone());
        }
        for name in &self.scrub {
            hyperfine.env_remove(name);
        }
        let finished = hyperfine
            .stdin(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .map_err(|error| format!("{HYPERFINE} could not be run: {error}"))?;
        let stderr = String::from_utf8_lossy(&finished.stderr);
        for warning in find_warnings(&stderr) {
            print_line(out, &format!("WARNING: {HYPERFINE} on {name}: {warning}"))?;
            self.warnings.push(format!("{name}: {warning}"));
        }
        if !finished.status.success() {
            let detail = find_error(&stderr);
            print_line(
                out,
                &format!("WARNING: {HYPERFINE} reported a problem on {name}: {detail}"),
            )?;
            self.failures.push(name.to_string());
        }
        if export.is_file() {
            shrink_json(&export);
        }
        Ok(())
    }

    pub fn capture_output(
        &mut self,
        out: &mut dyn Write,
        name: &str,
        program: &Path,
        args: &[String],
        as_json: bool,
    ) -> Result<PathBuf, String> {
        let suffix = if as_json { "json" } else { "txt" };
        let path = self.res.join(OUT_DIR).join(format!("{name}.{suffix}"));
        let file = File::create(&path)
            .map_err(|error| format!("{} could not be created: {error}", path.display()))?;
        let stderr = if as_json {
            Stdio::null()
        } else {
            Stdio::from(file.try_clone().map_err(|error| error.to_string())?)
        };
        let mut command = Command::new(program);
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(file)
            .stderr(stderr);
        for name in &self.scrub {
            command.env_remove(name);
        }
        let status = command
            .status()
            .map_err(|error| format!("{} could not be run: {error}", program.display()))?;
        if !status.success() {
            let label = format!("{name}.{suffix}");
            print_line(
                out,
                &format!(
                    "WARNING: {} exited {} while writing {label}",
                    program.display(),
                    status
                        .code()
                        .map_or("by a signal".to_string(), |c| c.to_string())
                ),
            )?;
            self.failures.push(label);
        }
        Ok(path)
    }
}

pub fn run_phases(
    out: &mut dyn Write,
    runner: &mut Runner,
    instances: &[Instance],
    control: usize,
    corpus: &Path,
    extensions: &[String],
) -> Result<(), String> {
    print_line(
        out,
        "\n== output and JSON captures (also the settling runs)",
    )?;
    for as_json in [false, true] {
        for table in TABLES {
            for instance in instances {
                let name = format!("{}-{}", table.as_str(), instance.name());
                let args = build_args(instance, corpus, extensions, table, as_json)?;
                runner.capture_output(out, &name, &instance.identity.binary, &args, as_json)?;
            }
        }
    }
    let bare = build_command(&instances[control], corpus, extensions, Table::OutOfTheBox)?;
    print_line(out, "\n== opening control run")?;
    runner.run_hyperfine(out, CONTROL_START, std::slice::from_ref(&bare))?;
    for table in TABLES {
        print_line(out, &format!("\n== {}", table.describe()))?;
        let commands: Vec<(String, String)> = instances
            .iter()
            .map(|instance| build_command(instance, corpus, extensions, table))
            .collect::<Result<_, _>>()?;
        runner.run_hyperfine(out, &format!("{}-{FORWARD}", table.as_str()), &commands)?;
        let reversed: Vec<(String, String)> = commands.into_iter().rev().collect();
        runner.run_hyperfine(out, &format!("{}-{REVERSE}", table.as_str()), &reversed)?;
    }
    print_line(out, "\n== closing control run")?;
    runner.run_hyperfine(out, CONTROL_END, std::slice::from_ref(&bare))
}

pub fn build_args(
    instance: &Instance,
    corpus: &Path,
    extensions: &[String],
    table: Table,
    as_json: bool,
) -> Result<Vec<String>, String> {
    instance
        .definition
        .build_args(corpus, extensions, table.uses_same_work(), as_json)
}

pub fn build_command(
    instance: &Instance,
    corpus: &Path,
    extensions: &[String],
    table: Table,
) -> Result<(String, String), String> {
    let args = build_args(instance, corpus, extensions, table, false)?;
    Ok((
        instance.name().to_string(),
        join_command(&instance.identity.binary, &args),
    ))
}

pub fn join_command(binary: &Path, args: &[String]) -> String {
    let mut parts = vec![quote(&binary.to_string_lossy())];
    parts.extend(args.iter().map(|arg| quote(arg)));
    parts.join(" ")
}

pub fn quote(part: &str) -> String {
    let part = part.replace('\\', "/");
    if part.contains(' ') && !part.starts_with('"') {
        format!("\"{part}\"")
    } else {
        part
    }
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

fn shrink_json(path: &Path) {
    let Ok(text) = fs::read_to_string(path) else {
        return;
    };
    let Ok(document) = serde_json::from_str::<Value>(&text) else {
        return;
    };
    if let Ok(compact) = serde_json::to_string(&document) {
        let _ = fs::write(path, compact);
    }
}

fn print_line(out: &mut dyn Write, message: &str) -> Result<(), String> {
    writeln!(out, "{message}")
        .map_err(|error| format!("this report could not be written: {error}"))?;
    out.flush()
        .map_err(|error| format!("this report could not be written: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_command_is_joined_the_way_hyperfine_reads_it_without_a_shell() {
        let binary = Path::new("D:\\counters\\mezura.exe");
        let args = ["D:\\bench corpora\\linux", "--languages", "c,h"].map(String::from);
        assert_eq!(
            join_command(binary, &args),
            "D:/counters/mezura.exe \"D:/bench corpora/linux\" --languages c,h"
        );
        assert_eq!(quote("\"already quoted\""), "\"already quoted\"");
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
}
