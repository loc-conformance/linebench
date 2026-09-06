use std::io::{self, IsTerminal, Write};
use std::process;

use linebench::machine::{Platform, PrepStep, explain_how_to_elevate};

use crate::output::{print_header, print_line, print_warning};

pub struct AppliedPrep {
    steps: Vec<PrepStep>,
    applied: Vec<String>,
}

impl AppliedPrep {
    pub fn get_applied_steps(&self) -> Vec<String> {
        self.applied.clone()
    }
}

impl Drop for AppliedPrep {
    fn drop(&mut self) {
        restore_all(&self.steps);
    }
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
        "and put it back when the run ends, fails or is interrupted. To do that:",
    )?;
    print_line(out, &format!("\n   {how}\n"))?;
    if yes {
        return print_line(out, "--yes given, carrying on.");
    }
    if !io::stdin().is_terminal() {
        return print_line(out, "no terminal to ask at, carrying on.");
    }
    let _ = write!(out, "Continue anyway, without it? [y/N] ");
    let _ = out.flush();
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
    platform: Platform,
) -> Result<Option<AppliedPrep>, String> {
    if plan.is_empty() {
        return Ok(None);
    }
    print_header(out, "== machine preparation")?;
    for step in &plan {
        print_line(out, &format!("   {}", step.describe()))?;
    }
    print_line(
        out,
        &format!(
            "   if this run dies without putting it back, by hand:\n   {}",
            format_manual_restore(&plan, platform)
        ),
    )?;
    let on_interrupt = plan.clone();
    ctrlc::set_handler(move || {
        restore_all(&on_interrupt);
        eprintln!("\ninterrupted. anything that was changed on the machine has been put back.");
        process::exit(130);
    })
    .map_err(|error| format!("the Ctrl-C handler could not be installed: {error}"))?;
    let mut prep = AppliedPrep {
        steps: plan,
        applied: Vec::new(),
    };
    for step in &prep.steps {
        let problems = step.apply();
        prep.applied.extend(describe_outcome(step, problems.len()));
        for problem in problems {
            print_warning(out, &problem)?;
        }
    }
    Ok(Some(prep))
}

pub fn format_manual_restore(steps: &[PrepStep], platform: Platform) -> String {
    steps
        .iter()
        .map(|step| match step {
            PrepStep::PowerScheme { was, .. } => format!("powercfg /setactive {was}"),
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

fn describe_outcome(step: &PrepStep, refused: usize) -> Option<String> {
    let parts = step.count_parts();
    match refused {
        0 => Some(step.describe()),
        partly if partly < parts => {
            Some(format!("{}, {partly} of {parts} refused", step.describe()))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn a_step_that_partly_took_is_recorded_with_how_much_refused_and_one_that_did_not_is_not() {
        let governors = BTreeMap::from([
            (PathBuf::from("/sys/cpu0"), "schedutil".to_string()),
            (PathBuf::from("/sys/cpu1"), "schedutil".to_string()),
            (PathBuf::from("/sys/cpu2"), "schedutil".to_string()),
        ]);
        let step = PrepStep::CpuGovernor { governors };
        assert_eq!(
            describe_outcome(&step, 0).as_deref(),
            Some("cpu governor on 3 cpus: schedutil -> performance")
        );
        assert_eq!(
            describe_outcome(&step, 1).as_deref(),
            Some("cpu governor on 3 cpus: schedutil -> performance, 1 of 3 refused")
        );
        assert_eq!(describe_outcome(&step, 3), None);
    }
}
