#[cfg(unix)]
use std::fs;
use std::io::Read;
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

pub fn capture_output(program: &str, args: &[&str]) -> Option<String> {
    let (ok, out) = capture_with_status(program, args)?;
    if ok && !out.is_empty() {
        Some(out)
    } else {
        None
    }
}

pub fn capture_with_status(program: &str, args: &[&str]) -> Option<(bool, String)> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
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
                return None;
            }
            Err(_) => return None,
        }
    };
    let bytes = reader.join().ok()?;
    let out = String::from_utf8_lossy(&bytes).trim().to_string();
    Some((status.success(), out))
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
    capture_with_status(POWERSHELL, &["-NoProfile", "-Command", script])
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
    use std::fs;
    use std::path::{Path, PathBuf};

    use super::*;

    const SOURCES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
    const THE_ONE_MODULE: &str = "os/windows.rs";

    #[test]
    fn unsafe_lives_in_one_module_and_nowhere_else() {
        let allow = ["allow(", "unsafe_code)"].concat();
        let keyword = ["un", "safe "].concat();
        let mut allowed = Vec::new();
        let mut leaked = Vec::new();
        for path in collect_sources(Path::new(SOURCES)) {
            let text = fs::read_to_string(&path).expect("a source file");
            let relative = path
                .strip_prefix(SOURCES)
                .expect("under src")
                .to_string_lossy()
                .replace('\\', "/");
            if text.contains(&allow) {
                allowed.push(relative.clone());
            }
            if relative != THE_ONE_MODULE && text.contains(&keyword) {
                leaked.push(relative);
            }
        }
        assert_eq!(allowed, [THE_ONE_MODULE]);
        assert!(
            leaked.is_empty(),
            "leaked outside {THE_ONE_MODULE}: {leaked:?}"
        );
    }

    #[test]
    fn the_cpu_line_of_proc_stat_gives_idle_with_iowait_and_the_total() {
        let text =
            "cpu  4705 150 1120 16250 520 0 30 0 0 0\ncpu0 1000 50 300 4000 100 0 10 0 0 0\n";
        assert_eq!(parse_proc_stat(text), Some((16770, 22775)));
        assert_eq!(parse_proc_stat("cpu  1 2 3\n"), None);
        assert_eq!(parse_proc_stat("intr 5\n"), None);
    }

    #[test]
    fn a_command_s_output_comes_back_whole_and_a_missing_program_is_none() {
        let (ok, out) = capture_with_status("git", &["--version"]).expect("git runs");
        assert!(ok && out.starts_with("git version"), "{out}");
        assert_eq!(capture_with_status("no-such-program-linebench", &[]), None);
        assert_eq!(capture_output("git", &["--no-such-flag-linebench"]), None);
    }

    fn collect_sources(dir: &Path) -> Vec<PathBuf> {
        let mut found = Vec::new();
        for entry in fs::read_dir(dir).expect("the source dir").flatten() {
            let path = entry.path();
            if path.is_dir() {
                found.extend(collect_sources(&path));
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                found.push(path);
            }
        }
        found
    }
}
