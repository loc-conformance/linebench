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

use linebench::counters::Definition;
use linebench::machine::{Platform, detect_platform};

use crate::config::{
    Command, Locations, Options, find_config, find_data_dir, parse_args, read_config,
    resolve_locations, resolve_out,
};
use crate::output::{Color, Output, enable_colors, paint, print_line};
use crate::shipped::collect_definitions;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const HELP: &str = "\
linebench: measure line counters on equal work, with the machine's state beside every number

usage: linebench <command> [flags]

commands
  setup     fetch every counter at the version its definition declares, and the corpus at its commit
  check     run every counter once, read its counts back, and compare them with the corpus
  noise     sample the machine's background load and the run-to-run spread, before committing to a run
  run       measure, and write the record, the csv files, the notes and the results page
  report    rewrite the results page from the records that are there
  help      this
  version   the version

where things are, strongest first: a flag, then the environment, then linebench.conf
  --corpus-path <dir>         the tree that gets counted
  --extensions rs,c           what to count in it, for a tree with no corpus definition
  --corpus <name>             a corpus definition, --corpus-path being its checkout   LINEBENCH_CORPUS
  --counters-dir <dir>        where setup keeps the binaries   LINEBENCH_COUNTERS, default counters/ in linebench's own directory
  --out <dir>                 where results go                 LINEBENCH_OUT, default results/
  --add <path>                a counter or corpus definition of your own, or a directory of them, beside the built-in ones

what a run measures
  --counters a,b,c            the instances, in the order they are timed; default every definition and every given
  --given <c>@<tag>=<path>    a build of counter <c> measured as the instance <c>@<tag>, copied before it is timed
  --definition <c>@<tag>=<f>  the definition that instance runs under, when its flags differ from the release's
  --args <c>@<tag>=<text>     arguments of that instance's own, split on spaces, right after the target; with no --given they ride on the release
  --control <instance>        the instance timed alone at both ends of the run; default control = in the conf, else the first

setup
  --allow-elevated            fetch as administrator or root all the same, where there is no ordinary user, as on a CI runner

run
  --warmup <n> --runs <n> --settle <s>   hyperfine's warmups, timed runs and pause; default 3, 15, 3
  --no-prep                   leave the power scheme or governor as it is
  --yes                       do not ask before measuring an unprepared machine
  --allow-unequal-exclusions  measure even when MS Defender excludes some counters and not others
  --keep-raw                  keep every counter's printed output beside the record
";

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
        Command::Help => print_line(&mut out, HELP.trim_end()).map(|_| 0),
        Command::Version => print_line(&mut out, &format!("linebench {VERSION}")).map(|_| 0),
        Command::Report => {
            let config = read_config(&find_config())?;
            commands::run_report(&mut out, &resolve_out(&options, &config))
        }
        Command::Setup => {
            let resolved = resolve_everything(&mut out, &options)?;
            commands::run_setup(
                &mut out,
                &options,
                &resolved.locations,
                &resolved.definitions,
                resolved.platform,
            )
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

fn resolve_everything(out: &mut dyn Write, options: &Options) -> Result<Resolved, String> {
    let platform = detect_platform()?;
    let data_dir = find_data_dir();
    if let Some(dir) = &data_dir {
        let _ = fs::create_dir_all(dir);
    }
    let config_path = find_config();
    let config = read_config(&config_path)?;
    let added: Vec<PathBuf> = config.add.iter().chain(&options.add).cloned().collect();
    let definitions = collect_definitions(out, &added)?;
    let locations = resolve_locations(
        options,
        &config,
        &config_path,
        &definitions.corpora,
        data_dir.as_deref(),
    )?;
    Ok(Resolved {
        platform,
        locations,
        definitions: definitions.counters,
    })
}
