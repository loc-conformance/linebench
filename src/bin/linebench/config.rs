use std::collections::BTreeMap;
use std::env;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use linebench::corpus::{Corpus, build_corpus_of};
use linebench::counters::Definition;
use linebench::fetch::INSTANCE_SEPARATOR;
use linebench::files::read_toml;
use linebench::os::capture_with_status;

use crate::help::{find_help_of, get_help, name_every_command};

pub const TOOL: &str = "linebench";
pub const CONFIG_FILE: &str = "linebench.conf";
pub const DATA_DIR_NAME: &str = "linebench";
pub const COUNTERS_DIR_NAME: &str = "counters";
pub const CORPORA_DIR_NAME: &str = "corpora";
pub const EVERYTHING: &str = "all";
pub const COUNTERS_ENV: &str = "LINEBENCH_COUNTERS";
pub const CORPUS_ENV: &str = "LINEBENCH_CORPUS";
pub const OUT_ENV: &str = "LINEBENCH_OUT";
pub const DEFAULT_OUT: &str = "results";
pub const COMMANDS: [&str; 7] = [
    "run", "fetch", "check", "noise", "insights", "report", "version",
];

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
    #[serde(default)]
    pub add: Vec<PathBuf>,
    #[serde(default)]
    pub skip: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GivenEntry {
    pub binary: Option<PathBuf>,
    pub definition: Option<PathBuf>,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Run,
    Fetch,
    Check,
    Noise,
    Insights,
    Report,
    Version,
}

impl Command {
    pub fn as_str(self) -> &'static str {
        match self {
            Command::Run => "run",
            Command::Fetch => "fetch",
            Command::Check => "check",
            Command::Noise => "noise",
            Command::Insights => "insights",
            Command::Report => "report",
            Command::Version => "version",
        }
    }
}

#[derive(Debug, Default)]
pub struct Options {
    pub command: Option<Command>,
    pub help: Option<String>,
    pub target: Option<String>,
    pub counters: Option<Vec<String>>,
    pub corpus: Option<Vec<String>>,
    pub corpus_path: Option<PathBuf>,
    pub counters_dir: Option<PathBuf>,
    pub extensions: Option<Vec<String>>,
    pub add: Vec<PathBuf>,
    pub given: BTreeMap<String, PathBuf>,
    pub definition_of: BTreeMap<String, PathBuf>,
    pub args_of: BTreeMap<String, Vec<String>>,
    pub expect_identical: Vec<(String, String)>,
    pub against: Option<String>,
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
    pub newest: bool,
}

#[derive(Debug)]
pub struct FetchPlan {
    pub counters_dir: PathBuf,
    pub skip: Vec<String>,
    pub corpora: Vec<(Corpus, PathBuf)>,
}

#[derive(Debug)]
pub struct Locations {
    pub counters_dir: PathBuf,
    pub corpus: Corpus,
    pub checkout: PathBuf,
    pub out: PathBuf,
    pub given: BTreeMap<String, GivenEntry>,
    pub control: Option<String>,
    pub skip: Vec<String>,
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
            "run" | "fetch" | "check" | "noise" | "insights" | "report" | "version"
                if options.command.is_none() =>
            {
                options.command = Some(match flag {
                    "run" => Command::Run,
                    "fetch" => Command::Fetch,
                    "check" => Command::Check,
                    "noise" => Command::Noise,
                    "insights" => Command::Insights,
                    "report" => Command::Report,
                    _ => Command::Version,
                });
            }
            "--help" | "-h" => {
                options.help = Some(match options.command {
                    Some(command) => find_help_of(command.as_str()),
                    None => get_help(),
                });
            }
            "--version" | "-V" => options.command = Some(Command::Version),
            "--counters" => {
                let first = value()?;
                let named = read_list(flag, first, &mut rest)?;
                options.counters = match named.iter().any(|name| name == EVERYTHING) {
                    true => None,
                    false => Some(named),
                };
            }
            "--extensions" => {
                let first = value()?;
                options.extensions = Some(read_list(flag, first, &mut rest)?);
            }
            "--corpus" => {
                let first = value()?;
                options.corpus = Some(read_list(flag, first, &mut rest)?);
            }
            "--corpus-path" => options.corpus_path = Some(PathBuf::from(value()?)),
            "--counters-dir" => options.counters_dir = Some(PathBuf::from(value()?)),
            "--add" => options.add.push(PathBuf::from(value()?)),
            "--given" => {
                let (instance, path) = split_assignment(flag, &value()?)?;
                options.given.insert(instance, PathBuf::from(path));
            }
            "--definition" => {
                let (instance, path) = split_assignment(flag, &value()?)?;
                options.definition_of.insert(instance, PathBuf::from(path));
            }
            "--args" => {
                let (instance, text) = split_assignment(flag, &value()?)?;
                let args = text.split_whitespace().map(String::from).collect();
                options.args_of.insert(instance, args);
            }
            "--expect-identical" => {
                let first = value()?;
                for pair in read_list(flag, first, &mut rest)? {
                    let (left, right) = pair
                        .split_once('=')
                        .filter(|(left, right)| !left.is_empty() && !right.is_empty())
                        .ok_or_else(|| {
                            format!(
                                "{flag} takes <instance>=<instance> pairs, and {pair} is not one"
                            )
                        })?;
                    options
                        .expect_identical
                        .push((left.to_string(), right.to_string()));
                }
            }
            "--against" => options.against = Some(value()?),
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
            "--newest" => options.newest = true,
            other
                if !other.starts_with('-')
                    && !COMMANDS.contains(&other)
                    && options.command.is_some()
                    && options.target.is_none() =>
            {
                options.target = Some(other.to_string());
            }
            other => return Err(explain_the_unknown_argument(other, options.command)),
        }
    }
    if options.help.is_some() {
        return Ok(options);
    }
    read_corpus_as_the_target(&mut options)?;
    Ok(options)
}

fn read_corpus_as_the_target(options: &mut Options) -> Result<(), String> {
    if options.command == Some(Command::Fetch) {
        return Ok(());
    }
    let Some(named) = options.corpus.take() else {
        return Ok(());
    };
    match named.as_slice() {
        [name] => options.target.get_or_insert(name.clone()),
        many => {
            return Err(format!(
                "name the one corpus to measure, and --corpus names {}",
                many.join(", ")
            ));
        }
    };
    Ok(())
}

pub fn find_config() -> PathBuf {
    let here = PathBuf::from(CONFIG_FILE);
    if here.is_file() {
        return here;
    }
    find_data_dir()
        .map(|dir| dir.join(CONFIG_FILE))
        .unwrap_or(here)
}

pub fn find_data_dir() -> Option<PathBuf> {
    let named = |name: &str| {
        env::var_os(name)
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
    };
    let under_sudo = !cfg!(windows) && env::var("SUDO_USER").is_ok_and(|user| !user.is_empty());
    let base = if cfg!(windows) {
        named("APPDATA")
    } else if cfg!(target_os = "macos") {
        find_home().map(|home| home.join("Library").join("Application Support"))
    } else {
        named("XDG_DATA_HOME")
            .filter(|_| !under_sudo)
            .or_else(|| find_home().map(|home| home.join(".local").join("share")))
    };
    base.map(|dir| dir.join(DATA_DIR_NAME))
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

pub fn resolve_fetch(
    options: &Options,
    config: &Config,
    config_path: &Path,
    corpora: &[Corpus],
    counters: &[Definition],
    data_dir: Option<&Path>,
) -> Result<FetchPlan, String> {
    let counters_dir = resolve_counters_dir(options, config, config_path, data_dir)?;
    let wanted: Vec<Corpus> = match &options.corpus {
        None if options.counters.is_none() => {
            return Err(explain_what_fetch_takes(
                options,
                config,
                counters,
                corpora,
                &counters_dir,
                data_dir,
            ));
        }
        None => Vec::new(),
        Some(named) if named.iter().any(|name| name == EVERYTHING) => corpora.to_vec(),
        Some(named) => named
            .iter()
            .map(|name| find_corpus(name, corpora).cloned())
            .collect::<Result<Vec<Corpus>, String>>()?,
    };
    let under = wanted.len() > 1;
    let taken = wanted
        .into_iter()
        .map(|corpus| {
            let checkout = find_checkout(&corpus.name, options, config)
                .map(|path| match under {
                    true => path.join(&corpus.name),
                    false => path,
                })
                .or_else(|| place_for_corpus(&corpus.name, data_dir))
                .ok_or_else(|| explain_the_homeless_corpus(&corpus.name))?;
            Ok((corpus, checkout))
        })
        .collect::<Result<Vec<(Corpus, PathBuf)>, String>>()?;
    Ok(FetchPlan {
        counters_dir,
        skip: config.skip.clone(),
        corpora: taken,
    })
}

pub fn resolve_counters_dir(
    options: &Options,
    config: &Config,
    config_path: &Path,
    data_dir: Option<&Path>,
) -> Result<PathBuf, String> {
    options
        .counters_dir
        .clone()
        .or_else(|| read_env(COUNTERS_ENV).map(PathBuf::from))
        .or_else(|| config.counters.clone())
        .or_else(|| data_dir.map(|dir| dir.join(COUNTERS_DIR_NAME)))
        .ok_or_else(|| explain_the_missing_counters_dir(config_path))
}

fn find_corpus<'a>(name: &str, corpora: &'a [Corpus]) -> Result<&'a Corpus, String> {
    corpora
        .iter()
        .find(|corpus| corpus.name == name)
        .ok_or_else(|| {
            format!(
                "no corpus definition named {name}, known: {}",
                name_the_corpora(corpora)
            )
        })
}

fn find_checkout(name: &str, options: &Options, config: &Config) -> Option<PathBuf> {
    options
        .corpus_path
        .clone()
        .or_else(|| config.corpora.get(name).cloned())
}

fn place_for_corpus(name: &str, data_dir: Option<&Path>) -> Option<PathBuf> {
    data_dir.map(|dir| dir.join(CORPORA_DIR_NAME).join(name))
}

fn name_the_corpora(corpora: &[Corpus]) -> String {
    corpora
        .iter()
        .map(|corpus| corpus.name.as_str())
        .collect::<Vec<&str>>()
        .join(", ")
}

fn explain_the_missing_checkout(name: &str, config_path: &Path, data_dir: Option<&Path>) -> String {
    let downloaded = place_for_corpus(name, data_dir)
        .map(|path| format!("   puts it in {}", show_path(&path)))
        .unwrap_or_else(|| "   downloads it".to_string());
    let mut lines = vec![
        format!("say where the {name} checkout is"),
        String::new(),
        format!("   linebench fetch --corpus {name}"),
        downloaded,
        String::new(),
        "or name one you already have, with --corpus-path <dir>".to_string(),
    ];
    if config_path.is_file() {
        lines.push(format!(
            "   or with [corpora] {name} = \"<dir>\" in {}",
            show_path(config_path)
        ));
    }
    lines.join("\n")
}

fn explain_the_homeless_corpus(name: &str) -> String {
    let unset = match cfg!(windows) {
        true => "%APPDATA%",
        false => "$XDG_DATA_HOME and $HOME",
    };
    format!("{unset} is not set, so say where {name} goes with --corpus-path <dir>")
}

fn show_path(path: &Path) -> String {
    let shown = path.display().to_string();
    match cfg!(windows) {
        true => shown.replace('/', "\\"),
        false => shown,
    }
}

fn describe_where_counters_come_from(options: &Options, config: &Config) -> String {
    if options.counters_dir.is_some() {
        return ", which --counters-dir names".to_string();
    }
    if read_env(COUNTERS_ENV).is_some() {
        return format!(", which {COUNTERS_ENV} names");
    }
    match config.counters.is_some() {
        true => format!(", which {CONFIG_FILE} names"),
        false => String::new(),
    }
}

fn explain_what_fetch_takes(
    options: &Options,
    config: &Config,
    counters: &[Definition],
    corpora: &[Corpus],
    counters_dir: &Path,
    data_dir: Option<&Path>,
) -> String {
    let names = |listed: Vec<&str>| listed.join(", ");
    let where_corpora = data_dir
        .map(|dir| show_path(&dir.join(CORPORA_DIR_NAME).join("<name>")))
        .unwrap_or_else(|| "the directory --corpus-path names".to_string());
    format!(
        "Specify which counters and which corpus to download, or \"{EVERYTHING}\" to get them \
         all\n\n\
         \x20 linebench fetch --counters {EVERYTHING} --corpus {EVERYTHING}\n\n\
         \x20 counters   {}\n\
         \x20 corpora    {}\n\n\
         The counters go to {}{}\n\
         A corpus is cloned to {where_corpora},\n\
         unless --corpus-path names somewhere else.",
        names(counters.iter().map(|c| c.name.as_str()).collect()),
        names(corpora.iter().map(|c| c.name.as_str()).collect()),
        show_path(counters_dir),
        describe_where_counters_come_from(options, config)
    )
}

pub fn resolve_locations(
    options: &Options,
    config: &Config,
    config_path: &Path,
    corpora: &[Corpus],
    data_dir: Option<&Path>,
) -> Result<Locations, String> {
    let counters_dir = resolve_counters_dir(options, config, config_path, data_dir)?;
    let (corpus, checkout) = match (read_target(options, corpora), &options.extensions) {
        (Target::Named(corpus), Some(_)) => {
            return Err(format!(
                "--extensions is for a tree with no corpus definition, and {} declares its own",
                corpus.name
            ));
        }
        (Target::Nothing, _) => {
            return Err(explain_the_missing_target(
                options.command,
                corpora,
                config,
                data_dir,
            ));
        }
        (Target::Tree(checkout), Some(extensions)) => {
            let corpus = build_corpus_of(&checkout, extensions)?;
            if corpora.iter().any(|known| known.name == corpus.name) {
                return Err(format!(
                    "the directory is named {0}, and so is a corpus definition, so its runs would \
                     mix with that corpus's: name {0} on its own, or rename the directory",
                    corpus.name
                ));
            }
            (corpus, checkout)
        }
        (Target::Tree(path), None) => {
            return Err(format!(
                "say what to count in {}, with --extensions rs,c",
                path.display()
            ));
        }
        (Target::Named(corpus), None) => {
            let checkout = find_checkout(&corpus.name, options, config)
                .or_else(|| place_for_corpus(&corpus.name, data_dir).filter(|path| path.is_dir()))
                .ok_or_else(|| explain_the_missing_checkout(&corpus.name, config_path, data_dir))?;
            (corpus, checkout)
        }
    };
    let out = resolve_out(options, config);
    let mut given = config.given.clone();
    let empty = || GivenEntry {
        binary: None,
        definition: None,
        args: Vec::new(),
    };
    for (instance, binary) in &options.given {
        given.entry(instance.clone()).or_insert_with(empty).binary = Some(binary.clone());
    }
    for (instance, args) in &options.args_of {
        given.entry(instance.clone()).or_insert_with(empty).args = args.clone();
    }
    for (instance, definition) in &options.definition_of {
        match given.get_mut(instance) {
            Some(entry) => entry.definition = Some(definition.clone()),
            None => {
                return Err(format!(
                    "--definition {instance}=... names an instance that no --given, --args or \
                     [given] entry makes"
                ));
            }
        }
    }
    for (instance, entry) in &given {
        if entry.binary.is_none() && entry.args.is_empty() {
            return Err(format!(
                "[given.\"{instance}\"] names neither a binary nor args, so there is nothing of \
                 its own to measure"
            ));
        }
        if !entry.args.is_empty() && !instance.contains(INSTANCE_SEPARATOR) {
            return Err(format!(
                "args make an instance of their own: name it {instance}@<tag>"
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
        skip: config.skip.clone(),
    })
}

pub fn check_skip_names(
    skip: &[String],
    counters: &[Definition],
    config_path: &Path,
) -> Result<(), String> {
    match skip
        .iter()
        .find(|name| !counters.iter().any(|d| &d.name == *name))
    {
        Some(name) => Err(format!(
            "{}: skip names {name}, and no counter definition has that name",
            config_path.display()
        )),
        None => Ok(()),
    }
}

pub fn read_env(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.trim().is_empty())
}

enum Target {
    Named(Corpus),
    Tree(PathBuf),
    Nothing,
}

fn read_target(options: &Options, corpora: &[Corpus]) -> Target {
    let Some(target) = options.target.clone().or_else(|| read_env(CORPUS_ENV)) else {
        return Target::Nothing;
    };
    match corpora.iter().find(|corpus| corpus.name == target) {
        Some(corpus) => Target::Named(corpus.clone()),
        None => Target::Tree(PathBuf::from(target)),
    }
}

fn explain_the_missing_target(
    command: Option<Command>,
    corpora: &[Corpus],
    config: &Config,
    data_dir: Option<&Path>,
) -> String {
    let verb = command.map_or("<command>", Command::as_str);
    let fetched = |name: &str| {
        config
            .corpora
            .get(name)
            .cloned()
            .or_else(|| {
                data_dir
                    .map(|dir| dir.join(CORPORA_DIR_NAME).join(name))
                    .filter(|path| path.is_dir())
            })
            .map(|path| format!("{name} is at {}", path.display()))
            .unwrap_or_else(|| format!("{name} is not on this machine yet"))
    };
    let lines: Vec<String> = corpora
        .iter()
        .map(|corpus| format!("\x20 {}", fetched(&corpus.name)))
        .collect();
    format!(
        "Say what to count, either a corpus definition or a tree of your own.\n\n\
         \x20 linebench {verb} {}\n\
         \x20 linebench {verb} <dir> --extensions rs,c\n\n{}",
        corpora
            .iter()
            .map(|corpus| corpus.name.as_str())
            .find(|name| *name != TOOL)
            .unwrap_or("<corpus>"),
        lines.join("\n")
    )
}

fn explain_the_missing_counters_dir(config_path: &Path) -> String {
    format!(
        "no home directory is known, so there is no default place for the counter binaries: \
         give one with --counters-dir <dir>, {COUNTERS_ENV}=<dir>, or counters = \"<dir>\" in {}",
        config_path.display()
    )
}

fn read_list<'a>(
    flag: &str,
    mut text: String,
    rest: &mut (impl Iterator<Item = &'a String> + Clone),
) -> Result<Vec<String>, String> {
    while let Some(next) = rest.clone().next() {
        if next.starts_with('-') || !(text.ends_with(',') || next.starts_with(',')) {
            break;
        }
        text.push_str(next);
        rest.next();
    }
    split_list(flag, &text)
}

fn split_list(flag: &str, text: &str) -> Result<Vec<String>, String> {
    let names: Vec<String> = text.split(',').map(|s| s.trim().to_string()).collect();
    if names.iter().any(|name| name.is_empty()) {
        return Err(format!("empty name in {flag} {text}"));
    }
    Ok(names)
}

fn explain_the_unknown_argument(other: &str, command: Option<Command>) -> String {
    let Some(command) = command else {
        return format!(
            "{other} is not a command of this program\n\n{}\n",
            name_every_command()
        );
    };
    format!(
        "{other} is not a flag of this command\n\n{}",
        find_help_of(command.as_str())
    )
}

fn split_assignment(flag: &str, text: &str) -> Result<(String, String), String> {
    match text.split_once('=') {
        Some((name, path)) if !name.is_empty() && !path.is_empty() => {
            Ok((name.to_string(), path.to_string()))
        }
        _ => Err(format!(
            "{flag} takes <instance>=<path>, and {text} is not that"
        )),
    }
}

fn parse_number(flag: &str, text: &str) -> Result<u32, String> {
    text.parse()
        .map_err(|_| format!("{flag} takes a whole number, and {text} is not one"))
}

fn find_home() -> Option<PathBuf> {
    let own = env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute());
    let Some(user) = env::var("SUDO_USER").ok().filter(|user| !user.is_empty()) else {
        return own;
    };
    let from_passwd = capture_with_status("getent", &["passwd", &user])
        .ok()
        .filter(|(ok, _)| *ok)
        .and_then(|(_, text)| parse_passwd_home(&text));
    let guessed = if cfg!(target_os = "macos") {
        PathBuf::from("/Users").join(&user)
    } else {
        PathBuf::from("/home").join(&user)
    };
    from_passwd
        .or_else(|| guessed.is_dir().then_some(guessed))
        .or(own)
}

fn parse_passwd_home(text: &str) -> Option<PathBuf> {
    let home = text.lines().next()?.split(':').nth(5)?.trim();
    (!home.is_empty()).then(|| PathBuf::from(home))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_home_in_a_passwd_line_is_the_sixth_field() {
        assert_eq!(
            parse_passwd_home("petros:x:1000:1000:Petros:/home/petros:/bin/bash\n"),
            Some(PathBuf::from("/home/petros"))
        );
        assert_eq!(
            parse_passwd_home("petros:x:1000:1000::/home/petros:/bin/bash"),
            Some(PathBuf::from("/home/petros"))
        );
        assert_eq!(parse_passwd_home("petros:x:1000"), None);
        assert_eq!(parse_passwd_home(""), None);
    }

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
        let pairs = parse("run --expect-identical mezura=mezura@dev, tokei=tokei@dev").unwrap();
        assert_eq!(
            pairs.expect_identical,
            [
                ("mezura".to_string(), "mezura@dev".to_string()),
                ("tokei".to_string(), "tokei@dev".to_string())
            ]
        );
        assert!(
            parse("run --expect-identical mezura")
                .unwrap_err()
                .contains("<instance>=<instance>")
        );
        assert!(parse("fetch --allow-elevated").unwrap().allow_elevated);
        assert!(parse("fetch --newest").unwrap().newest);
        assert!(!parse("fetch").unwrap().newest);
        assert_eq!(parse("run linux").unwrap().target.as_deref(), Some("linux"));
        assert_eq!(
            parse("check --corpus linux").unwrap().target.as_deref(),
            Some("linux")
        );
        assert_eq!(
            parse("fetch --corpus linux,linebench").unwrap().corpus,
            Some(vec!["linux".to_string(), "linebench".to_string()])
        );
        assert_eq!(
            parse("check linux --corpus linux")
                .unwrap()
                .target
                .as_deref(),
            Some("linux")
        );
        assert!(
            parse("check --corpus linux,linebench")
                .unwrap_err()
                .contains("the one corpus to measure")
        );
        let stray = parse("run linux other").unwrap_err();
        assert!(
            stray.starts_with(
                "other is not a flag of this command

"
            ),
            "{stray}"
        );
        assert!(stray.contains("linebench run <corpus|dir>"), "{stray}");
        assert!(!stray.contains("linebench check <corpus|dir>"), "{stray}");
        assert_eq!(
            parse("run --against 20260908-100000")
                .unwrap()
                .against
                .as_deref(),
            Some("20260908-100000")
        );
        assert!(parse("run --against").unwrap_err().contains("--against"));
        assert!(
            parse("run --runs five")
                .unwrap_err()
                .contains("whole number")
        );
        assert!(
            parse("run --given mezura")
                .unwrap_err()
                .contains("<instance>=<path>")
        );
        let misspelled = parse("run --nonsense").unwrap_err();
        assert!(
            misspelled.starts_with(
                "--nonsense is not a flag of this command

"
            ),
            "{misspelled}"
        );
        assert!(
            misspelled.contains("linebench run <corpus|dir>"),
            "{misspelled}"
        );
        let nameless = parse("mezura@dev").unwrap_err();
        assert!(
            nameless.starts_with(
                "mezura@dev is not a command of this program

"
            ),
            "{nameless}"
        );
        assert!(nameless.contains("linebench fetch ["), "{nameless}");
        assert!(!nameless.contains("Downloads what the other"), "{nameless}");
        let twice = parse("run check").unwrap_err();
        assert!(
            twice.starts_with(
                "check is not a flag of this command

"
            ),
            "{twice}"
        );
        assert_eq!(
            parse("run --counters mezura,,scc").unwrap_err(),
            "empty name in --counters mezura,,scc"
        );
        assert_eq!(
            parse("run --counters mezura,").unwrap_err(),
            "empty name in --counters mezura,"
        );
    }

    #[test]
    fn help_after_a_command_is_that_commands_own_and_a_bare_one_is_the_whole_of_it() {
        let one = parse("fetch --help").unwrap().help.unwrap();
        assert!(one.starts_with("linebench fetch ["), "{one}");
        assert!(!one.contains("linebench check "), "{one}");
        assert!(!one.contains("A flag beats"), "{one}");
        let short = parse("check -h").unwrap().help.unwrap();
        assert!(short.starts_with("linebench check "), "{short}");
        let whole = parse("--help").unwrap().help.unwrap();
        assert!(whole.contains("linebench check "), "{whole}");
        assert!(whole.contains("A flag beats"), "{whole}");
        let asked = parse("check --corpus linux,linebench --help").unwrap();
        assert!(asked.help.is_some());
        assert_eq!(asked.target, None);
    }

    #[test]
    fn a_counter_list_the_shell_split_on_its_spaces_is_put_back_together() {
        let expected = ["mezura".to_string(), "mezura@dev".to_string()];
        for line in [
            "run --counters mezura,mezura@dev",
            "run --counters mezura, mezura@dev",
            "run --counters mezura ,mezura@dev",
            "run --counters mezura , mezura@dev",
            "run --counters=mezura, mezura@dev",
        ] {
            let options = parse(line).unwrap();
            assert_eq!(options.counters.as_deref(), Some(&expected[..]), "{line}");
            assert_eq!(options.command, Some(Command::Run), "{line}");
        }
        let stops_at_a_flag = parse("--counters mezura, --runs 5").unwrap_err();
        assert_eq!(stops_at_a_flag, "empty name in --counters mezura,");
        let stops_at_a_command = parse("--counters mezura run").unwrap();
        assert_eq!(
            stops_at_a_command.counters.as_deref(),
            Some(&["mezura".to_string()][..])
        );
        assert_eq!(stops_at_a_command.command, Some(Command::Run));
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
        let conf = Path::new("linebench.conf");
        let data = Some(Path::new("D:/data/linebench"));
        let options = parse("run linux --given mezura@dev=D:/new.exe").unwrap();
        let locations = resolve_locations(&options, &config, conf, &corpora, data).unwrap();
        assert_eq!(locations.counters_dir, PathBuf::from("D:/c"));
        assert_eq!(locations.checkout, PathBuf::from("D:/linux"));
        assert_eq!(locations.corpus.name, "linux");
        let entry = &locations.given["mezura@dev"];
        assert_eq!(entry.binary, Some(PathBuf::from("D:/new.exe")));
        assert_eq!(entry.definition, Some(PathBuf::from("D:/dev.toml")));

        let empty = Config::default();
        let refusal = resolve_locations(&options, &empty, conf, &corpora, data).unwrap_err();
        assert!(
            refusal.contains("say where the linux checkout is"),
            "{refusal}"
        );
        let aimless = parse("run").unwrap();
        let refusal = resolve_locations(&aimless, &empty, conf, &corpora, data).unwrap_err();
        assert!(refusal.starts_with("Say what to count"), "{refusal}");
        let refusal = resolve_locations(&options, &empty, conf, &corpora, None).unwrap_err();
        assert!(refusal.contains("--counters-dir <dir>"), "{refusal}");
        let orphan = parse("run linux --definition scc@x=D:/x.toml").unwrap();
        let refusal = resolve_locations(&orphan, &config, conf, &corpora, data).unwrap_err();
        assert!(
            refusal.contains("no --given, --args or [given] entry"),
            "{refusal}"
        );
    }

    #[test]
    fn the_arguments_of_an_instance_ride_on_the_release_or_on_its_own_build_and_need_a_tag() {
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
        let conf = Path::new("linebench.conf");
        let data = Some(Path::new("D:/data/linebench"));
        let typed = [
            "linebench",
            "run",
            "linux",
            "--args",
            "mezura@c16=--threads 4 16",
            "--args",
            "mezura@dev=--fast",
        ]
        .map(String::from);
        let options = parse_args(&typed).unwrap();
        let locations = resolve_locations(&options, &config, conf, &corpora, data).unwrap();
        let release_with = &locations.given["mezura@c16"];
        assert_eq!(release_with.binary, None);
        assert_eq!(release_with.args, ["--threads", "4", "16"]);
        let own_build = &locations.given["mezura@dev"];
        assert_eq!(own_build.binary, Some(PathBuf::from("D:/old.exe")));
        assert_eq!(own_build.definition, Some(PathBuf::from("D:/dev.toml")));
        assert_eq!(own_build.args, ["--fast"]);
        let plain =
            parse_args(&["linebench", "run", "linux", "--args", "mezura=--fast"].map(String::from))
                .unwrap();
        let refusal = resolve_locations(&plain, &config, conf, &corpora, data).unwrap_err();
        assert!(refusal.contains("mezura@<tag>"), "{refusal}");
        let hollow: Config = toml::from_str(
            "[corpora]\nlinux = \"D:/linux\"\n[given.\"mezura@x\"]\ndefinition = \"D:/x.toml\"\n",
        )
        .unwrap();
        let refusal =
            resolve_locations(&parse("run linux").unwrap(), &hollow, conf, &corpora, data)
                .unwrap_err();
        assert!(refusal.contains("neither a binary nor args"), "{refusal}");
    }

    #[test]
    fn a_tree_with_no_corpus_definition_is_counted_by_its_extensions_under_its_own_name() {
        let conf = Path::new("linebench.conf");
        let data = Some(Path::new("D:/data/linebench"));
        let options = parse("run D:/src/tree --extensions .rs, c").unwrap();
        let locations = resolve_locations(&options, &Config::default(), conf, &[], data).unwrap();
        assert_eq!(locations.corpus.name, "tree");
        assert_eq!(locations.corpus.extensions, ["rs", "c"]);
        assert!(!locations.corpus.is_pinned() && locations.corpus.files.is_none());
        assert_eq!(locations.checkout, PathBuf::from("D:/src/tree"));
        assert_eq!(
            locations.counters_dir,
            PathBuf::from("D:/data/linebench/counters")
        );
        let known_here = vec![
            linebench::corpus::parse_corpus(
                "name = \"linux\"\nextensions = [\"c\"]\n",
                Path::new("linux.toml"),
            )
            .unwrap(),
        ];
        let named_too = parse("run linux --extensions rs").unwrap();
        let refusal =
            resolve_locations(&named_too, &Config::default(), conf, &known_here, data).unwrap_err();
        assert!(refusal.contains("declares its own"), "{refusal}");
        let no_path = parse("run --extensions rs").unwrap();
        let refusal = resolve_locations(&no_path, &Config::default(), conf, &[], data).unwrap_err();
        assert!(refusal.starts_with("Say what to count"), "{refusal}");
        let no_list = parse("run D:/src/tree").unwrap();
        let refusal = resolve_locations(&no_list, &Config::default(), conf, &[], data).unwrap_err();
        assert!(refusal.contains("--extensions rs,c"), "{refusal}");
        let known = vec![
            linebench::corpus::parse_corpus(
                "name = \"tree\"\nextensions = [\"c\"]\n",
                Path::new("tree.toml"),
            )
            .unwrap(),
        ];
        let refusal =
            resolve_locations(&options, &Config::default(), conf, &known, data).unwrap_err();
        assert!(refusal.contains("name tree on its own"), "{refusal}");
        let dot = parse("run D:/src/tree --extensions .").unwrap();
        let refusal = resolve_locations(&dot, &Config::default(), conf, &[], data).unwrap_err();
        assert!(refusal.contains("empty"), "{refusal}");
    }

    #[test]
    fn the_conf_leaves_counters_out_by_name_and_a_name_no_definition_has_is_refused() {
        let config: Config =
            toml::from_str("skip = [\"cloc\"]\n[corpora]\nlinux = \"D:/linux\"\n").unwrap();
        let corpora = vec![
            linebench::corpus::parse_corpus(
                "name = \"linux\"\nextensions = [\"c\"]\n",
                Path::new("linux.toml"),
            )
            .unwrap(),
        ];
        let conf = Path::new("linebench.conf");
        let data = Some(Path::new("D:/data/linebench"));
        let locations =
            resolve_locations(&parse("run linux").unwrap(), &config, conf, &corpora, data).unwrap();
        assert_eq!(locations.skip, ["cloc"]);
        let counters = linebench::counters::read_definitions(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/counters"
        )))
        .unwrap();
        assert!(check_skip_names(&locations.skip, &counters, conf).is_ok());
        let refused = check_skip_names(&["nope".to_string()], &counters, conf).unwrap_err();
        assert!(refused.contains("skip names nope"), "{refused}");
    }

    #[test]
    fn the_example_conf_reads_back_with_every_key_where_it_was_meant() {
        let config: Config = toml::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/linebench.conf.example"
        )))
        .unwrap();
        assert_eq!(config.control.as_deref(), Some("mezura"));
        assert_eq!(config.corpora.len(), 1);
        assert!(config.corpora.contains_key("linux"));
        assert!(config.given.is_empty() && config.add.is_empty() && config.skip.is_empty());
        assert!(config.counters.is_none() && config.out.is_none());
    }

    fn parse(line: &str) -> Result<Options, String> {
        let args: Vec<String> = std::iter::once("linebench")
            .chain(line.split_whitespace())
            .map(String::from)
            .collect();
        parse_args(&args)
    }
}
