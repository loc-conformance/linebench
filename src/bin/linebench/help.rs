use std::path::Path;

use crate::output::{Color, paint};
use crate::shipped::{CORPORA, COUNTERS};

const HELP: &str = r#"linebench: measure line counters fairly, with the machine's state as context

linebench fetch [--counters all] [--corpus all] [--counters-dir <dir>] [--corpus-path <dir>]
                [--latest] [--allow-elevated] [--add <path>]

    Downloads what the other commands need and does nothing else. A counter arrives at the
    version its definition declares and a corpus at the commit its definition pins, so a second
    fetch answers "already here" for what matches and downloads again what does not.

    Say what to take. The word all takes everything there is.

    counters   {counters}
    corpora    {corpora}

    --counters a,b,c           the counters to download, or all
    --corpus a,b               the corpora to download, or all
    --counters-dir <dir>       where the binaries go
    --corpus-path <dir>        where a single corpus goes
    --latest                   take each counter's latest release and pin it beside the binary
    --allow-elevated           download as administrator or root, where there is no ordinary user
    --add <path>               a definition of your own, or a directory of them

    What arrived, its version and its sha256 go into linebench-fetched.toml beside the binaries.
    The counters land in the counters directory and a corpus in corpora/<name> next to it, which
    is where every later command looks for it with nothing said.

linebench check <corpus|dir> [--extensions rs,c] [--counters a,b,c] [--corpus-path <dir>]
                [--given <c>@<tag>=<path>] [--definition <c>@<tag>=<f>] [--args <c>@<tag>=<text>]
                [--expect-identical a=b] [--counters-dir <dir>] [--add <path>]

    Runs every counter once over the target, reads its counts back out of its own output, and
    holds them against each other and against the count the corpus definition declares. Nothing
    is timed, so it answers whether a run would mean anything before the run is paid for.

    The target is a corpus definition by name, counted where fetch put it, or a directory of
    your own, which takes --extensions to say what counts in it.

    --extensions rs,c          what to count in a directory of your own
    --counters a,b,c           the instances, in the order they are timed
    --corpus-path <dir>        where a named corpus sits, when it sits elsewhere
    --given <c>@<tag>=<path>   a build of your own, copied before it is measured
    --definition <c>@<tag>=<f> the definition that instance runs under
    --args <c>@<tag>=<text>    arguments of its own, right after the target
    --expect-identical a=b     instances whose JSON output has to match, volatile fields aside
    --counters-dir <dir>       where the binaries are
    --add <path>               a definition of your own, or a directory of them

linebench noise <corpus|dir> [--control <instance>] [--runs <n>] [--settle <s>] [--out <dir>]

    Times the control alone, five times, and samples what the machine was doing while it did.
    Its verdict is how far apart those five runs came out and how much of the machine was busy
    underneath them, which is what says whether a run made now would replicate.

    --control <instance>       the instance it times; default the one the conf names
    --runs <n>                 how many times to time it
    --settle <s>               seconds of quiet before each one

linebench run <corpus|dir> [--counters a,b,c] [--control <instance>] [--warmup <n>] [--runs <n>]
                [--settle <s>] [--against <stamp>] [--no-prep] [--yes] [--keep-raw]
                [--allow-unequal-exclusions] [--out <dir>] [--extensions rs,c]

    Times every instance over the target through hyperfine, twice, once with the flags that make
    the work equal and once with none, and writes the record, the csv files, the notes and the
    results page. The control is timed alone at both ends, and how far its two answers sit apart
    is the drift the record carries, which is the run's own claim about whether it replicates.

    --counters a,b,c           the instances, in the order they are timed
    --control <instance>       the instance timed alone at both ends
    --warmup <n>               hyperfine's warmup runs, default 3
    --runs <n>                 timed runs per instance, default 15
    --settle <s>               seconds of pause before each set, default 3
    --against <stamp>          a second since block read against that earlier run
    --no-prep                  leave the power scheme or the governor as it is
    --yes                      do not ask before measuring an unprepared machine
    --keep-raw                 keep every counter's printed output beside the record
    --allow-unequal-exclusions measure even with uneven MS Defender exclusions
    --out <dir>                where results go, default results/ here

    A run under an instance with args or a build of its own is written under results/local/,
    since its numbers answer for that build alone and never for the release.

linebench insights <corpus|dir> [--counters a,b,c] [--yes] [--out <dir>] [--extensions rs,c]

    Measures what a run cannot measure about itself, since watching a process closely enough
    disturbs the times it would report. The floor is what a counter costs before it has counted
    anything, the memory curve is what it held while it counted, and the system calls are how
    much it asked of the kernel to do it.

    --counters a,b,c           the instances, in the order they are measured
    --yes                      do not ask when a tool the section needs is missing
    --out <dir>                where the session goes, default results/ here

linebench report [--out <dir>]

    Rewrites the results page from the records that are already there, with nothing measured.

linebench version

    Prints this build's version.

A flag beats an environment variable, which beats linebench.conf. The conf is optional and holds
what belongs to this machine: control, counters, out, skip, given and the corpora that sit
somewhere other than where fetch puts them. A --help after a command prints that command alone.
Output to a terminal has colour and output to a file or a pipe does not."#;

#[cfg(feature = "maintenance")]
const BUMP_HELP: &str = r#"
linebench bump-versions [<counter>] [--json]

    Asks every channel what it publishes latest and, where that is ahead of the version a
    definition declares, writes the new one into counters/<name>.toml and leaves the rest of the
    file as it is. Named a counter, it does that for that one alone.

    --json                     print only what moved, as one document

    It maintains this repository's own definitions and nobody who measures needs it, so it is
    built with --features maintenance alone. A raised version is half a change: the definitions
    are compiled into the binary, so the binary is built again before anything measures the new
    version, and the same-work flags are read against that release's notes by a person.
"#;

const TOOL_AND_SPACE: &str = "linebench ";
const FLAG_OPENING: &str = "--";
#[cfg(feature = "maintenance")]
const CLOSING_NOTE: &str = "A flag beats an environment variable";
const COUNTERS_HERE: &str = "{counters}";
const CORPORA_HERE: &str = "{corpora}";
const LISTING_OPENINGS: [&str; 2] = ["    counters   ", "    corpora    "];

pub fn get_help() -> String {
    let whole = HELP
        .replace(COUNTERS_HERE, &name_them(COUNTERS))
        .replace(CORPORA_HERE, &name_them(CORPORA));
    #[cfg(not(feature = "maintenance"))]
    return whole;
    #[cfg(feature = "maintenance")]
    whole.replace(
        CLOSING_NOTE,
        &format!(
            "{BUMP_HELP}
{CLOSING_NOTE}"
        ),
    )
}

pub fn find_help_of(named: &str) -> String {
    let whole = get_help();
    let bare = format!("{TOOL_AND_SPACE}{named}");
    let opening = format!("{bare} ");
    let mut block: Vec<&str> = Vec::new();
    for line in whole
        .lines()
        .skip_while(|line| *line != bare && !line.starts_with(&opening))
    {
        if !block.is_empty() && !line.is_empty() && !line.starts_with(' ') {
            break;
        }
        block.push(line);
    }
    match block.is_empty() {
        true => whole,
        false => format!("{}\n", block.join("\n").trim_end()),
    }
}

pub fn name_every_command() -> String {
    let whole = get_help();
    let mut named: Vec<&str> = Vec::new();
    let mut inside = false;
    for line in whole.lines() {
        if line.starts_with(TOOL_AND_SPACE) {
            if !named.is_empty() {
                named.push("");
            }
            inside = true;
        } else if line.trim().is_empty() {
            inside = false;
        }
        if inside {
            named.push(line);
        }
    }
    named.join("\n")
}

pub fn paint_help(text: &str) -> String {
    let mut painted: Vec<String> = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        if line.starts_with(TOOL_AND_SPACE) {
            inside = true;
        } else if line.trim().is_empty() {
            inside = false;
        }
        let listed = LISTING_OPENINGS
            .iter()
            .any(|opening| line.starts_with(opening));
        painted.push(paint_help_line(line, inside || listed));
    }
    painted.join("\n")
}

fn name_them(shipped: &[(&str, &str)]) -> String {
    shipped
        .iter()
        .filter_map(|(path, _)| Path::new(path).file_stem())
        .map(|stem| stem.to_string_lossy().into_owned())
        .collect::<Vec<String>>()
        .join(", ")
}

fn paint_help_line(line: &str, inside: bool) -> String {
    if line.trim().is_empty() {
        return line.to_string();
    }
    let signature = line.starts_with(TOOL_AND_SPACE);
    let ground = (!inside).then_some(Color::Grey);
    let mut runs: Vec<(Option<Color>, String)> = Vec::new();
    for (index, chunk) in line.split_inclusive(' ').enumerate() {
        if signature && index == 1 {
            hold(&mut runs, Some(Color::Blue), chunk);
            continue;
        }
        match find_flag(chunk) {
            Some((start, end)) => {
                hold(&mut runs, ground, &chunk[..start]);
                hold(&mut runs, Some(Color::Green), &chunk[start..end]);
                hold(&mut runs, ground, &chunk[end..]);
            }
            None => hold(&mut runs, ground, chunk),
        }
    }
    runs.iter()
        .map(|(color, text)| tinted(text, *color))
        .collect()
}

fn hold(runs: &mut Vec<(Option<Color>, String)>, color: Option<Color>, text: &str) {
    if text.is_empty() {
        return;
    }
    match runs.last_mut() {
        Some((held, so_far)) if *held == color => so_far.push_str(text),
        _ => runs.push((color, text.to_string())),
    }
}

fn find_flag(chunk: &str) -> Option<(usize, usize)> {
    let start = chunk.find(FLAG_OPENING)?;
    if !chunk[..start].chars().all(|opening| opening == '[') {
        return None;
    }
    let end = chunk[start..]
        .find(|letter: char| !(letter.is_ascii_alphanumeric() || letter == '-'))
        .map_or(chunk.len(), |offset| start + offset);
    Some((start, end))
}

fn tinted(text: &str, color: Option<Color>) -> String {
    match color {
        Some(color) if !text.is_empty() => paint(color, text).to_string(),
        _ => text.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use crate::config::COMMANDS;

    use super::*;

    #[test]
    fn every_command_the_parser_takes_has_a_block_of_its_own() {
        for named in COMMANDS {
            let block = find_help_of(named);
            let opening = format!("{TOOL_AND_SPACE}{named}");
            assert!(block.starts_with(&opening), "{named}: {block}");
        }
    }

    #[test]
    fn a_block_carries_its_own_command_and_neither_another_nor_the_closing_note() {
        let fetch = find_help_of("fetch");
        assert!(fetch.contains("--counters-dir"), "{fetch}");
        assert!(fetch.contains("linebench-fetched.toml"), "{fetch}");
        assert!(!fetch.contains("linebench check "), "{fetch}");
        assert!(!fetch.contains("A flag beats"), "{fetch}");
    }

    #[test]
    fn a_command_whose_line_is_its_bare_name_is_cut_like_the_rest() {
        let version = find_help_of("version");
        assert!(version.starts_with("linebench version\n"), "{version}");
        assert!(version.contains("Prints this build's version"), "{version}");
        assert!(!version.contains("linebench report"), "{version}");
    }

    #[test]
    fn the_fetch_block_names_the_counters_and_corpora_that_are_shipped() {
        let whole = get_help();
        assert!(!whole.contains(COUNTERS_HERE), "{whole}");
        assert!(!whole.contains(CORPORA_HERE), "{whole}");
        let fetch = find_help_of("fetch");
        assert!(fetch.contains(&name_them(COUNTERS)), "{fetch}");
        assert!(fetch.contains(&name_them(CORPORA)), "{fetch}");
        assert!(name_them(COUNTERS).contains("cloc, mezura"), "{fetch}");
    }

    #[test]
    fn the_lines_that_carry_the_names_are_the_ones_the_painter_leaves_plain() {
        let whole = get_help();
        for opening in LISTING_OPENINGS {
            assert!(whole.contains(opening), "{opening} is gone from the help");
        }
    }

    #[test]
    fn the_list_of_commands_is_every_signature_and_no_description() {
        let named = name_every_command();
        for command in COMMANDS {
            let opening = format!("{TOOL_AND_SPACE}{command}");
            assert!(
                named.contains(&opening),
                "{command} is missing from {named}"
            );
        }
        assert!(!named.contains("Prints this build's version"), "{named}");
        assert!(named.contains("[--allow-elevated]"), "{named}");
    }
}
