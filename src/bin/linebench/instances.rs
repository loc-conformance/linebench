use std::collections::BTreeSet;
use std::path::Path;

use linebench::corpus::Corpus;
use linebench::counters::{Definition, read_definition_for};
use linebench::fetch::{INSTANCE_SEPARATOR, identify_counter, read_manifest, stage_given};
use linebench::machine::Platform;
use linebench::measure::Instance;

use crate::config::{Locations, Options};

pub struct ChosenInstances {
    pub instances: Vec<Instance>,
    pub control: usize,
    pub left_out: Vec<String>,
    pub warnings: Vec<String>,
}

pub fn build_instances(
    definitions: &[Definition],
    locations: &Locations,
    options: &Options,
    platform: Platform,
) -> Result<ChosenInstances, String> {
    let manifest = read_manifest(&locations.counters_dir)?;
    let system = platform.as_system();
    let corpus = &locations.corpus;
    let mut left_out = Vec::new();
    let mut warnings = Vec::new();
    let selected: Vec<String> = match &options.counters {
        Some(named) => {
            if named.is_empty() {
                return Err("--counters names nothing".to_string());
            }
            for name in named {
                if corpus.skips(system, name) {
                    warnings.push(format!(
                        "{name} runs because --counters names it. The {} definition leaves it \
                         out on {system}",
                        corpus.name
                    ));
                }
            }
            named.clone()
        }
        None => {
            let (set_up, reasons) = choose_set_up(
                definitions,
                &locations.counters_dir,
                platform,
                corpus,
                &locations.skip,
            )?;
            left_out = reasons;
            let selected: Vec<String> = set_up
                .into_iter()
                .filter(|name| !locations.given.contains_key(name))
                .chain(locations.given.keys().cloned())
                .collect();
            if selected.is_empty() {
                let any_skipped = definitions
                    .iter()
                    .any(|d| corpus.skips(system, &d.name) || locations.skip.contains(&d.name));
                return Err(if any_skipped {
                    format!(
                        "every counter that is set up is left out, by linebench.conf or by the \
                         corpus definition on {system}; name one with --counters"
                    )
                } else {
                    "no counter is downloaded yet: run fetch --counters all".to_string()
                });
            }
            selected
        }
    };
    let mut seen = BTreeSet::new();
    let mut instances = Vec::new();
    for name in &selected {
        if !seen.insert(name.clone()) {
            return Err(format!("{name} is named twice"));
        }
        let instance = if let Some(entry) = locations.given.get(name) {
            let counter = name
                .split_once(INSTANCE_SEPARATOR)
                .map_or(name.as_str(), |(counter, _)| counter);
            let definition = match &entry.definition {
                Some(path) => read_definition_for(path, counter)?,
                None => find_definition(definitions, counter)?,
            };
            let tag = check_given_name(name, &definition)?;
            let identity = match &entry.binary {
                Some(binary) => {
                    stage_given(&definition, tag, binary, platform, &locations.counters_dir)?
                }
                None => {
                    let mut identity = identify_counter(
                        &definition,
                        platform,
                        &locations.counters_dir,
                        &manifest,
                    )?;
                    identity.instance = name.clone();
                    identity
                }
            };
            Instance {
                definition,
                identity,
                args: entry.args.clone(),
            }
        } else {
            let definition = find_definition(definitions, name).map_err(|_| {
                format!(
                    "{name} is neither a counter definition ({}) nor a given instance ({})",
                    definitions
                        .iter()
                        .map(|d| d.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                    if locations.given.is_empty() {
                        "none".to_string()
                    } else {
                        locations
                            .given
                            .keys()
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(", ")
                    }
                )
            })?;
            let identity =
                identify_counter(&definition, platform, &locations.counters_dir, &manifest)?;
            Instance {
                definition,
                identity,
                args: Vec::new(),
            }
        };
        instances.push(instance);
    }
    let find_control = |named: &str| instances.iter().position(|i| i.get_name() == named);
    let control = match (&options.control, &locations.control) {
        (Some(named), _) => find_control(named).ok_or_else(|| {
            let skipped = if locations.skip.contains(named) {
                format!(", and linebench.conf leaves {named} out")
            } else if corpus.skips(system, named) {
                format!(", and the corpus definition leaves {named} out on {system}")
            } else {
                String::new()
            };
            format!(
                "--control {named} is not among the instances of this run ({}){skipped}",
                selected.join(", ")
            )
        })?,
        (None, Some(named)) => match find_control(named) {
            Some(found) => found,
            None => {
                warnings.push(format!(
                    "the control {named} from linebench.conf is not in this run, so {} stands in \
                     as the control",
                    instances[0].get_name()
                ));
                0
            }
        },
        (None, None) => 0,
    };
    Ok(ChosenInstances {
        instances,
        control,
        left_out,
        warnings,
    })
}

fn check_given_name<'a>(name: &'a str, definition: &Definition) -> Result<Option<&'a str>, String> {
    match name.split_once(INSTANCE_SEPARATOR) {
        Some((_, tag)) => Ok(Some(tag)),
        None if definition.acquisition.is_none() => Ok(None),
        None => Err(format!(
            "{name} has a release that the fetch command takes, so a build of your own is named \
             {name}@<tag>"
        )),
    }
}

fn choose_set_up(
    definitions: &[Definition],
    counters_dir: &Path,
    platform: Platform,
    corpus: &Corpus,
    conf_skip: &[String],
) -> Result<(Vec<String>, Vec<String>), String> {
    let system = platform.as_system();
    let mut set_up = Vec::new();
    let mut left_out = Vec::new();
    for definition in definitions {
        let binary = counters_dir.join(definition.get_binary_name(system)?);
        let could_run = binary.is_file() || definition.acquisition.is_some();
        if could_run && conf_skip.contains(&definition.name) {
            left_out.push(format!("{}: linebench.conf leaves it out", definition.name));
        } else if could_run && corpus.skips(system, &definition.name) {
            left_out.push(format!(
                "{}: the corpus definition leaves it out on {system}",
                definition.name
            ));
        } else if binary.is_file() {
            set_up.push(definition.name.clone());
        } else if definition.acquisition.is_some() {
            left_out.push(format!(
                "{}: it is not set up on this machine",
                definition.name
            ));
        }
    }
    Ok((set_up, left_out))
}

fn find_definition(definitions: &[Definition], counter: &str) -> Result<Definition, String> {
    definitions
        .iter()
        .find(|d| d.name == counter)
        .cloned()
        .ok_or_else(|| format!("no counter definition named {counter}"))
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;

    use linebench::corpus::parse_corpus;
    use linebench::counters::{parse_definition, read_definitions};

    use super::*;

    const GIVEN_ONLY: &str = "\
name = \"mine\"

[run]
args      = [\"{target}\"]
languages = [\"--ext\", \"{extensions}\"]

[read]
files    = \"files\"
lines    = \"lines\"
code     = \"code\"
comments = \"comments\"
blanks   = \"blanks\"
";

    #[test]
    fn the_default_set_is_what_setup_fetched_and_a_release_that_was_not_is_named_as_left_out() {
        let dir = env::temp_dir().join("linebench-the_default_set_is_what_setup_fetched");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("scc.exe"), "").unwrap();
        let mut definitions =
            read_definitions(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/counters"))).unwrap();
        definitions.push(parse_definition(GIVEN_ONLY, Path::new("mine.toml")).unwrap());
        let plain =
            parse_corpus("name = \"t\"\nextensions = [\"c\"]\n", Path::new("t.toml")).unwrap();
        let (set_up, left_out) =
            choose_set_up(&definitions, &dir, Platform::Windows, &plain, &[]).unwrap();
        assert_eq!(set_up, ["scc"]);
        assert_eq!(
            left_out,
            [
                "cloc: it is not set up on this machine",
                "mezura: it is not set up on this machine",
                "tokei: it is not set up on this machine"
            ]
        );
        let skipping = parse_corpus(
            "name = \"t\"\nextensions = [\"c\"]\n[skip]\nwindows = [\"scc\", \"cloc\"]\n",
            Path::new("t.toml"),
        )
        .unwrap();
        let (set_up, left_out) =
            choose_set_up(&definitions, &dir, Platform::Windows, &skipping, &[]).unwrap();
        assert!(set_up.is_empty());
        assert_eq!(
            left_out[0],
            "cloc: the corpus definition leaves it out on windows"
        );
        assert_eq!(
            left_out[2],
            "scc: the corpus definition leaves it out on windows"
        );
        assert!(left_out.iter().all(|entry| !entry.starts_with("mine")));
        let (_, on_wsl) = choose_set_up(&definitions, &dir, Platform::Wsl, &skipping, &[]).unwrap();
        assert!(
            on_wsl.iter().all(|entry| !entry.contains("leaves it out")),
            "{on_wsl:?}"
        );
        let by_conf = ["scc".to_string()];
        let (set_up, left_out) =
            choose_set_up(&definitions, &dir, Platform::Windows, &plain, &by_conf).unwrap();
        assert!(set_up.is_empty());
        assert_eq!(left_out[2], "scc: linebench.conf leaves it out");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_plain_name_is_a_given_build_only_for_a_counter_that_has_no_release() {
        let mine = parse_definition(GIVEN_ONLY, Path::new("mine.toml")).unwrap();
        let scc = read_definitions(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/counters")))
            .unwrap()
            .into_iter()
            .find(|d| d.name == "scc")
            .unwrap();
        assert_eq!(check_given_name("mine", &mine).unwrap(), None);
        assert_eq!(check_given_name("mine@x", &mine).unwrap(), Some("x"));
        assert_eq!(check_given_name("scc@dev", &scc).unwrap(), Some("dev"));
        let refused = check_given_name("scc", &scc).unwrap_err();
        assert!(refused.contains("scc@<tag>"), "{refused}");
    }
}
