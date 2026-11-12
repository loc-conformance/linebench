use std::collections::BTreeMap;
use std::fs;
#[cfg(target_os = "linux")]
use std::io::Write;
use std::io::{self, Read};
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::machine::Platform;

#[cfg(unix)]
#[allow(unsafe_code)]
mod unix;
#[cfg(windows)]
#[allow(unsafe_code)]
mod windows;

const POWERSHELL: &str = "powershell";
#[cfg(unix)]
const PROC_STAT: &str = "/proc/stat";
#[cfg(unix)]
const PROC_SELF: &str = "/proc/self";
#[cfg(unix)]
const SYS_DEVICES: &str = "/sys/devices";
const CPU_LINE: &str = "cpu ";
const RSS_LINE: &str = "VmRSS:";
const PEAK_LINE: &str = "VmHWM:";
const IDLE_COLUMN: usize = 3;
const IOWAIT_COLUMN: usize = 4;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
const POLL: Duration = Duration::from_millis(20);
const SUDO: &str = "sudo";
const NO_PROMPT: &str = "-n";
#[cfg(target_os = "linux")]
const DROP_CACHES: &str = "/proc/sys/vm/drop_caches";
/// 1 empties the page cache, 2 the cached directory entries and inodes, 3 both.
#[cfg(target_os = "linux")]
const DROP_EVERYTHING: &[u8] = b"3\n";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhysicalCore {
    pub efficiency_class: u32,
    pub logical_cpus: Vec<usize>,
}

#[derive(Debug)]
pub struct AffinityGuard {
    #[cfg(windows)]
    saved_mask: usize,
    #[cfg(unix)]
    saved_cpus: Vec<usize>,
}

impl Drop for AffinityGuard {
    fn drop(&mut self) {
        #[cfg(windows)]
        let _ = windows::write_affinity_mask(self.saved_mask);
        #[cfg(unix)]
        let _ = unix::write_affinity_cpus(&self.saved_cpus);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Footprint {
    pub resident: u64,
    pub peak: u64,
}

/// What a run was given over its whole life. On Linux it includes the children the run waited for.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    pub faults: u64,
    /// Windows counts what it read from storage inside its one fault total, so there it is zero.
    pub disk_read_bytes: u64,
}

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

pub fn apply_execution_env(
    command: &mut Command,
    env: &BTreeMap<String, String>,
    scrub: &[String],
) {
    for name in scrub {
        command.env_remove(name);
    }
    command.envs(env);
}

pub fn capture_with_status(program: &str, args: &[&str]) -> Result<(bool, String), Unfinished> {
    capture_with_status_in_env(program, args, &BTreeMap::new(), &[])
}

pub fn capture_with_status_in_env(
    program: &str,
    args: &[&str],
    env: &BTreeMap<String, String>,
    scrub: &[String],
) -> Result<(bool, String), Unfinished> {
    let could_not_run = |error: &dyn std::fmt::Display| Unfinished::CouldNotRun(error.to_string());
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    apply_execution_env(&mut command, env, scrub);
    let mut child = command.spawn().map_err(|error| {
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
pub fn read_process_memory(_platform: Platform, child: &Child) -> Option<Footprint> {
    windows::read_process_memory(child)
}

#[cfg(unix)]
pub fn read_process_memory(platform: Platform, child: &Child) -> Option<Footprint> {
    if !platform.is_linux() {
        return None;
    }
    let (resident, peak) =
        parse_proc_status(&fs::read_to_string(format!("/proc/{}/status", child.id())).ok()?)?;
    Some(Footprint { resident, peak })
}

#[cfg(windows)]
pub fn try_reap(_platform: Platform, child: &mut Child) -> io::Result<Option<(ExitStatus, Usage)>> {
    Ok(child
        .try_wait()?
        .map(|status| (status, windows::read_usage(child))))
}

/// On Linux the run is reaped here, behind the back of `Child`, so nothing may wait on it or kill
/// it afterwards: its pid can already belong to another process.
#[cfg(unix)]
pub fn try_reap(platform: Platform, child: &mut Child) -> io::Result<Option<(ExitStatus, Usage)>> {
    if !platform.is_linux() {
        return Ok(child.try_wait()?.map(|status| (status, Usage::default())));
    }
    unix::reap_without_blocking(child.id())
}

#[cfg(windows)]
pub fn reap(_platform: Platform, child: &mut Child) -> io::Result<(ExitStatus, Usage)> {
    let status = child.wait()?;
    Ok((status, windows::read_usage(child)))
}

/// On Linux the run is reaped behind the back of `Child`, as in `try_reap`.
#[cfg(unix)]
pub fn reap(platform: Platform, child: &mut Child) -> io::Result<(ExitStatus, Usage)> {
    if !platform.is_linux() {
        return Ok((child.wait()?, Usage::default()));
    }
    unix::reap(child.id())
}

/// Empties the kernel page cache and its cached directory entries and inodes, for the whole
/// system. The write wants root: as root it is made here, otherwise through `sudo -n tee`, which
/// fails at once where sudo would ask for a password.
#[cfg(target_os = "linux")]
pub fn drop_page_cache(privileged: bool) -> Result<(), String> {
    unix::sync_filesystems();
    let written = match privileged {
        true => fs::write(DROP_CACHES, DROP_EVERYTHING).map_err(|error| error.to_string()),
        false => write_through_sudo(DROP_CACHES, DROP_EVERYTHING),
    };
    written.map_err(|reason| format!("{DROP_CACHES} could not be written: {reason}"))
}

#[cfg(not(target_os = "linux"))]
pub fn drop_page_cache(_privileged: bool) -> Result<(), String> {
    Err("the page cache is emptied on linux".to_string())
}

/// Goes as far as `drop_page_cache` up to the open of the file: as root it opens it for writing,
/// otherwise it runs the same `sudo -n tee` on an empty input, so a sudoers rule that allows that
/// one command passes the same as a remembered password. The kernel empties the cache on a write,
/// and both stop at the open.
#[cfg(target_os = "linux")]
pub fn check_page_cache_can_be_dropped(privileged: bool) -> Result<(), String> {
    let opened = match privileged {
        true => fs::OpenOptions::new()
            .write(true)
            .open(DROP_CACHES)
            .map(|_| ())
            .map_err(|error| error.to_string()),
        false => write_through_sudo(DROP_CACHES, &[]),
    };
    opened.map_err(|reason| format!("{DROP_CACHES} could not be written: {reason}"))
}

#[cfg(not(target_os = "linux"))]
pub fn check_page_cache_can_be_dropped(_privileged: bool) -> Result<(), String> {
    Err("the page cache is emptied on linux".to_string())
}

/// Starts again the minutes sudo remembers the password for, and says whether it did. It fails at
/// once where a plain sudo would ask for the password: once the minutes have run out, and under a
/// sudoers rule that covers the one write while the user's other entries want a password.
pub fn keep_sudo_alive() -> bool {
    run_quietly(SUDO, &[NO_PROMPT, "-v"])
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

#[cfg(windows)]
pub fn read_core_topology(platform: Platform) -> Result<Vec<PhysicalCore>, String> {
    if platform != Platform::Windows {
        return Err("affinity ladder is only supported on Linux and Windows".to_string());
    }
    windows::read_core_topology()
}

#[cfg(unix)]
pub fn read_core_topology(platform: Platform) -> Result<Vec<PhysicalCore>, String> {
    if !platform.is_linux() {
        return Err("affinity ladder is only supported on Linux and Windows".to_string());
    }
    let allowed = unix::read_affinity_cpus()?;
    read_linux_core_topology(Path::new(SYS_DEVICES), &allowed)
}

#[cfg(windows)]
pub fn pin_current_process(platform: Platform, cpus: &[usize]) -> Result<AffinityGuard, String> {
    if platform != Platform::Windows {
        return Err("affinity ladder is only supported on Linux and Windows".to_string());
    }
    let saved_mask = windows::read_affinity_mask()?;
    let target_mask = windows::build_affinity_mask(cpus)?;
    windows::write_affinity_mask(target_mask)?;
    Ok(AffinityGuard { saved_mask })
}

#[cfg(unix)]
pub fn pin_current_process(platform: Platform, cpus: &[usize]) -> Result<AffinityGuard, String> {
    if !platform.is_linux() {
        return Err("affinity ladder is only supported on Linux and Windows".to_string());
    }
    let saved_cpus = unix::read_affinity_cpus()?;
    unix::write_affinity_cpus(cpus)?;
    Ok(AffinityGuard { saved_cpus })
}

pub fn read_linux_core_topology(
    sys_devices: &Path,
    allowed: &[usize],
) -> Result<Vec<PhysicalCore>, String> {
    if allowed.is_empty() {
        return Err("the process affinity mask is empty".to_string());
    }
    let cpu_root = sys_devices.join("system/cpu");
    let p_cores = fs::read_to_string(sys_devices.join("cpu_core/cpus"))
        .ok()
        .and_then(|text| parse_cpu_list(&text));
    let e_cores = fs::read_to_string(sys_devices.join("cpu_atom/cpus"))
        .ok()
        .and_then(|text| parse_cpu_list(&text));
    let mut cores: Vec<PhysicalCore> = Vec::new();
    for &cpu in allowed {
        let cpu_dir = cpu_root.join(format!("cpu{cpu}"));
        let siblings_text = fs::read_to_string(cpu_dir.join("topology/thread_siblings_list"))
            .or_else(|_| fs::read_to_string(cpu_dir.join("topology/core_cpus_list")))
            .map_err(|_| format!("cpu {cpu} topology siblings could not be read"))?;
        let mut siblings = parse_cpu_list(&siblings_text)
            .ok_or_else(|| format!("cpu {cpu} topology siblings could not be parsed"))?;
        siblings.retain(|sibling| allowed.contains(sibling));
        if siblings.is_empty() {
            continue;
        }
        let efficiency_class = if p_cores.is_some() || e_cores.is_some() {
            u32::from(p_cores.as_ref().is_some_and(|list| list.contains(&cpu)))
        } else {
            fs::read_to_string(cpu_dir.join("cpu_capacity"))
                .ok()
                .and_then(|text| text.trim().parse::<u32>().ok())
                .unwrap_or(0)
        };
        if let Some(existing) = cores.iter_mut().find(|core| core.logical_cpus == siblings) {
            existing.efficiency_class = existing.efficiency_class.max(efficiency_class);
        } else {
            cores.push(PhysicalCore {
                efficiency_class,
                logical_cpus: siblings,
            });
        }
    }
    if cores.is_empty() {
        return Err("no physical cores were found in the process affinity mask".to_string());
    }
    cores.sort_by_key(|core| core.logical_cpus[0]);
    Ok(cores)
}

pub fn parse_cpu_list(text: &str) -> Option<Vec<usize>> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut cpus = Vec::new();
    for part in trimmed.split(',') {
        let part = part.trim();
        if part.is_empty() {
            return None;
        }
        if let Some((start, end)) = part.split_once('-') {
            let start = start.trim().parse::<usize>().ok()?;
            let end = end.trim().parse::<usize>().ok()?;
            if start > end {
                return None;
            }
            cpus.extend(start..=end);
        } else {
            cpus.push(part.parse::<usize>().ok()?);
        }
    }
    cpus.sort_unstable();
    cpus.dedup();
    (!cpus.is_empty()).then_some(cpus)
}

pub fn parse_proc_status(text: &str) -> Option<(u64, u64)> {
    let read = |key: &str| {
        text.lines()
            .find(|line| line.starts_with(key))
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|kilobytes| kilobytes.parse::<u64>().ok())
            .map(|kilobytes| kilobytes * 1024)
    };
    Some((read(RSS_LINE)?, read(PEAK_LINE)?))
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

/// `sudo echo 3 > file` fails, since the user's own shell opens the file before sudo starts, so the
/// bytes go through `tee` running as root.
#[cfg(target_os = "linux")]
fn write_through_sudo(path: &str, bytes: &[u8]) -> Result<(), String> {
    let mut tee = Command::new(SUDO)
        .args([NO_PROMPT, "tee", path])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("{SUDO} could not be run: {error}"))?;
    let wrote = tee.stdin.take().map(|mut stdin| stdin.write_all(bytes));
    let finished = tee
        .wait_with_output()
        .map_err(|error| format!("{SUDO} could not be waited on: {error}"))?;
    let said = String::from_utf8_lossy(&finished.stderr).trim().to_string();
    match (finished.status.success(), wrote) {
        (true, Some(Ok(()))) => Ok(()),
        (true, _) => Err("tee ended before it was given what to write".to_string()),
        (false, _) if said.is_empty() => Err(format!("{SUDO} ended with {}", finished.status)),
        (false, _) => Err(said),
    }
}

#[cfg(all(test, target_os = "linux"))]
pub fn drop_from_page_cache(file: &fs::File) -> io::Result<()> {
    unix::drop_from_page_cache(file)
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
    fn the_resident_size_and_the_high_water_mark_come_out_of_proc_status_in_bytes() {
        let text = "Name:	tokei
VmRSS:	   45208 kB
VmHWM:	   61440 kB
Threads:	16
";
        assert_eq!(
            parse_proc_status(text),
            Some((45_208 * 1024, 61_440 * 1024))
        );
        assert_eq!(
            parse_proc_status(
                "Name:	tokei
VmRSS:	 45208 kB
"
            ),
            None
        );
        assert_eq!(parse_proc_status(""), None);
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

    #[test]
    fn a_linux_cpu_list_parses_single_cpus_and_ranges_and_refuses_malformed_entries() {
        assert_eq!(parse_cpu_list("0,8\n"), Some(vec![0, 8]));
        assert_eq!(
            parse_cpu_list("0-3,8-11\n"),
            Some(vec![0, 1, 2, 3, 8, 9, 10, 11])
        );
        assert_eq!(parse_cpu_list("4,0-2,2\n"), Some(vec![0, 1, 2, 4]));
        assert_eq!(parse_cpu_list(""), None);
        assert_eq!(parse_cpu_list("4-2"), None);
        assert_eq!(parse_cpu_list("0,"), None);
        assert_eq!(parse_cpu_list("cpu0"), None);
    }

    #[test]
    fn linux_sysfs_topology_groups_smt_siblings_reads_hybrid_classes_and_refuses_missing_siblings()
    {
        let sys = std::env::temp_dir().join("linebench-os-sysfs-topology");
        let _ = fs::remove_dir_all(&sys);
        for (cpu, siblings) in [
            (0usize, "0,4\n"),
            (1, "1,5\n"),
            (2, "2\n"),
            (3, "3\n"),
            (4, "0,4\n"),
            (5, "1,5\n"),
        ] {
            let topo = sys.join(format!("system/cpu/cpu{cpu}/topology"));
            fs::create_dir_all(&topo).expect("mkdir");
            fs::write(topo.join("thread_siblings_list"), siblings).expect("write siblings");
        }
        fs::create_dir_all(sys.join("cpu_core")).expect("mkdir cpu_core");
        fs::create_dir_all(sys.join("cpu_atom")).expect("mkdir cpu_atom");
        fs::write(sys.join("cpu_core/cpus"), "0-1,4-5\n").expect("write p-cores");
        fs::write(sys.join("cpu_atom/cpus"), "2-3\n").expect("write e-cores");

        let cores = read_linux_core_topology(&sys, &[0, 1, 2, 3, 4, 5]).expect("topology");
        assert_eq!(
            cores,
            vec![
                PhysicalCore {
                    efficiency_class: 1,
                    logical_cpus: vec![0, 4],
                },
                PhysicalCore {
                    efficiency_class: 1,
                    logical_cpus: vec![1, 5],
                },
                PhysicalCore {
                    efficiency_class: 0,
                    logical_cpus: vec![2],
                },
                PhysicalCore {
                    efficiency_class: 0,
                    logical_cpus: vec![3],
                },
            ]
        );

        let missing = read_linux_core_topology(&sys, &[0, 99]).unwrap_err();
        assert!(missing.contains("cpu 99"), "{missing}");
        let _ = fs::remove_dir_all(sys.join("cpu_core"));
        let _ = fs::remove_dir_all(sys.join("cpu_atom"));
        for (cpu, perf) in [(0usize, "166\n"), (1, "161\n"), (2, "156\n"), (3, "151\n")] {
            let cppc = sys.join(format!("system/cpu/cpu{cpu}/acpi_cppc"));
            fs::create_dir_all(&cppc).expect("mkdir cppc");
            fs::write(cppc.join("highest_perf"), perf).expect("write highest_perf");
        }
        let homogeneous = read_linux_core_topology(&sys, &[0, 1, 2, 3, 4, 5]).expect("homogeneous");
        assert!(homogeneous.iter().all(|core| core.efficiency_class == 0));
        let _ = fs::remove_dir_all(&sys);
    }

    #[test]
    fn only_the_os_module_allows_unsafe_code_and_every_unsafe_block_explains_why_it_is_sound() {
        fn collect_rs_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
            for entry in fs::read_dir(dir).expect("read_dir").flatten() {
                let path = entry.path();
                if path.is_dir() {
                    collect_rs_files(&path, out);
                } else if path.extension().is_some_and(|ext| ext == "rs") {
                    out.push(path);
                }
            }
        }
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut files = Vec::new();
        collect_rs_files(&src, &mut files);
        let mut total_allows = 0usize;
        for path in files {
            let text = fs::read_to_string(&path).expect("read rs");
            let count = text
                .lines()
                .filter(|line| {
                    line.trim().starts_with("#[allow(unsafe_code)]")
                        || line.trim().starts_with("#![allow(unsafe_code)]")
                })
                .count();
            if path.ends_with("os/mod.rs") || path.ends_with("os\\mod.rs") {
                assert_eq!(count, 2, "{}", path.display());
            } else {
                assert_eq!(
                    count,
                    0,
                    "{} should have no allow(unsafe_code)",
                    path.display()
                );
            }
            total_allows += count;
            if path.ends_with("os/windows.rs")
                || path.ends_with("os\\windows.rs")
                || path.ends_with("os/unix.rs")
                || path.ends_with("os\\unix.rs")
            {
                let lines: Vec<&str> = text.lines().collect();
                for (index, line) in lines.iter().enumerate() {
                    if line.contains("unsafe {") {
                        let prev = index
                            .checked_sub(1)
                            .and_then(|i| lines.get(i))
                            .copied()
                            .unwrap_or("");
                        let prev_two = index
                            .checked_sub(2)
                            .and_then(|i| lines.get(i))
                            .copied()
                            .unwrap_or("");
                        let has_comment = prev.trim_start().starts_with("// Sound because ")
                            || (prev.trim_end().ends_with('=')
                                && prev_two.trim_start().starts_with("// Sound because "));
                        assert!(
                            has_comment,
                            "{} line {} unsafe block lacks sound comment",
                            path.display(),
                            index + 1
                        );
                    }
                }
            }
        }
        assert_eq!(total_allows, 2);
    }

    #[cfg(any(windows, target_os = "linux"))]
    #[test]
    fn pinning_the_current_process_to_one_core_narrows_the_topology_and_drop_restores_it() {
        let platform = if cfg!(windows) {
            Platform::Windows
        } else {
            Platform::Linux
        };
        let before = read_core_topology(platform).expect("host topology");
        let first_cpu = before[0].logical_cpus[0];
        {
            let _guard = pin_current_process(platform, &[first_cpu]).expect("pin to 1 cpu");
            let pinned = read_core_topology(platform).expect("pinned topology");
            assert_eq!(pinned.len(), 1);
            assert_eq!(pinned[0].logical_cpus, vec![first_cpu]);
        }
        let restored = read_core_topology(platform).expect("restored topology");
        assert_eq!(restored, before);
    }
}
