#![deny(unsafe_code)]

#[cfg(feature = "maintenance")]
mod bump;

mod commands;
mod config;
mod help;
mod instances;
mod output;
mod page;
mod prep;
mod shipped;

use std::env;
use std::fs;
use std::io::Write;
#[cfg(feature = "maintenance")]
use std::path::Path;

use std::path::PathBuf;
use std::process;

use linebench::corpus::Corpus;
use linebench::counters::Definition;
use linebench::fetch::read_manifest;
use linebench::latest::apply_latest_pins;
use linebench::machine::{Platform, VERSION, detect_platform};

#[cfg(feature = "maintenance")]
use crate::config::COUNTERS_DIR_NAME;

use crate::config::{
    Command, Config, Locations, Options, check_skip_names, find_config, find_data_dir, parse_args,
    read_config, resolve_fetch, resolve_locations, resolve_out,
};
use crate::help::{find_help_of, get_help, paint_help};
use crate::output::{Color, Output, enable_colors, paint, print_line};
use crate::shipped::collect_definitions;

const NAME_THE_RUN: &str = "name the run to read";

fn main() {
    enable_colors();
    let code = match dispatch() {
        Ok(code) => code,
        Err(message) => {
            match message.split_once("\n\n") {
                Some((said, under)) => {
                    eprintln!("{}\n\n{}", paint(Color::Red, said), paint_help(under));
                }
                None => eprintln!("{}", paint(Color::Red, &message)),
            }
            2
        }
    };
    process::exit(code);
}

fn dispatch() -> Result<i32, String> {
    let args: Vec<String> = env::args().collect();
    let options = parse_args(&args)?;
    let mut out = Output::new();
    if let Some(help) = &options.help {
        return print_line(&mut out, &paint_help(help)).map(|_| 0);
    }
    let Some(command) = options.command else {
        return print_line(&mut out, &paint_help(&get_help())).map(|_| 0);
    };
    match command {
        Command::Version => print_line(&mut out, &format!("linebench {VERSION}")).map(|_| 0),
        #[cfg(feature = "maintenance")]
        Command::BumpVersions => bump::run_bump_versions(
            &mut out,
            Path::new(COUNTERS_DIR_NAME),
            options.target.as_deref(),
            options.as_json,
        ),
        Command::Report => {
            let config = read_config(&find_config())?;
            commands::run_report(&mut out, &resolve_out(&options, &config), options.verify)
        }
        Command::Status => {
            let ground = read_ground(&mut out, &options)?;
            commands::run_status(&mut out, &options, &ground)
        }
        Command::Verify => {
            let Some(named) = options.target.as_deref() else {
                return Err(format!(
                    "{NAME_THE_RUN}\n\n{}",
                    find_help_of(command.as_str())
                ));
            };
            commands::run_verify(&mut out, &PathBuf::from(named))
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
        Command::Check | Command::Noise | Command::Insights | Command::Run => {
            let resolved = resolve_everything(&mut out, &options)?;
            let (options, locations) = (&options, &resolved.locations);
            let definitions = &resolved.definitions;
            let platform = resolved.platform;
            let code = match command {
                Command::Check => {
                    commands::run_check(&mut out, options, locations, definitions, platform)
                }
                Command::Noise => {
                    commands::run_noise(&mut out, options, locations, definitions, platform)
                }
                Command::Insights => {
                    commands::run_insights(&mut out, options, locations, definitions, platform)
                }
                _ => commands::run_benchmark(&mut out, options, locations, definitions, platform),
            }?;
            commands::warn_about_staged_builds(&mut out, &resolved.counters_dir)?;
            Ok(code)
        }
    }
}

struct Resolved {
    platform: Platform,
    counters_dir: PathBuf,
    locations: Vec<Locations>,
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
    let wanted = resolve_locations(
        options,
        &config,
        &config_path,
        &corpora,
        data_dir.as_deref(),
    )?;
    for line in &wanted.left_behind {
        print_line(out, line)?;
    }
    let manifest = read_manifest(&wanted.counters_dir)?;
    for line in apply_latest_pins(&mut definitions, &manifest) {
        print_line(out, &line)?;
    }
    Ok(Resolved {
        platform,
        counters_dir: wanted.counters_dir,
        locations: wanted.locations,
        definitions,
    })
}
