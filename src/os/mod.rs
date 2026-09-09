#[cfg(unix)]
use std::fs;
use std::io::{self, Read};
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::machine::Platform;

#[cfg(windows)]
mod windows;

const POWERSHELL: &str = "powershell";
#[cfg(unix)]
const PROC_STAT: &str = "/proc/stat";
#[cfg(unix)]
const PROC_SELF: &str = "/proc/self";
const CPU_LINE: &str = "cpu ";
const IDLE_COLUMN: usize = 3;
const IOWAIT_COLUMN: usize = 4;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
const POLL: Duration = Duration::from_millis(20);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unfinished {
    NotFound,
    CouldNotRun(String),
    TimedOut,
}

impl Unfinished {
    pub fn describe(&self, what: &str) -> String {
        match self {
            Unfinished::NotFound => format!("{what} could not be run: no such program"),
            Unfinished::CouldNotRun(error) => format!("{what} could not be run: {error}"),
            Unfinished::TimedOut => format!(
                "{what} gave no answer in {} s and was stopped",
                COMMAND_TIMEOUT.as_secs()
            ),
        }
    }
}

pub fn capture_output(program: &str, args: &[&str]) -> Option<String> {
    let (ok, out) = capture_with_status(program, args).ok()?;
    if ok && !out.is_empty() {
        Some(out)
    } else {
        None
    }
}

pub fn capture_with_status(program: &str, args: &[&str]) -> Result<(bool, String), Unfinished> {
    let could_not_run = |error: &dyn std::fmt::Display| Unfinished::CouldNotRun(error.to_string());
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                Unfinished::NotFound
            } else {
                could_not_run(&error)
            }
        })?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| could_not_run(&"its output could not be read"))?;
    let reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stdout.read_to_end(&mut bytes);
        bytes
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < COMMAND_TIMEOUT => thread::sleep(POLL),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(Unfinished::TimedOut);
            }
            Err(error) => return Err(could_not_run(&error)),
        }
    };
    let bytes = reader
        .join()
        .map_err(|_| could_not_run(&"its output could not be read"))?;
    let out = String::from_utf8_lossy(&bytes).trim().to_string();
    Ok((status.success(), out))
}

pub fn run_quietly(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

pub fn run_powershell(script: &str) -> Option<String> {
    capture_output(POWERSHELL, &["-NoProfile", "-Command", script])
}

pub fn run_powershell_with_status(script: &str) -> Option<(bool, String)> {
    capture_with_status(POWERSHELL, &["-NoProfile", "-Command", script]).ok()
}

#[cfg(windows)]
pub fn is_privileged(_platform: Platform) -> bool {
    windows::is_user_an_admin()
}

#[cfg(unix)]
pub fn is_privileged(platform: Platform) -> bool {
    if platform.is_linux() {
        return fs::metadata(PROC_SELF).is_ok_and(|me| me.uid() == 0);
    }
    capture_output("id", &["-u"]).as_deref() == Some("0")
}

#[cfg(windows)]
pub fn read_system_times(_platform: Platform) -> Option<(u64, u64)> {
    windows::read_system_times()
}

#[cfg(unix)]
pub fn read_system_times(platform: Platform) -> Option<(u64, u64)> {
    if !platform.is_linux() {
        return None;
    }
    parse_proc_stat(&fs::read_to_string(PROC_STAT).ok()?)
}

pub fn parse_proc_stat(text: &str) -> Option<(u64, u64)> {
    let line = text.lines().find(|line| line.starts_with(CPU_LINE))?;
    let numbers: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .filter_map(|n| n.parse().ok())
        .collect();
    if numbers.len() <= IDLE_COLUMN {
        return None;
    }
    let idle = numbers[IDLE_COLUMN] + numbers.get(IOWAIT_COLUMN).copied().unwrap_or(0);
    Some((idle, numbers.iter().sum()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cpu_line_of_proc_stat_gives_idle_with_iowait_and_the_total() {
        let text =
            "cpu  4705 150 1120 16250 520 0 30 0 0 0\ncpu0 1000 50 300 4000 100 0 10 0 0 0\n";
        assert_eq!(parse_proc_stat(text), Some((16770, 22775)));
        assert_eq!(parse_proc_stat("cpu  1 2 3\n"), None);
        assert_eq!(parse_proc_stat("intr 5\n"), None);
    }

    #[test]
    fn a_command_s_output_comes_back_whole_and_a_missing_program_says_so() {
        let (ok, out) = capture_with_status("git", &["--version"]).expect("git runs");
        assert!(ok && out.starts_with("git version"), "{out}");
        let missing = capture_with_status("no-such-program-linebench", &[]).unwrap_err();
        assert_eq!(missing, Unfinished::NotFound);
        assert_eq!(
            missing.describe("no-such-program-linebench"),
            "no-such-program-linebench could not be run: no such program"
        );
        assert_eq!(
            Unfinished::TimedOut.describe("git ls-files"),
            "git ls-files gave no answer in 30 s and was stopped"
        );
        assert_eq!(capture_output("git", &["--no-such-flag-linebench"]), None);
    }
}
