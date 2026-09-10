use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;

use linebench::counters::read_definitions;
use linebench::latest::{Standing, describe_latest, find_latest_release, judge_latest};
use linebench::measure::print_line;

use crate::output::{Color, paint};

const ACQUISITION_BLOCK: &str = "[acquisition]";
const VERSION_KEY: &str = "version";
const NO_CHANNEL: &str = "says nothing about where it is fetched from, so there is no release to \
                          look up";
const NOTHING_MOVED: &str = "[]";

pub fn run_bump_versions(
    out: &mut dyn Write,
    dir: &Path,
    only: Option<&str>,
    as_json: bool,
) -> Result<i32, String> {
    let definitions = read_definitions(dir)?;
    if let Some(name) = only
        && !definitions.iter().any(|definition| definition.name == name)
    {
        return Err(format!(
            "no counter definition named {name} in {}",
            dir.display()
        ));
    }
    let mut moved: Vec<Moved> = Vec::new();
    let mut failed = Vec::new();
    for definition in &definitions {
        if only.is_some_and(|name| name != definition.name) {
            continue;
        }
        let mut say = |line: String| -> Result<(), String> {
            let said = format!("{}: {line}", definition.name);
            match as_json {
                true => {
                    eprintln!("{said}");
                    Ok(())
                }
                false => print_line(out, &said),
            }
        };
        let Some(how) = &definition.acquisition else {
            say(NO_CHANNEL.to_string())?;
            continue;
        };
        let latest = match find_latest_release(definition) {
            Ok(latest) => latest,
            Err(refused) => {
                failed.push(definition.name.clone());
                say(paint(Color::Red, &refused).to_string())?;
                continue;
            }
        };
        if judge_latest(how, &latest) != Standing::Newer {
            say(describe_latest(&definition.name, how, None, &latest))?;
            continue;
        }
        let file = match write_the_version_into(dir, &definition.name, &latest) {
            Ok(file) => file,
            Err(refused) => {
                failed.push(definition.name.clone());
                say(paint(Color::Red, &refused).to_string())?;
                continue;
            }
        };
        say(format!(
            "{} raised to {} in {}",
            how.version,
            paint(Color::Green, &latest),
            file.display()
        ))?;
        moved.push(Moved {
            counter: definition.name.clone(),
            from: how.version.clone(),
            to: latest,
        });
    }
    if as_json {
        let document = serde_json::to_string(&moved).unwrap_or_else(|_| NOTHING_MOVED.to_string());
        print_line(out, &document)?;
    }
    Ok(i32::from(!failed.is_empty()))
}

#[derive(Debug, Serialize)]
struct Moved {
    counter: String,
    from: String,
    to: String,
}

fn write_the_version_into(dir: &Path, name: &str, to: &str) -> Result<PathBuf, String> {
    let file = dir.join(format!("{name}.toml"));
    let held = fs::read_to_string(&file)
        .map_err(|error| format!("{} could not be read: {error}", file.display()))?;
    let raised = raise_the_version_line(&held, to)
        .map_err(|refused| format!("{}: {refused}", file.display()))?;
    fs::write(&file, raised)
        .map_err(|error| format!("{} could not be written: {error}", file.display()))?;
    Ok(file)
}

// Only the value between the quotes moves. Writing the file back through the serialiser would
// reflow every line, and these are aligned by hand.
fn raise_the_version_line(held: &str, to: &str) -> Result<String, String> {
    let mut inside = false;
    let mut raised = false;
    let mut lines: Vec<String> = Vec::new();
    for line in held.lines() {
        let opens_a_block = line.trim_start().starts_with('[');
        if opens_a_block {
            inside = line.trim() == ACQUISITION_BLOCK;
        }
        let names_the_version = line
            .split_once('=')
            .is_some_and(|(key, _)| key.trim() == VERSION_KEY);
        if !inside || raised || opens_a_block || !names_the_version {
            lines.push(line.to_string());
            continue;
        }
        let opened = line
            .find('"')
            .ok_or("its version is not written between quotes")?;
        let closed = line[opened + 1..]
            .find('"')
            .map(|at| at + opened + 1)
            .ok_or("its version opens a quote that nothing closes")?;
        lines.push(format!("{}{to}{}", &line[..=opened], &line[closed..]));
        raised = true;
    }
    if !raised {
        return Err(format!(
            "it has no {VERSION_KEY} line under {ACQUISITION_BLOCK}"
        ));
    }
    let mut written = lines.join("\n");
    if held.ends_with('\n') {
        written.push('\n');
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    const A_DEFINITION: &str = "\
name         = \"scc\"
version-flag = \"--version\"

[acquisition]
channel = \"github-release-asset\"
name    = \"boyter/scc\"
version = \"4.0.0\"

[run]
args = [\"{target}\"]
";

    #[test]
    fn only_the_version_between_the_quotes_moves_and_the_file_keeps_its_shape() {
        let raised = raise_the_version_line(A_DEFINITION, "4.1.0").unwrap();
        assert_eq!(raised, A_DEFINITION.replace("\"4.0.0\"", "\"4.1.0\""));
        assert!(raised.ends_with("args = [\"{target}\"]\n"), "{raised}");
    }

    #[test]
    fn a_version_outside_the_acquisition_block_is_left_alone() {
        let elsewhere = A_DEFINITION.replace("args = [\"{target}\"]", "version = \"1.0.0\"");
        let raised = raise_the_version_line(&elsewhere, "4.1.0").unwrap();
        assert!(raised.contains("version = \"4.1.0\""), "{raised}");
        assert!(raised.contains("version = \"1.0.0\""), "{raised}");
    }

    #[test]
    fn a_definition_with_no_acquisition_block_is_refused_before_anything_is_written() {
        let none = "name = \"mezura\"\n\n[run]\nargs = [\"{target}\"]\n";
        let refused = raise_the_version_line(none, "3.1.0").unwrap_err();
        assert!(refused.contains(ACQUISITION_BLOCK), "{refused}");
    }
}
