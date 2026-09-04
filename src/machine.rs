use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::env::consts::{ARCH, OS};
use std::fs;
use std::path::{self, Component, Path, PathBuf, Prefix};
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::os::{capture_output, read_system_times, run_powershell, run_quietly};

pub const WINDOWS: &str = "windows";
pub const LINUX: &str = "linux";
pub const MACOS: &str = "macos";
pub const WSL: &str = "wsl";
pub const UNKNOWN: &str = "unknown";
const BACKGROUND_SAMPLE_SECONDS: u64 = 6;
const WINDOWS_HIGH_PERFORMANCE: &str = "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c";
const PERFORMANCE: &str = "performance";
const CPU_DIR: &str = "/sys/devices/system/cpu";
const GOVERNOR_FILE: &str = "cpufreq/scaling_governor";
const AVAILABLE_GOVERNORS_FILE: &str = "scaling_available_governors";
const PROC_VERSION: &str = "/proc/version";
const PROC_MEMINFO: &str = "/proc/meminfo";
const OS_RELEASE: &str = "/etc/os-release";
const SYS_BLOCK: &str = "/sys/block";
const DF_COLUMNS_WITH_TYPE: usize = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Windows,
    Macos,
    Linux,
    Wsl,
}

impl Platform {
    pub fn as_str(self) -> &'static str {
        match self {
            Platform::Windows => WINDOWS,
            Platform::Macos => MACOS,
            Platform::Linux => LINUX,
            Platform::Wsl => WSL,
        }
    }

    pub fn as_system(self) -> &'static str {
        match self {
            Platform::Wsl => LINUX,
            other => other.as_str(),
        }
    }

    pub fn is_linux(self) -> bool {
        matches!(self, Platform::Linux | Platform::Wsl)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Machine {
    pub platform: Platform,
    pub arch: String,
    pub os: String,
    pub kernel: String,
    pub cpu: String,
    pub logical_cores: usize,
    pub ram_bytes: Option<u64>,
    pub cpu_scaling: String,
    pub corpus_fs: String,
    pub corpus_device: String,
    pub global_gitignore: String,
    pub hyperfine: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PrepStep {
    CpuGovernor {
        governors: BTreeMap<PathBuf, String>,
    },
    PowerScheme {
        was: String,
    },
}

impl PrepStep {
    pub fn describe(&self) -> String {
        match self {
            PrepStep::CpuGovernor { governors } => {
                let was: BTreeSet<&str> = governors.values().map(String::as_str).collect();
                let was: Vec<&str> = was.into_iter().collect();
                format!(
                    "cpu governor on {} cpus: {} -> {PERFORMANCE}",
                    governors.len(),
                    was.join("/")
                )
            }
            PrepStep::PowerScheme { was } => format!("power scheme: {was} -> high performance"),
        }
    }

    pub fn apply(&self) -> Vec<String> {
        match self {
            PrepStep::CpuGovernor { governors } => governors
                .keys()
                .filter_map(|path| {
                    fs::write(path, PERFORMANCE)
                        .err()
                        .map(|error| format!("could not set {}: {error}", path.display()))
                })
                .collect(),
            PrepStep::PowerScheme { .. } => {
                if run_quietly("powercfg", &["/setactive", WINDOWS_HIGH_PERFORMANCE]) {
                    Vec::new()
                } else {
                    vec!["could not set the high performance power scheme".to_string()]
                }
            }
        }
    }

    pub fn restore(&self) {
        match self {
            PrepStep::CpuGovernor { governors } => {
                for (path, value) in governors {
                    let _ = fs::write(path, value);
                }
            }
            PrepStep::PowerScheme { was } => {
                run_quietly("powercfg", &["/setactive", was]);
            }
        }
    }
}

pub fn detect_platform() -> Result<Platform, String> {
    match OS {
        WINDOWS => Ok(Platform::Windows),
        MACOS => Ok(Platform::Macos),
        LINUX => {
            let version = fs::read_to_string(PROC_VERSION).unwrap_or_default();
            if version.to_lowercase().contains("microsoft") {
                Ok(Platform::Wsl)
            } else {
                Ok(Platform::Linux)
            }
        }
        other => Err(format!(
            "linebench measures on {WINDOWS}, {LINUX} and {MACOS}, and this is {other}"
        )),
    }
}

pub fn detect_arch() -> String {
    match ARCH {
        "x86_64" | "amd64" => "x86_64".to_string(),
        "aarch64" | "arm64" => "arm64".to_string(),
        other => other.to_string(),
    }
}

pub fn collect_machine(platform: Platform, corpus: &Path) -> Machine {
    Machine {
        platform,
        arch: detect_arch(),
        os: read_os_name(platform),
        kernel: read_kernel(platform),
        cpu: read_cpu_name(platform),
        logical_cores: thread::available_parallelism().map_or(1, |cores| cores.get()),
        ram_bytes: read_ram_bytes(platform),
        cpu_scaling: read_cpu_scaling(platform),
        corpus_fs: read_filesystem_of(platform, corpus),
        corpus_device: read_device_of(platform, corpus),
        global_gitignore: capture_output("git", &["config", "--get", "core.excludesFile"])
            .unwrap_or_else(|| "none".to_string()),
        hyperfine: capture_output("hyperfine", &["--version"])
            .unwrap_or_else(|| "missing".to_string()),
    }
}

pub fn plan_prep(platform: Platform) -> Vec<PrepStep> {
    let step = match platform {
        Platform::Linux | Platform::Wsl => plan_cpu_governor(),
        Platform::Windows => plan_power_scheme(),
        Platform::Macos => None,
    };
    step.into_iter().collect()
}

pub fn explain_how_to_elevate(platform: Platform) -> (&'static str, String) {
    if platform == Platform::Windows {
        return (
            "administrator",
            "run it from a terminal opened with \"Run as administrator\"".to_string(),
        );
    }
    let argv: Vec<String> = env::args().collect();
    ("root", format!("sudo {}", argv.join(" ")))
}

pub fn sample_background_busy(platform: Platform) -> Option<f64> {
    let before = read_system_times(platform)?;
    thread::sleep(Duration::from_secs(BACKGROUND_SAMPLE_SECONDS));
    let after = read_system_times(platform)?;
    calculate_busy_percent(before, after)
}

pub fn calculate_busy_percent(before: (u64, u64), after: (u64, u64)) -> Option<f64> {
    let total = after.1.saturating_sub(before.1);
    if total == 0 {
        return None;
    }
    let idle = after.0.saturating_sub(before.0);
    let busy = 100.0 * (1.0 - idle as f64 / total as f64);
    Some((busy * 10.0).round() / 10.0)
}

pub fn find_active_power_scheme(powercfg_output: &str) -> Option<String> {
    powercfg_output
        .replace(':', " ")
        .split_whitespace()
        .find(|token| token.matches('-').count() == 4)
        .map(str::to_lowercase)
}

fn plan_cpu_governor() -> Option<PrepStep> {
    let mut paths: Vec<PathBuf> = fs::read_dir(CPU_DIR)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| is_a_numbered_cpu(path))
        .map(|path| path.join(GOVERNOR_FILE))
        .filter(|path| path.is_file())
        .collect();
    paths.sort();
    let first = paths.first()?;
    let available = read_trimmed_file(&first.with_file_name(AVAILABLE_GOVERNORS_FILE))?;
    if !available
        .split_whitespace()
        .any(|governor| governor == PERFORMANCE)
    {
        return None;
    }
    let mut governors = BTreeMap::new();
    for path in paths {
        let current = read_trimmed_file(&path)?;
        governors.insert(path, current);
    }
    if governors.values().all(|governor| governor == PERFORMANCE) {
        return None;
    }
    Some(PrepStep::CpuGovernor { governors })
}

fn plan_power_scheme() -> Option<PrepStep> {
    let active = find_active_power_scheme(&capture_output("powercfg", &["/getactivescheme"])?)?;
    if active == WINDOWS_HIGH_PERFORMANCE {
        return None;
    }
    let schemes = capture_output("powercfg", &["/list"])?.to_lowercase();
    if !schemes.contains(WINDOWS_HIGH_PERFORMANCE) {
        return None;
    }
    Some(PrepStep::PowerScheme { was: active })
}

fn is_a_numbered_cpu(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_prefix("cpu"))
        .is_some_and(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()))
}

fn read_os_name(platform: Platform) -> String {
    match platform {
        Platform::Windows => run_powershell("(Get-CimInstance Win32_OperatingSystem).Caption"),
        Platform::Macos => capture_output("sw_vers", &["-productName"])
            .zip(capture_output("sw_vers", &["-productVersion"]))
            .map(|(name, version)| format!("{name} {version}")),
        Platform::Linux | Platform::Wsl => read_trimmed_file(Path::new(OS_RELEASE))
            .and_then(|text| find_value_after(&text, "PRETTY_NAME="))
            .map(|name| name.trim_matches('"').to_string()),
    }
    .unwrap_or_else(|| UNKNOWN.to_string())
}

fn read_kernel(platform: Platform) -> String {
    match platform {
        Platform::Windows => run_powershell("[Environment]::OSVersion.Version.ToString()"),
        _ => capture_output("uname", &["-r"]),
    }
    .unwrap_or_else(|| UNKNOWN.to_string())
}

fn read_cpu_name(platform: Platform) -> String {
    match platform {
        Platform::Windows => run_powershell("(Get-CimInstance Win32_Processor).Name"),
        Platform::Macos => capture_output("sysctl", &["-n", "machdep.cpu.brand_string"]),
        Platform::Linux | Platform::Wsl => {
            capture_output("lscpu", &[]).and_then(|out| find_value_after(&out, "Model name:"))
        }
    }
    .unwrap_or_else(|| UNKNOWN.to_string())
}

fn read_ram_bytes(platform: Platform) -> Option<u64> {
    match platform {
        Platform::Windows => {
            run_powershell("(Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory")?
                .parse()
                .ok()
        }
        Platform::Macos => capture_output("sysctl", &["-n", "hw.memsize"])?
            .parse()
            .ok(),
        Platform::Linux | Platform::Wsl => {
            let meminfo = read_trimmed_file(Path::new(PROC_MEMINFO))?;
            let kilobytes: u64 = find_value_after(&meminfo, "MemTotal:")?
                .split_whitespace()
                .next()?
                .parse()
                .ok()?;
            Some(kilobytes * 1024)
        }
    }
}

fn read_cpu_scaling(platform: Platform) -> String {
    match platform {
        Platform::Windows => {
            capture_output("powercfg", &["/getactivescheme"]).unwrap_or_else(|| UNKNOWN.to_string())
        }
        Platform::Macos => "n/a".to_string(),
        Platform::Linux | Platform::Wsl => {
            read_trimmed_file(&Path::new(CPU_DIR).join("cpu0").join(GOVERNOR_FILE))
                .unwrap_or_else(|| "none".to_string())
        }
    }
}

fn read_filesystem_of(platform: Platform, path: &Path) -> String {
    let shown = path.to_string_lossy();
    match platform {
        Platform::Windows => find_drive_letter(path).and_then(|drive| {
            run_powershell(&format!("(Get-Volume -DriveLetter {drive}).FileSystemType"))
        }),
        Platform::Macos => capture_output("diskutil", &["info", &shown])
            .and_then(|out| find_value_after(&out, "File System Personality:")),
        Platform::Linux | Platform::Wsl => capture_output("df", &["-T", &shown])
            .and_then(|out| read_df_row(&out))
            .or_else(|| capture_output("df", &[&shown]).and_then(|out| read_df_row(&out))),
    }
    .unwrap_or_else(|| UNKNOWN.to_string())
}

fn read_df_row(out: &str) -> Option<String> {
    let mut lines = out.lines();
    lines.next()?;
    let fields: Vec<&str> = lines.last()?.split_whitespace().collect();
    match fields.as_slice() {
        [source, kind, ..] if fields.len() >= DF_COLUMNS_WITH_TYPE => {
            Some(format!("{kind} {source}"))
        }
        [source, ..] => Some(source.to_string()),
        [] => None,
    }
}

fn read_device_of(platform: Platform, path: &Path) -> String {
    let shown = path.to_string_lossy();
    match platform {
        Platform::Windows => find_drive_letter(path).and_then(|drive| {
            run_powershell(&format!(
                "$d = (Get-Partition -DriveLetter {drive} | Get-Disk); \
                 $p = Get-PhysicalDisk | Where-Object {{$_.DeviceId -eq $d.Number}}; \
                 \"{{0}}, {{1}}, {{2}}\" -f $d.FriendlyName, $p.MediaType, $p.BusType"
            ))
        }),
        Platform::Macos => capture_output("diskutil", &["info", &shown]).map(|out| {
            let wanted = ["Device / Media Name", "Solid State", "Protocol"];
            let found: Vec<&str> = out
                .lines()
                .filter(|line| wanted.iter().any(|w| line.trim().starts_with(w)))
                .filter_map(|line| line.split_once(':').map(|(_, value)| value.trim()))
                .collect();
            found.join(", ")
        }),
        Platform::Linux | Platform::Wsl => read_linux_device_of(&shown),
    }
    .filter(|found| !found.is_empty())
    .unwrap_or_else(|| UNKNOWN.to_string())
}

fn read_linux_device_of(path: &str) -> Option<String> {
    let source = capture_output("df", &["--output=source", path])?
        .lines()
        .last()?
        .trim()
        .to_string();
    let parent = capture_output("lsblk", &["-no", "PKNAME", &source])
        .map(|out| out.trim().to_string())
        .filter(|parent| !parent.is_empty())
        .or_else(|| {
            Some(
                Path::new(&source)
                    .file_name()?
                    .to_string_lossy()
                    .into_owned(),
            )
        })?;
    let block = Path::new(SYS_BLOCK).join(&parent);
    if !block.is_dir() {
        return None;
    }
    let read = |relative: &str| read_trimmed_file(&block.join(relative));
    let mut parts = vec![
        read("device/model")
            .filter(|model| !model.is_empty())
            .unwrap_or(parent),
    ];
    if read("queue/rotational").as_deref() == Some("1") {
        parts.push("spinning".to_string());
    }
    if let Some(speed) = read("device/device/current_link_speed") {
        match read("device/device/current_link_width") {
            Some(width) if !width.is_empty() => parts.push(format!("{speed} x{width}")),
            _ => parts.push(speed),
        }
    }
    Some(parts.join(", "))
}

fn find_drive_letter(path: &Path) -> Option<char> {
    let absolute = path::absolute(path).ok()?;
    match absolute.components().next()? {
        Component::Prefix(prefix) => match prefix.kind() {
            Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => Some(letter as char),
            _ => None,
        },
        _ => None,
    }
}

fn read_trimmed_file(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|text| text.trim().to_string())
}

fn find_value_after(text: &str, prefix: &str) -> Option<String> {
    text.lines()
        .find_map(|line| line.trim().strip_prefix(prefix))
        .map(|value| value.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_busy_share_is_the_ticks_that_were_not_idle() {
        assert_eq!(
            calculate_busy_percent((1000, 2000), (1500, 3000)),
            Some(50.0)
        );
        assert_eq!(calculate_busy_percent((0, 0), (940, 1000)), Some(6.0));
        assert_eq!(calculate_busy_percent((10, 10), (10, 10)), None);
        assert_eq!(calculate_busy_percent((0, 0), (2, 3)), Some(33.3));
    }

    #[test]
    fn the_active_power_scheme_is_the_guid_in_what_powercfg_prints() {
        let printed = "Power Scheme GUID: 381b4222-f694-41f0-9685-ff5bb260df2e  (Balanced)";
        assert_eq!(
            find_active_power_scheme(printed).as_deref(),
            Some("381b4222-f694-41f0-9685-ff5bb260df2e")
        );
        assert_eq!(find_active_power_scheme("powercfg is not recognized"), None);
    }

    #[test]
    fn a_governor_step_names_the_cpus_and_what_they_were() {
        let governors = BTreeMap::from([
            (PathBuf::from("/sys/cpu0"), "schedutil".to_string()),
            (PathBuf::from("/sys/cpu1"), "powersave".to_string()),
            (PathBuf::from("/sys/cpu2"), "schedutil".to_string()),
        ]);
        let step = PrepStep::CpuGovernor { governors };
        assert_eq!(
            step.describe(),
            "cpu governor on 3 cpus: powersave/schedutil -> performance"
        );
    }

    #[test]
    fn the_available_governors_sit_beside_the_governor_of_each_cpu() {
        let governor = Path::new(CPU_DIR).join("cpu0").join(GOVERNOR_FILE);
        let available = governor.with_file_name(AVAILABLE_GOVERNORS_FILE);
        assert!(
            available.ends_with("cpu0/cpufreq/scaling_available_governors"),
            "{}",
            available.display()
        );
    }

    #[test]
    fn a_df_answer_needs_a_row_under_the_header() {
        let with_type = "Filesystem     Type 1K-blocks     Used Available Use% Mounted on\n\
                         /dev/sdd       ext4 1055762868 12345678 989999999   2% /\n";
        assert_eq!(read_df_row(with_type).as_deref(), Some("ext4 /dev/sdd"));
        let without = "Filesystem     1K-blocks     Used Available Use% Mounted on\n\
                       D:\\            1953497084 10000000 1943497084   1% /mnt/d\n";
        assert_eq!(read_df_row(without).as_deref(), Some("D:\\"));
        let header_only =
            "Filesystem 512-blocks Used Available Capacity iused ifree %iused Mounted on\n";
        assert_eq!(read_df_row(header_only), None);
    }

    #[test]
    fn a_value_is_found_after_its_label_and_trimmed() {
        let lscpu = "Architecture:  x86_64\nModel name:    AMD Ryzen 7 9700X 8-Core Processor \n";
        assert_eq!(
            find_value_after(lscpu, "Model name:").as_deref(),
            Some("AMD Ryzen 7 9700X 8-Core Processor")
        );
        assert_eq!(find_value_after(lscpu, "Vendor:"), None);
    }
}
