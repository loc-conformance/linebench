use std::collections::BTreeMap;
use std::env;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use linebench::corpus::{Corpus, build_corpus_of};
use linebench::counters::Definition;
use linebench::fetch::INSTANCE_SEPARATOR;
use linebench::files::read_toml;
use linebench::os::capture_with_status;

pub const CONFIG_FILE: &str = "linebench.conf";
pub const DATA_DIR_NAME: &str = "linebench";
pub const COUNTERS_DIR_NAME: &str = "counters";
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
    Setup,
    Check,
    Noise,
    Insights,
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
            "run" | "setup" | "check" | "noise" | "insights" | "report" | "help" | "version"
                if options.command.is_none() =>
            {
                options.command = Some(match flag {
                    "run" => Command::Run,
                    "setup" => Command::Setup,
                    "check" => Command::Check,
                    "noise" => Command::Noise,
                    "insights" => Command::Insights,
                    "report" => Command::Report,
                    "help" => Command::Help,
                    _ => Command::Version,
                });
            }
            "--help" | "-h" => options.command = Some(Command::Help),
            "--version" | "-V" => options.command = Some(Command::Version),
            "--counters" => {
                let first = value()?;
                options.counters = Some(read_list(flag, first, &mut rest)?);
            }
            "--extensions" => {
                let first = value()?;
                options.extensions = Some(read_list(flag, first, &mut rest)?);
            }
            "--corpus" => options.corpus = Some(value()?),
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
            other => return Err(explain_the_unknown_argument(other)),
        }
    }
    Ok(options)
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

pub fn resolve_locations(
    options: &Options,
    config: &Config,
    config_path: &Path,
    corpora: &[Corpus],
    data_dir: Option<&Path>,
) -> Result<Locations, String> {
    let counters_dir = options
        .counters_dir
        .clone()
        .or_else(|| read_env(COUNTERS_ENV).map(PathBuf::from))
        .or_else(|| config.counters.clone())
        .or_else(|| data_dir.map(|dir| dir.join(COUNTERS_DIR_NAME)))
        .ok_or_else(|| explain_the_missing_counters_dir(config_path))?;
    let named = options.corpus.clone().or_else(|| read_env(CORPUS_ENV));
    let (corpus, checkout) = match (&options.corpus, &options.extensions) {
        (Some(name), Some(_)) => {
            return Err(format!(
                "--extensions is for a tree with no corpus definition, and {name} declares its own"
            ));
        }
        (None, Some(extensions)) => {
            let checkout = options.corpus_path.clone().ok_or_else(|| {
                "--extensions counts the tree at --corpus-path, and no --corpus-path was given"
                    .to_string()
            })?;
            let corpus = build_corpus_of(&checkout, extensions)?;
            if corpora.iter().any(|known| known.name == corpus.name) {
                return Err(format!(
                    "the directory is named {0}, and so is a corpus definition, so its runs would \
                     mix with that corpus's: use --corpus {0}, or rename the directory",
                    corpus.name
                ));
            }
            (corpus, checkout)
        }
        (_, None) => {
            let corpus_name = named
                .clone()
                .or_else(
                    || match config.corpora.keys().collect::<Vec<_>>().as_slice() {
                        [only] => Some((*only).clone()),
                        _ => None,
                    },
                )
                .ok_or_else(|| explain_the_missing_corpus(config_path, corpora))?;
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
                        "nothing says where the {corpus_name} checkout is: --corpus-path <dir>, \
                         or [corpora] {corpus_name} = \"<dir>\" in {}",
                        config_path.display()
                    )
                })?;
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

fn explain_the_missing_corpus(config_path: &Path, corpora: &[Corpus]) -> String {
    let known: Vec<&str> = corpora.iter().map(|c| c.name.as_str()).collect();
    let sample = known.first().copied().unwrap_or("linux");
    format!(
        "nothing says what to count. A tree as it stands:\n\n\
         \x20 linebench <command> --corpus-path <dir> --extensions rs,c\n\n\
         or a corpus definition ({}) and where its checkout is, in any of these:\n\n\
         \x20 flags   --corpus {sample} --corpus-path <dir>\n\
         \x20 env     {CORPUS_ENV}={sample}, with the checkout in the file\n\
         \x20 file    [corpora] {sample} = \"<dir>\" in {}",
        known.join(", "),
        config_path.display()
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

const COMMANDS: [&str; 8] = [
    "run", "setup", "check", "noise", "insights", "report", "help", "version",
];

fn split_list(flag: &str, text: &str) -> Result<Vec<String>, String> {
    let names: Vec<String> = text.split(',').map(|s| s.trim().to_string()).collect();
    if names.iter().any(|name| name.is_empty()) {
        return Err(format!("empty name in {flag} {text}"));
    }
    Ok(names)
}

fn explain_the_unknown_argument(other: &str) -> String {
    if COMMANDS.contains(&other) {
        return format!("second command: {other}");
    }
    if other.starts_with('-') {
        return format!("unknown flag: {other}");
    }
    format!("unknown argument: {other}")
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
        assert!(parse("setup --allow-elevated").unwrap().allow_elevated);
        assert!(parse("setup --newest").unwrap().newest);
        assert!(!parse("setup").unwrap().newest);
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
        assert_eq!(
            parse("run --nonsense").unwrap_err(),
            "unknown flag: --nonsense"
        );
        assert_eq!(
            parse("run mezura@dev").unwrap_err(),
            "unknown argument: mezura@dev"
        );
        assert_eq!(parse("run check").unwrap_err(), "second command: check");
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
        let options = parse("run --given mezura@dev=D:/new.exe").unwrap();
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
            refusal.contains("--corpus-path <dir> --extensions"),
            "{refusal}"
        );
        let refusal = resolve_locations(&options, &empty, conf, &corpora, None).unwrap_err();
        assert!(refusal.contains("--counters-dir <dir>"), "{refusal}");
        let orphan = parse("run --definition scc@x=D:/x.toml").unwrap();
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
            parse_args(&["linebench", "run", "--args", "mezura=--fast"].map(String::from)).unwrap();
        let refusal = resolve_locations(&plain, &config, conf, &corpora, data).unwrap_err();
        assert!(refusal.contains("mezura@<tag>"), "{refusal}");
        let hollow: Config = toml::from_str(
            "[corpora]\nlinux = \"D:/linux\"\n[given.\"mezura@x\"]\ndefinition = \"D:/x.toml\"\n",
        )
        .unwrap();
        let refusal =
            resolve_locations(&parse("run").unwrap(), &hollow, conf, &corpora, data).unwrap_err();
        assert!(refusal.contains("neither a binary nor args"), "{refusal}");
    }

    #[test]
    fn a_tree_with_no_corpus_definition_is_counted_by_its_extensions_under_its_own_name() {
        let conf = Path::new("linebench.conf");
        let data = Some(Path::new("D:/data/linebench"));
        let options = parse("run --corpus-path D:/src/tree --extensions .rs, c").unwrap();
        let locations = resolve_locations(&options, &Config::default(), conf, &[], data).unwrap();
        assert_eq!(locations.corpus.name, "tree");
        assert_eq!(locations.corpus.extensions, ["rs", "c"]);
        assert!(!locations.corpus.is_pinned() && locations.corpus.files.is_none());
        assert_eq!(locations.checkout, PathBuf::from("D:/src/tree"));
        assert_eq!(
            locations.counters_dir,
            PathBuf::from("D:/data/linebench/counters")
        );
        let named_too = parse("run --corpus linux --corpus-path D:/x --extensions rs").unwrap();
        let refusal =
            resolve_locations(&named_too, &Config::default(), conf, &[], data).unwrap_err();
        assert!(refusal.contains("declares its own"), "{refusal}");
        let no_path = parse("run --extensions rs").unwrap();
        let refusal = resolve_locations(&no_path, &Config::default(), conf, &[], data).unwrap_err();
        assert!(refusal.contains("no --corpus-path"), "{refusal}");
        let known = vec![
            linebench::corpus::parse_corpus(
                "name = \"tree\"\nextensions = [\"c\"]\n",
                Path::new("tree.toml"),
            )
            .unwrap(),
        ];
        let refusal =
            resolve_locations(&options, &Config::default(), conf, &known, data).unwrap_err();
        assert!(refusal.contains("use --corpus tree"), "{refusal}");
        let dot = parse("run --corpus-path D:/src/tree --extensions .").unwrap();
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
            resolve_locations(&parse("run").unwrap(), &config, conf, &corpora, data).unwrap();
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
