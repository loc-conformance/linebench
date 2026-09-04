use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process;

use serde::{Deserialize, Serialize};

use linebench::machine::{Platform, PrepStep, explain_how_to_elevate};

use crate::output::{print_header, print_line, print_warning};

pub const PREP_FILE: &str = "linebench-prep.toml";

#[derive(Debug, Default, Serialize, Deserialize)]
struct SavedPrep {
    #[serde(default)]
    steps: Vec<PrepStep>,
}

pub struct AppliedPrep {
    steps: Vec<PrepStep>,
    file: PathBuf,
}

impl Drop for AppliedPrep {
    fn drop(&mut self) {
        restore_all(&self.steps);
        let _ = fs::remove_file(&self.file);
    }
}

pub fn restore_after_interrupted_run(
    out: &mut dyn Write,
    counters_dir: &Path,
    privileged: bool,
    platform: Platform,
) -> Result<(), String> {
    let file = counters_dir.join(PREP_FILE);
    if !file.is_file() {
        return Ok(());
    }
    let text = fs::read_to_string(&file)
        .map_err(|error| format!("{} could not be read: {error}", file.display()))?;
    let saved: SavedPrep =
        toml::from_str(&text).map_err(|error| format!("{}: {error}", file.display()))?;
    if saved.steps.is_empty() {
        let _ = fs::remove_file(&file);
        return Ok(());
    }
    let described: Vec<String> = saved.steps.iter().map(|step| step.describe()).collect();
    if privileged {
        restore_all(&saved.steps);
        let _ = fs::remove_file(&file);
        return print_warning(
            out,
            &format!(
                "an earlier run was interrupted after it set {}; it has been put back now",
                described.join(" and ")
            ),
        );
    }
    print_warning(
        out,
        &format!(
            "an earlier run was interrupted after it set {}, and this one cannot put it back \
             without {}. Run this as {} once, or by hand:\n  {}",
            described.join(" and "),
            explain_how_to_elevate(platform).0,
            explain_how_to_elevate(platform).0,
            format_manual_restore(&saved.steps, platform)
        ),
    )
}

pub fn announce_prep(
    out: &mut dyn Write,
    plan: &[PrepStep],
    privileged: bool,
    yes: bool,
    platform: Platform,
) -> Result<(), String> {
    if plan.is_empty() || privileged {
        return Ok(());
    }
    let (authority, how) = explain_how_to_elevate(platform);
    print_line(out, "")?;
    print_line(
        out,
        &format!("Not running as {authority}, so the machine is measured exactly as it is now."),
    )?;
    print_line(out, &format!("As {authority} this run would first set:"))?;
    for step in plan {
        print_line(out, &format!("   {}", step.describe()))?;
    }
    print_line(
        out,
        "and put it back at the end, however the run ends. To do that:",
    )?;
    print_line(out, &format!("\n   {how}\n"))?;
    if yes {
        return print_line(out, "--yes given, carrying on.");
    }
    if !io::stdin().is_terminal() {
        return print_line(out, "no terminal to ask at, carrying on.");
    }
    print!("Continue anyway, without it? [y/N] ");
    let _ = io::stdout().flush();
    let mut answer = String::new();
    let _ = io::stdin().read_line(&mut answer);
    match answer.trim().to_lowercase().as_str() {
        "y" | "yes" => Ok(()),
        _ => Err("stopped.".to_string()),
    }
}

pub fn apply_prep(
    out: &mut dyn Write,
    plan: Vec<PrepStep>,
    counters_dir: &Path,
    platform: Platform,
) -> Result<Option<AppliedPrep>, String> {
    if plan.is_empty() {
        return Ok(None);
    }
    print_header(out, "== machine preparation")?;
    let file = counters_dir.join(PREP_FILE);
    let saved = SavedPrep {
        steps: plan.clone(),
    };
    let text = toml::to_string(&saved)
        .map_err(|error| format!("the prep file could not be written out: {error}"))?;
    fs::write(&file, text)
        .map_err(|error| format!("{} could not be written: {error}", file.display()))?;
    for step in &plan {
        print_line(out, &format!("   {}", step.describe()))?;
        for problem in step.apply() {
            print_warning(out, &problem)?;
        }
    }
    print_line(
        out,
        &format!(
            "   if this run dies without putting it back, by hand:\n   {}",
            format_manual_restore(&plan, platform)
        ),
    )?;
    let on_interrupt = plan.clone();
    let file_on_interrupt = file.clone();
    ctrlc::set_handler(move || {
        restore_all(&on_interrupt);
        let _ = fs::remove_file(&file_on_interrupt);
        eprintln!("\ninterrupted. anything that was changed on the machine has been put back.");
        process::exit(130);
    })
    .map_err(|error| format!("the Ctrl-C handler could not be installed: {error}"))?;
    Ok(Some(AppliedPrep { steps: plan, file }))
}

pub fn format_manual_restore(steps: &[PrepStep], platform: Platform) -> String {
    steps
        .iter()
        .map(|step| match step {
            PrepStep::PowerScheme { was } => format!("powercfg /setactive {was}"),
            PrepStep::CpuGovernor { governors } => {
                let was = governors.values().next().cloned().unwrap_or_default();
                match platform {
                    Platform::Windows => format!("set the cpu governor back to {was}"),
                    _ => format!(
                        "for f in /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor; do echo {was} | sudo tee $f; done"
                    ),
                }
            }
        })
        .collect::<Vec<_>>()
        .join("\n   ")
}

fn restore_all(steps: &[PrepStep]) {
    for step in steps {
        step.restore();
    }
}
