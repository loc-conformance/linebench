use std::path::Path;

use crate::output::{Color, paint};
use crate::shipped::{CORPORA, COUNTERS};

const HELP: &str = r#"linebench: time line counters on equal work, and record the machine's state

linebench fetch [--counters all] [--corpus all] [--counters-dir <dir>] [--corpus-path <dir>]
                [--latest] [--allow-elevated] [--add <path>]

    Downloads what the other commands need and measures nothing. A counter arrives at the version
    its definition declares and a corpus at the commit its definition pins, so a second fetch
    answers "already here" for what matches and downloads again what does not.

    Say what to take. The word all takes everything there is.

    counters   {counters}
    corpora    {corpora}

    --counters a,b,c           the counters to download, or all
    --corpus a,b               the corpora to download, or all
    --counters-dir <dir>       where the binaries go
    --corpus-path <dir>        the folder the corpora sit in, or one corpus's own checkout
    --latest                   take each counter's latest release and pin it beside the binary
    --allow-elevated           download as root, where there is no ordinary user
    --add <path>               a definition of your own, or a directory of them

    What arrived, its version and its sha256 go into linebench-fetched.toml beside the binaries.
    The counters land in the counters directory and a corpus in corpora/<name> next to it, which
    is where every later command looks for it with nothing said.

linebench check <corpora|all|dir> [--extensions rs,c] [--counters a,b,c] [--control <instance>]
                [--allow-unequal-exclusions] [--corpus-path <dir>] [--given <c>@<tag>=<path>]
                [--definition <c>@<tag>=<f>] [--args <c>@<tag>=<text>] [--counters-dir <dir>]
                [--add <path>]

    Runs every counter once over the target, reads its counts back out of its own output, and
    holds them against each other and against the count the corpus definition declares. Nothing
    is timed, so it answers whether a run would mean anything before one is paid for.

    The target is a corpus definition by name, counted where fetch put it, several of them
    separated by commas, all for every corpus this machine holds, or a directory of your own,
    which takes --extensions to say what counts in it. A list is checked one corpus at a time.

    --extensions rs,c          what to count in a directory of your own
    --counters a,b,c           the instances, in the order they run
    --control <instance>       the instance hyperfine is tried with
    --allow-unequal-exclusions pass even with uneven MS Defender exclusions
    --corpus-path <dir>        the folder the corpora sit in, or one corpus's own checkout
    --given <c>@<tag>=<path>   a build of your own, copied before it is measured
    --definition <c>@<tag>=<f> the definition that instance runs under
    --args <c>@<tag>=<text>    arguments of its own, right after the target
    --counters-dir <dir>       where the binaries are
    --add <path>               a definition of your own, or a directory of them

linebench noise <corpus|dir> [--control <instance>] [--counters a,b,c] [--runs <n>]
                [--settle <s>] [--extensions rs,c] [--given <c>@<tag>=<path>]
                [--definition <c>@<tag>=<f>] [--args <c>@<tag>=<text>] [--counters-dir <dir>]
                [--add <path>] [--corpus-path <dir>]

    Times the control alone, five times by default, and samples what else the machine was doing
    meanwhile. The verdict is how far apart those runs came out and how much of the machine was
    busy under them, which says whether a run made now would replicate.

    --control <instance>       the instance it times, by default the one the conf names
    --counters a,b,c           the instances the control is chosen from
    --runs <n>                 how many times to time it, default 5, at least 3
    --settle <s>               seconds of quiet before each one, default none
    --extensions rs,c          what to count in a directory of your own
    --given <c>@<tag>=<path>   a build of your own, copied before it is measured
    --definition <c>@<tag>=<f> the definition that instance runs under
    --args <c>@<tag>=<text>    arguments of its own, right after the target
    --counters-dir <dir>       where the binaries are
    --add <path>               a definition of your own, or a directory of them
    --corpus-path <dir>        the folder the corpora sit in, or one corpus's own checkout

linebench run <corpora|all|dir> [--counters a,b,c] [--control <instance>] [--warmup <n>]
                [--runs <n>] [--settle <s>] [--against <stamp>] [--no-prep] [--yes] [--keep-raw]
                [--allow-unequal-exclusions] [--out <dir>] [--extensions rs,c]
                [--given <c>@<tag>=<path>] [--definition <c>@<tag>=<f>] [--args <c>@<tag>=<text>]
                [--expect-identical a=b] [--counters-dir <dir>] [--add <path>]
                [--corpus-path <dir>]

    Times every instance over the target through hyperfine, twice, once with the flags that make
    the work equal and once with none, and writes the record, the csv files, the notes and the
    results page. The control is timed alone at both ends, and the gap between its two answers is
    the drift the record carries: the run's own claim about whether it replicates.

    Several corpora, run linux,cpython or run all, are measured one after the other in one
    process. The machine is prepared once, each corpus keeps its own run directory and its own
    record, and every summary is printed again together at the end. A corpus that fails is named
    and the rest carry on. all is every corpus definition with a checkout on this machine, and
    the ones missing are named in a line of their own.

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
    --extensions rs,c          what to count in a directory of your own
    --given <c>@<tag>=<path>   a build of your own, copied before it is measured
    --definition <c>@<tag>=<f> the definition that instance runs under
    --args <c>@<tag>=<text>    arguments of its own, right after the target
    --expect-identical a=b     instances whose JSON output has to match, volatile fields aside
    --counters-dir <dir>       where the binaries are
    --add <path>               a definition of your own, or a directory of them
    --corpus-path <dir>        the folder the corpora sit in, or one corpus's own checkout

    A run under an instance with args or a build of its own is written under results/local/,
    since its numbers answer for that build alone.

linebench insights <corpus|dir> [--counters a,b,c] [--only floor,memory,syscalls] [--yes]
                [--allow-unequal-exclusions] [--out <dir>] [--extensions rs,c]
                [--given <c>@<tag>=<path>] [--definition <c>@<tag>=<f>] [--args <c>@<tag>=<text>]
                [--counters-dir <dir>] [--add <path>] [--corpus-path <dir>]

    Measures what a run cannot measure about itself, since watching a process closely enough
    disturbs the times it would report. The floor is what a counter costs before it has counted
    anything, the memory curve is what it held while it counted, and the system calls are how
    much it asked of the kernel to do it.

    --counters a,b,c           the instances, in the order they are measured
    --only <parts>             which of floor, memory and syscalls to measure, default all
    --yes                      do not ask when a tool the section needs is missing
    --allow-unequal-exclusions measure even with uneven MS Defender exclusions
    --out <dir>                where the session goes, default results/ here
    --extensions rs,c          what to count in a directory of your own
    --given <c>@<tag>=<path>   a build of your own, copied before it is measured
    --definition <c>@<tag>=<f> the definition that instance runs under
    --args <c>@<tag>=<text>    arguments of its own, right after the target
    --counters-dir <dir>       where the binaries are
    --add <path>               a definition of your own, or a directory of them
    --corpus-path <dir>        the folder the corpora sit in, or one corpus's own checkout

linebench status [--counters-dir <dir>] [--add <path>]

    Says what this machine holds. Every counter with the version its definition pins, what its
    channel publishes latest, and what sits in the counters directory. Every corpus with the path
    of its checkout and the commit there. Then the paths all of that came from.

    --counters-dir <dir>       where the binaries are
    --add <path>               a definition of your own, or a directory of them

linebench report [--out <dir>] [--verify]

    Rewrites the results page from the records that are already there, with nothing measured.

    --out <dir>                where the results are, default results/ here
    --verify                   build the page and say whether the one on disk is that page,
                               writing nothing

linebench verify <a run directory|an insights directory>

    Reads a run back and holds its numbers against each other: every measurement against itself,
    the derived columns against the ones they come from, the counts against their sum, equal work
    re-judged, and the csv files rebuilt from the record. Where the hyperfine exports were kept
    too, every statistic is recomputed from the time of every single execution.

    What one insights command wrote is read the same way. The floor against itself and against its
    exports, every curve against the peak the system reported, every traced instance against the
    calls it listed, and every number against the instances that were measured.

    Name the directory, or the run.json or insights.json inside it. A folder holding many of them
    is not one, and the refusal says to name a directory further in.

    What could not be read is printed. A check that fails exits 1, and a missing file is reported
    as a gap and does not.

Everywhere:

    --dry-run                  what the command would write goes to the temp folder and is
                               deleted at the end, and no release is looked up. The machine is
                               prepared as always, so a dry run takes as long as the real one
    --help, -h                 this text, or one command's own after its name
    --version, -V              print this build's version

A flag beats an environment variable, which beats linebench.conf.
"#;

#[cfg(feature = "maintenance")]
const BUMP_HELP: &str = r#"
linebench bump-versions [<counter>] [--json]

    Asks every channel what it publishes latest and, where that is ahead of the version a
    definition declares, writes the new one into counters/<name>.toml and leaves the rest of the
    file as it is. Named a counter, it does that for that one alone.

    --json                     print only what moved, as one document

    It maintains this repository's own definitions and nobody who measures needs it, so it is
    built with --features maintenance alone. A raised version is half a change: the definitions
    are compiled into the binary, so it has to be rebuilt before anything measures the new
    version, and the same-work flags are read against that release's notes by a person.
"#;

const TOOL_AND_SPACE: &str = "linebench ";
#[cfg(feature = "maintenance")]
const EVERYWHERE: &str = "Everywhere:";
const FLAG_OPENING: &str = "--";
/// Flags of no command in particular, which is why no usage line names them.
const ALWAYS: [&str; 5] = ["--dry-run", "--help", "-h", "--version", "-V"];
/// A usage line carries the long spelling alone, and the short one goes wherever it goes.
const SHORT: [(&str, &str); 1] = [("--yes", "-y")];
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
        EVERYWHERE,
        &format!("{}\n{EVERYWHERE}", BUMP_HELP.trim_start()),
    )
}

/// The flags a command takes, read off the usage line of its own help block, so that the text and
/// what the parser allows cannot drift apart. ALWAYS is the handful that belong to no command.
pub fn find_flags_of(named: &str) -> Vec<String> {
    let block = find_block_of(named).unwrap_or_default();
    let usage = block
        .lines()
        .take_while(|line| !line.trim().is_empty())
        .collect::<Vec<&str>>()
        .join(" ");
    let mut flags: Vec<String> = ALWAYS.iter().map(|flag| (*flag).to_string()).collect();
    for word in usage.split(['[', ']', ' ']) {
        if word.starts_with(FLAG_OPENING) && !flags.iter().any(|held| held == word) {
            flags.push(word.to_string());
            if let Some((_, short)) = SHORT.iter().find(|(long, _)| long == &word) {
                flags.push((*short).to_string());
            }
        }
    }
    flags
}

pub fn find_help_of(named: &str) -> String {
    find_block_of(named).unwrap_or_else(get_help)
}

/// Nothing for a command with no block of its own, which today is only the version behind its
/// flag. A caller that would print the whole help in its place had better say nothing instead.
pub fn find_block_of(named: &str) -> Option<String> {
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
        true => None,
        false => Some(format!("{}\n", block.join("\n").trim_end())),
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
    fn the_usage_line_of_a_command_and_the_flags_listed_under_it_name_the_same_flags() {
        for named in COMMANDS {
            let block = find_help_of(named);
            let mut listed: Vec<&str> = block
                .lines()
                .filter_map(|line| line.strip_prefix("    "))
                .filter(|line| line.starts_with(FLAG_OPENING))
                .filter_map(|line| line.split_whitespace().next())
                .collect();
            listed.sort_unstable();
            let mut taken: Vec<String> = find_flags_of(named)
                .into_iter()
                .filter(|flag| !ALWAYS.contains(&flag.as_str()))
                .filter(|flag| !SHORT.iter().any(|(_, short)| short == flag))
                .collect();
            taken.sort();
            assert_eq!(listed, taken, "{named}");
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
    fn one_command_is_cut_out_of_the_help_without_what_follows_it() {
        let status = find_help_of("status");
        assert!(status.starts_with("linebench status "), "{status}");
        assert!(status.contains("Says what this machine holds"), "{status}");
        assert!(!status.contains("linebench report"), "{status}");
        assert!(!status.contains("Everywhere:"), "{status}");
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
