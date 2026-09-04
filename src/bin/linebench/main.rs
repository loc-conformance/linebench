#![deny(unsafe_code)]

mod commands;
mod config;
mod instances;
mod output;
mod page;
mod prep;
mod shipped;

use std::env;
use std::process;

use linebench::machine::detect_platform;

use crate::config::{Command, find_config, parse_args, read_config, resolve_locations};
use crate::output::{Color, Output, enable_colors, paint, print_line};
use crate::shipped::{read_shipped_corpora, read_shipped_definitions};

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
  --counters-dir <dir>        where setup keeps the binaries        LINEBENCH_COUNTERS
  --corpus <name>             which corpus definition               LINEBENCH_CORPUS
  --corpus-path <dir>         the checkout of that corpus
  --out <dir>                 where results go                      LINEBENCH_OUT, default results/
  --definitions <dir>         a checkout to read counters/ and corpora/ from, over the shipped ones

what a run measures
  --counters a,b,c            the instances, in the order they are timed; default every definition and every given
  --given <c>@<tag>=<path>    a build of counter <c> measured as the instance <c>@<tag>, copied before it is timed
  --definition <c>@<tag>=<f>  the definition that instance runs under, when its flags differ from the release's
  --control <instance>        the instance timed alone at both ends of the run; default the first

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
    let command = options.command.unwrap_or(Command::Help);
    match command {
        Command::Help => return print_line(&mut out, HELP.trim_end()).map(|_| 0),
        Command::Version => {
            return print_line(&mut out, &format!("linebench {VERSION}")).map(|_| 0);
        }
        _ => {}
    }
    let platform = detect_platform()?;
    let config_path = find_config();
    let config = read_config(&config_path)?;
    let definitions_dir = options
        .definitions
        .clone()
        .or_else(|| config.definitions.clone());
    let definitions = read_shipped_definitions(definitions_dir.as_deref())?;
    let corpora = read_shipped_corpora(definitions_dir.as_deref())?;
    let locations = resolve_locations(&options, &config, &config_path, &corpora)?;
    match command {
        Command::Setup => {
            commands::run_setup(&mut out, &options, &locations, &definitions, platform)
        }
        Command::Check => {
            commands::run_check(&mut out, &options, &locations, &definitions, platform)
        }
        Command::Noise => {
            commands::run_noise(&mut out, &options, &locations, &definitions, platform)
        }
        Command::Run => {
            commands::run_benchmark(&mut out, &options, &locations, &definitions, platform)
        }
        Command::Report => commands::run_report(&mut out, &locations),
        Command::Help | Command::Version => Ok(0),
    }
}
