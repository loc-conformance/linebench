#![deny(unsafe_code)]

mod commands;
mod config;
mod instances;
mod output;
mod page;
mod prep;
mod shipped;

use std::env;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process;

use linebench::corpus::Corpus;
use linebench::counters::Definition;
use linebench::fetch::read_manifest;
use linebench::machine::{Platform, detect_platform};
use linebench::newest::apply_newest_pins;

use crate::config::{
    Command, Config, Locations, Options, check_skip_names, find_config, find_data_dir, parse_args,
    read_config, resolve_fetch, resolve_locations, resolve_out,
};
use crate::output::{Color, Output, enable_colors, paint, paint_help, print_line};
use crate::shipped::collect_definitions;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const HELP: &str = r#"linebench: measure line counters fairly, with the machine's state as context

linebench fetch [--counters all] [--corpus all] [--counters-dir <dir>] [--corpus-path <dir>]
                [--newest] [--allow-elevated] [--add <path>]

    Downloads what the other commands need and does nothing else. A counter arrives at the
    version its definition declares and a corpus at the commit its definition pins, so a second
    fetch answers "already here" for what matches and downloads again what does not.

    Say what to take. The word all takes everything there is.

    --counters a,b,c           the counters to download, or all
    --corpus a,b               the corpora to download, or all
    --counters-dir <dir>       where the binaries go
    --corpus-path <dir>        where a single corpus goes
    --newest                   take each counter's newest release and pin it beside the binary
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

linebench help

    This.

linebench version

    Prints this build's version.

A flag beats an environment variable, which beats linebench.conf. The conf is optional and holds
what belongs to this machine: control, counters, out, skip, given and the corpora that sit
somewhere other than where fetch puts them. Output to a terminal has colour and output to a file
or a pipe does not."#;

fn main() {
    enable_colors();
    let code = match dispatch() {
        Ok(code) => code,
        Err(message) => {
            eprintln!("{}", paint(Color::Red, &message));
            2
        }
    };
    process::exit(code);
}

fn dispatch() -> Result<i32, String> {
    let args: Vec<String> = env::args().collect();
    let options = parse_args(&args)?;
    let mut out = Output::new();
    match options.command.unwrap_or(Command::Help) {
        Command::Help => print_line(&mut out, &paint_help(HELP)).map(|_| 0),
        Command::Version => print_line(&mut out, &format!("linebench {VERSION}")).map(|_| 0),
        Command::Report => {
            let config = read_config(&find_config())?;
            commands::run_report(&mut out, &resolve_out(&options, &config))
        }
        Command::Fetch => {
            let ground = read_ground(&mut out, &options)?;
            let plan = resolve_fetch(
                &options,
                &ground.config,
                &ground.config_path,
                &ground.corpora,
                &ground.counters,
                ground.data_dir.as_deref(),
            )?;
            commands::run_fetch(&mut out, &options, &plan, &ground.counters, ground.platform)
        }
        Command::Check => {
            let resolved = resolve_everything(&mut out, &options)?;
            commands::run_check(
                &mut out,
                &options,
                &resolved.locations,
                &resolved.definitions,
                resolved.platform,
            )
        }
        Command::Noise => {
            let resolved = resolve_everything(&mut out, &options)?;
            commands::run_noise(
                &mut out,
                &options,
                &resolved.locations,
                &resolved.definitions,
                resolved.platform,
            )
        }
        Command::Insights => {
            let resolved = resolve_everything(&mut out, &options)?;
            commands::run_insights(
                &mut out,
                &options,
                &resolved.locations,
                &resolved.definitions,
                resolved.platform,
            )
        }
        Command::Run => {
            let resolved = resolve_everything(&mut out, &options)?;
            commands::run_benchmark(
                &mut out,
                &options,
                &resolved.locations,
                &resolved.definitions,
                resolved.platform,
            )
        }
    }
}

struct Resolved {
    platform: Platform,
    locations: Locations,
    definitions: Vec<Definition>,
}

struct Ground {
    platform: Platform,
    data_dir: Option<PathBuf>,
    config_path: PathBuf,
    config: Config,
    counters: Vec<Definition>,
    corpora: Vec<Corpus>,
}

fn read_ground(out: &mut dyn Write, options: &Options) -> Result<Ground, String> {
    let platform = detect_platform()?;
    let data_dir = find_data_dir();
    if let Some(dir) = &data_dir {
        let _ = fs::create_dir_all(dir);
    }
    let config_path = find_config();
    let config = read_config(&config_path)?;
    let added: Vec<PathBuf> = config.add.iter().chain(&options.add).cloned().collect();
    let definitions = collect_definitions(out, &added)?;
    check_skip_names(&config.skip, &definitions.counters, &config_path)?;
    Ok(Ground {
        platform,
        data_dir,
        config_path,
        config,
        counters: definitions.counters,
        corpora: definitions.corpora,
    })
}

fn resolve_everything(out: &mut dyn Write, options: &Options) -> Result<Resolved, String> {
    let ground = read_ground(out, options)?;
    let Ground {
        platform,
        data_dir,
        config_path,
        config,
        counters,
        corpora,
    } = ground;
    let mut definitions = counters;
    let locations = resolve_locations(
        options,
        &config,
        &config_path,
        &corpora,
        data_dir.as_deref(),
    )?;
    let manifest = read_manifest(&locations.counters_dir)?;
    for line in apply_newest_pins(&mut definitions, &manifest) {
        print_line(out, &line)?;
    }
    Ok(Resolved {
        platform,
        locations,
        definitions,
    })
}
