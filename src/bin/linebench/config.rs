use std::collections::BTreeMap;
use std::env;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use linebench::corpus::Corpus;
use linebench::fetch::INSTANCE_SEPARATOR;
use linebench::files::read_toml;

pub const CONFIG_FILE: &str = "linebench.conf";
pub const COUNTERS_ENV: &str = "LINEBENCH_COUNTERS";
pub const CORPUS_ENV: &str = "LINEBENCH_CORPUS";
pub const OUT_ENV: &str = "LINEBENCH_OUT";
pub const DEFAULT_OUT: &str = "results";

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub counters: Option<PathBuf>,
    #[serde(default)]
    pub corpora: BTreeMap<String, PathBuf>,
    #[serde(default)]
    pub given: BTreeMap<String, GivenEntry>,
    pub control: Option<String>,
    pub out: Option<PathBuf>,
    pub definitions: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GivenEntry {
    pub binary: PathBuf,
    pub definition: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Run,
    Setup,
    Check,
    Noise,
    Report,
    Help,
    Version,
}

#[derive(Debug, Default)]
pub struct Options {
    pub command: Option<Command>,
    pub counters: Option<Vec<String>>,
    pub corpus: Option<String>,
    pub corpus_path: Option<PathBuf>,
    pub counters_dir: Option<PathBuf>,
    pub definitions: Option<PathBuf>,
    pub given: BTreeMap<String, PathBuf>,
    pub definition_of: BTreeMap<String, PathBuf>,
    pub control: Option<String>,
    pub warmup: Option<u32>,
    pub runs: Option<u32>,
    pub settle: Option<u32>,
    pub out: Option<PathBuf>,
    pub no_prep: bool,
    pub yes: bool,
    pub allow_unequal: bool,
    pub allow_elevated: bool,
    pub keep_raw: bool,
}

#[derive(Debug)]
pub struct Locations {
    pub counters_dir: PathBuf,
    pub corpus: Corpus,
    pub checkout: PathBuf,
    pub out: PathBuf,
    pub given: BTreeMap<String, GivenEntry>,
    pub control: Option<String>,
}

pub fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut options = Options::default();
    let mut rest = args.iter().skip(1);
    while let Some(arg) = rest.next() {
        let (flag, attached) = match arg.split_once('=') {
            Some((flag, value)) if flag.starts_with("--") => (flag, Some(value.to_string())),
            _ => (arg.as_str(), None),
        };
        let mut value = || -> Result<String, String> {
            match &attached {
                Some(text) if !text.is_empty() => Ok(text.clone()),
                Some(_) => Err(format!("{flag} needs a value")),
                None => rest
                    .next()
                    .cloned()
                    .ok_or_else(|| format!("{flag} needs a value")),
            }
        };
        match flag {
            "run" | "setup" | "check" | "noise" | "report" | "help" | "version"
                if options.command.is_none() =>
            {
                options.command = Some(match flag {
                    "run" => Command::Run,
                    "setup" => Command::Setup,
                    "check" => Command::Check,
                    "noise" => Command::Noise,
                    "report" => Command::Report,
                    "help" => Command::Help,
                    _ => Command::Version,
                });
            }
            "--help" | "-h" => options.command = Some(Command::Help),
            "--version" | "-V" => options.command = Some(Command::Version),
            "--counters" => {
                options.counters = Some(
                    value()?
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect(),
                );
            }
            "--corpus" => options.corpus = Some(value()?),
            "--corpus-path" => options.corpus_path = Some(PathBuf::from(value()?)),
            "--counters-dir" => options.counters_dir = Some(PathBuf::from(value()?)),
            "--definitions" => options.definitions = Some(PathBuf::from(value()?)),
            "--given" => {
                let (instance, path) = split_assignment(flag, &value()?)?;
                options.given.insert(instance, PathBuf::from(path));
            }
            "--definition" => {
                let (instance, path) = split_assignment(flag, &value()?)?;
                options.definition_of.insert(instance, PathBuf::from(path));
            }
            "--control" => options.control = Some(value()?),
            "--warmup" => options.warmup = Some(parse_number(flag, &value()?)?),
            "--runs" => options.runs = Some(parse_number(flag, &value()?)?),
            "--settle" => options.settle = Some(parse_number(flag, &value()?)?),
            "--out" => options.out = Some(PathBuf::from(value()?)),
            "--no-prep" => options.no_prep = true,
            "--yes" | "-y" => options.yes = true,
            "--allow-unequal-exclusions" => options.allow_unequal = true,
            "--allow-elevated" => options.allow_elevated = true,
            "--keep-raw" => options.keep_raw = true,
            other => {
                return Err(format!(
                    "{other} is not something linebench understands; linebench help lists what is"
                ));
            }
        }
    }
    Ok(options)
}

pub fn find_config() -> PathBuf {
    let here = PathBuf::from(CONFIG_FILE);
    if here.is_file() {
        return here;
    }
    env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(CONFIG_FILE)))
        .filter(|beside| beside.is_file())
        .unwrap_or(here)
}

pub fn read_config(path: &Path) -> Result<Config, String> {
    if !path.is_file() {
        return Ok(Config::default());
    }
    read_toml(path)
}

pub fn resolve_out(options: &Options, config: &Config) -> PathBuf {
    options
        .out
        .clone()
        .or_else(|| read_env(OUT_ENV).map(PathBuf::from))
        .or_else(|| config.out.clone())
        .unwrap_or_else(|| PathBuf::from(DEFAULT_OUT))
}

pub fn resolve_locations(
    options: &Options,
    config: &Config,
    config_path: &Path,
    corpora: &[Corpus],
) -> Result<Locations, String> {
    let counters_dir = options
        .counters_dir
        .clone()
        .or_else(|| read_env(COUNTERS_ENV).map(PathBuf::from))
        .or_else(|| config.counters.clone());
    let corpus_name = options
        .corpus
        .clone()
        .or_else(|| read_env(CORPUS_ENV))
        .or_else(
            || match config.corpora.keys().collect::<Vec<_>>().as_slice() {
                [only] => Some((*only).clone()),
                _ => None,
            },
        );
    let (Some(counters_dir), Some(corpus_name)) = (counters_dir, corpus_name) else {
        return Err(explain_the_missing_locations(config_path, corpora));
    };
    let corpus = corpora
        .iter()
        .find(|corpus| corpus.name == corpus_name)
        .cloned()
        .ok_or_else(|| {
            format!(
                "no corpus definition named {corpus_name}; known: {}",
                corpora
                    .iter()
                    .map(|c| c.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;
    let checkout = options
        .corpus_path
        .clone()
        .or_else(|| config.corpora.get(&corpus_name).cloned())
        .ok_or_else(|| {
            format!(
                "nothing says where the {corpus_name} checkout is: --corpus-path <dir>, or \
                 [corpora] {corpus_name} = \"<dir>\" in {}",
                config_path.display()
            )
        })?;
    let out = resolve_out(options, config);
    let mut given = config.given.clone();
    for (instance, binary) in &options.given {
        let definition = options.definition_of.get(instance).cloned().or_else(|| {
            given
                .get(instance)
                .and_then(|entry| entry.definition.clone())
        });
        given.insert(
            instance.clone(),
            GivenEntry {
                binary: binary.clone(),
                definition,
            },
        );
    }
    for (instance, definition) in &options.definition_of {
        match given.get_mut(instance) {
            Some(entry) => entry.definition = Some(definition.clone()),
            None => {
                return Err(format!(
                    "--definition {instance}=... names an instance that no --given or [given] entry \
                     gives a binary for"
                ));
            }
        }
    }
    for instance in given.keys() {
        if !instance.contains(INSTANCE_SEPARATOR) {
            return Err(format!(
                "a given instance is named <counter>@<tag>, and {instance} has no @"
            ));
        }
    }
    Ok(Locations {
        counters_dir,
        corpus,
        checkout,
        out,
        given,
        control: config.control.clone(),
    })
}

pub fn read_env(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.trim().is_empty())
}

fn explain_the_missing_locations(config_path: &Path, corpora: &[Corpus]) -> String {
    let sample_corpus = corpora.first().map_or("linux", |c| c.name.as_str());
    format!(
        "the counters directory and the corpus are not set, and there is no default. Set them in \
         one of three ways, strongest first:\n\n\
         \x20 --counters-dir <dir> --corpus <name> --corpus-path <dir>   for this invocation only\n\
         \x20 {COUNTERS_ENV} / {CORPUS_ENV}                               environment\n\
         \x20 {}\n\
         \x20     copied from {CONFIG_FILE}.example and edited, e.g.\n\n\
         \x20     counters = \"D:/counters\"\n\n\
         \x20     [corpora]\n\
         \x20     {sample_corpus} = \"D:/corpora/{sample_corpus}\"\n\n\
         {CONFIG_FILE} is gitignored and machine-local.",
        config_path.display()
    )
}

fn split_assignment(flag: &str, text: &str) -> Result<(String, String), String> {
    match text.split_once('=') {
        Some((name, path)) if !name.is_empty() && !path.is_empty() => {
            Ok((name.to_string(), path.to_string()))
        }
        _ => Err(format!(
            "{flag} takes <counter>@<tag>=<path>, and {text} is not that"
        )),
    }
}

fn parse_number(flag: &str, text: &str) -> Result<u32, String> {
    text.parse()
        .map_err(|_| format!("{flag} takes a whole number, and {text} is not one"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_are_read_with_and_without_the_equals_sign_and_the_command_anywhere() {
        let options = parse(
            "--counters mezura,mezura@dev --given mezura@dev=D:/b/mezura.exe run --runs=5 \
             --definition mezura@dev=D:/m/.linebench/mezura.toml --yes",
        )
        .unwrap();
        assert_eq!(options.command, Some(Command::Run));
        assert_eq!(
            options.counters.as_deref(),
            Some(&["mezura".to_string(), "mezura@dev".to_string()][..])
        );
        assert_eq!(
            options.given.get("mezura@dev"),
            Some(&PathBuf::from("D:/b/mezura.exe"))
        );
        assert_eq!(
            options.definition_of.get("mezura@dev"),
            Some(&PathBuf::from("D:/m/.linebench/mezura.toml"))
        );
        assert_eq!(options.runs, Some(5));
        assert!(options.yes && !options.allow_elevated);
        assert!(parse("setup --allow-elevated").unwrap().allow_elevated);
        assert!(
            parse("run --runs five")
                .unwrap_err()
                .contains("whole number")
        );
        assert!(
            parse("run --given mezura")
                .unwrap_err()
                .contains("<counter>@<tag>=<path>")
        );
        assert!(parse("run --nonsense").unwrap_err().contains("--nonsense"));
        assert!(
            parse("run --out=")
                .unwrap_err()
                .contains("--out needs a value")
        );
    }

    #[test]
    fn a_given_from_the_command_line_wins_over_the_config_and_keeps_its_definition() {
        let config: Config = toml::from_str(
            "counters = \"D:/c\"\n[corpora]\nlinux = \"D:/linux\"\n\
             [given.\"mezura@dev\"]\nbinary = \"D:/old.exe\"\ndefinition = \"D:/dev.toml\"\n",
        )
        .unwrap();
        let corpora = vec![
            linebench::corpus::parse_corpus(
                "name = \"linux\"\nextensions = [\"c\"]\n",
                Path::new("linux.toml"),
            )
            .unwrap(),
        ];
        let options = parse("run --given mezura@dev=D:/new.exe").unwrap();
        let locations =
            resolve_locations(&options, &config, Path::new("linebench.conf"), &corpora).unwrap();
        assert_eq!(locations.counters_dir, PathBuf::from("D:/c"));
        assert_eq!(locations.checkout, PathBuf::from("D:/linux"));
        assert_eq!(locations.corpus.name, "linux");
        let entry = &locations.given["mezura@dev"];
        assert_eq!(entry.binary, PathBuf::from("D:/new.exe"));
        assert_eq!(entry.definition, Some(PathBuf::from("D:/dev.toml")));

        let empty = Config::default();
        let refusal =
            resolve_locations(&options, &empty, Path::new("linebench.conf"), &corpora).unwrap_err();
        assert!(refusal.contains("--counters-dir <dir>"), "{refusal}");
        let orphan = parse("run --definition scc@x=D:/x.toml").unwrap();
        let refusal =
            resolve_locations(&orphan, &config, Path::new("linebench.conf"), &corpora).unwrap_err();
        assert!(refusal.contains("no --given or [given] entry"), "{refusal}");
    }

    fn parse(line: &str) -> Result<Options, String> {
        let args: Vec<String> = std::iter::once("linebench")
            .chain(line.split_whitespace())
            .map(String::from)
            .collect();
        parse_args(&args)
    }
}
