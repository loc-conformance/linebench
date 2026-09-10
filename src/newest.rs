use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::thread;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::counters::{Acquisition, Channel, Definition};
use crate::fetch::{
    Manifest, explain_github_refusal, mentions_any, read_github_api_within, read_url_within,
};
use crate::files::read_toml;

pub const NEWEST_FILE: &str = "linebench-newest.toml";
pub const CACHE_SECONDS: u64 = 6 * 60 * 60;
const LOOKUP_SECONDS: u32 = 10;
const GITHUB_API: &str = "https://api.github.com/repos";
const CRATES_API: &str = "https://crates.io/api/v1/crates";
const NOT_FOUND_MARKS: [&str; 2] = ["error: 404", "error 404"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standing {
    Same,
    Newer,
    Behind,
    Differs,
}

pub struct Lookup<'a> {
    pub definition: &'a Definition,
    pub outcome: Result<String, String>,
    pub age_seconds: Option<u64>,
}

pub fn choose_counters_to_look_up<'a>(
    definitions: impl IntoIterator<Item = &'a Definition>,
) -> Vec<&'a Definition> {
    let mut chosen: Vec<&Definition> = Vec::new();
    for definition in definitions {
        if definition.acquisition.is_some() && !chosen.iter().any(|d| d.name == definition.name) {
            chosen.push(definition);
        }
    }
    chosen
}

pub fn collect_newest_releases<'a>(
    definitions: &[&'a Definition],
    dir: &Path,
    now: u64,
    look_up: impl Fn(&Definition) -> Result<String, String> + Sync,
) -> (Vec<Lookup<'a>>, Option<String>) {
    let mut cache = read_cache(dir);
    let look_up = &look_up;
    let lookups: Vec<Lookup> = thread::scope(|scope| {
        let handles: Vec<_> = definitions
            .iter()
            .map(|definition| {
                let cached = definition.acquisition.as_ref().and_then(|how| {
                    cache
                        .get(&definition.name)
                        .filter(|entry| entry.channel == how.channel && entry.name == how.name)
                        .filter(|entry| now.saturating_sub(entry.looked_up_at) < CACHE_SECONDS)
                });
                match cached {
                    Some(entry) => Ok((
                        entry.version.clone(),
                        now.saturating_sub(entry.looked_up_at),
                    )),
                    None => Err(scope.spawn(move || look_up(definition))),
                }
            })
            .collect();
        definitions
            .iter()
            .zip(handles)
            .map(|(definition, handle)| match handle {
                Ok((version, age)) => Lookup {
                    definition,
                    outcome: Ok(version),
                    age_seconds: Some(age),
                },
                Err(handle) => Lookup {
                    definition,
                    outcome: handle.join().unwrap_or_else(|_| {
                        Err(format!("the lookup for {} stopped short", definition.name))
                    }),
                    age_seconds: None,
                },
            })
            .collect()
    });
    let mut fresh = false;
    for lookup in &lookups {
        if let (Ok(version), None, Some(how)) = (
            &lookup.outcome,
            lookup.age_seconds,
            &lookup.definition.acquisition,
        ) {
            cache.insert(
                lookup.definition.name.clone(),
                Cached {
                    channel: how.channel,
                    name: how.name.clone(),
                    version: version.clone(),
                    looked_up_at: now,
                },
            );
            fresh = true;
        }
    }
    let warning = match fresh {
        true => write_cache(dir, &cache).err(),
        false => None,
    };
    (lookups, warning)
}

pub fn find_newest_release(definition: &Definition) -> Result<String, String> {
    let Some(how) = &definition.acquisition else {
        return Err(format!(
            "{} says nothing about where it is fetched from, so there is no release to look up",
            definition.name
        ));
    };
    match how.channel {
        Channel::GithubReleaseAsset | Channel::GithubReleaseFile => read_github_latest(&how.name),
        Channel::CratesIo => read_crate_version(&how.name),
    }
}

pub fn compare_versions(one: &str, other: &str) -> Option<Ordering> {
    let parse = |text: &str| -> Option<Vec<u64>> {
        strip_leading_v(text)
            .split('.')
            .map(|part| part.parse().ok())
            .collect()
    };
    let (mut one, mut other) = (parse(one)?, parse(other)?);
    let width = one.len().max(other.len());
    one.resize(width, 0);
    other.resize(width, 0);
    Some(one.cmp(&other))
}

pub fn judge_newest(how: &Acquisition, newest: &str) -> Standing {
    match compare_versions(newest, &how.version) {
        Some(Ordering::Greater) => Standing::Newer,
        Some(Ordering::Equal) => Standing::Same,
        Some(Ordering::Less) => Standing::Behind,
        None if strip_leading_v(newest) == strip_leading_v(&how.version) => Standing::Same,
        None => Standing::Differs,
    }
}

pub fn apply_newest_pins(definitions: &mut [Definition], manifest: &Manifest) -> Vec<String> {
    let mut set_aside = Vec::new();
    for definition in definitions {
        let Some(entry) = manifest
            .0
            .get(&definition.name)
            .filter(|entry| entry.newest)
        else {
            continue;
        };
        let Some(how) = definition.acquisition.as_mut() else {
            continue;
        };
        let (name, pin) = (&definition.name, &entry.version);
        if definition.added {
            set_aside.push(format!(
                "{name}: the pin of {pin} from fetch --newest is set aside, since the \
                 definition comes from {}",
                definition.path.display()
            ));
            continue;
        }
        if entry.channel != how.channel {
            set_aside.push(format!(
                "{name}: the pin of {pin} from fetch --newest is set aside, since the \
                 definition now fetches from another channel"
            ));
            continue;
        }
        if compare_versions(pin, &how.version) != Some(Ordering::Greater) {
            set_aside.push(format!(
                "{name}: the definition now pins {}, so the pin of {pin} from fetch --newest \
                 is set aside",
                how.version
            ));
            continue;
        }
        let shipped = std::mem::replace(&mut how.version, pin.clone());
        definition.shipped_version = Some(shipped);
    }
    set_aside
}

pub fn remember_newest(
    dir: &Path,
    name: &str,
    how: &Acquisition,
    version: &str,
    now: u64,
) -> Result<(), String> {
    let mut cache = read_cache(dir);
    cache.insert(
        name.to_string(),
        Cached {
            channel: how.channel,
            name: how.name.clone(),
            version: version.to_string(),
            looked_up_at: now,
        },
    );
    write_cache(dir, &cache)
}

pub fn describe_newest(
    name: &str,
    how: &Acquisition,
    shipped: Option<&str>,
    newest: &str,
) -> String {
    let pinned = &how.version;
    let release = match how.channel {
        Channel::CratesIo => "crates.io release",
        Channel::GithubReleaseAsset | Channel::GithubReleaseFile => "release",
    };
    let by = describe_pinned_version(how, shipped);
    let mut text = match judge_newest(how, newest) {
        Standing::Newer => format!(
            "the newest {release} is {newest}; {by}; fetch --counters {name} --newest fetches it"
        ),
        Standing::Same => format!("{pinned} is the newest {release}"),
        Standing::Behind => format!("{by}, ahead of the newest {release} {newest}"),
        Standing::Differs => format!("the newest {release} is tagged {newest}; {by}"),
    };
    if let Some(origin) = describe_pin_origin(how, shipped) {
        text.push_str(&format!(" ({origin})"));
    }
    text
}

pub fn describe_pinned_version(how: &Acquisition, shipped: Option<&str>) -> String {
    match shipped {
        Some(_) => format!("the pin in effect is {}", how.version),
        None => format!("the definition pins {}", how.version),
    }
}

pub fn describe_pin_origin(how: &Acquisition, shipped: Option<&str>) -> Option<String> {
    shipped.map(|shipped| {
        format!(
            "fetch --newest pinned {} over the {shipped} the definition ships with",
            how.version
        )
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct Cached {
    channel: Channel,
    name: String,
    version: String,
    looked_up_at: u64,
}

type Cache = BTreeMap<String, Cached>;

fn read_cache(dir: &Path) -> Cache {
    let path = dir.join(NEWEST_FILE);
    if !path.is_file() {
        return Cache::default();
    }
    read_toml(&path).unwrap_or_default()
}

fn write_cache(dir: &Path, cache: &Cache) -> Result<(), String> {
    let path = dir.join(NEWEST_FILE);
    let text = toml::to_string(cache)
        .map_err(|error| format!("the release lookups could not be written out: {error}"))?;
    fs::write(&path, text)
        .map_err(|error| format!("{} could not be written: {error}", path.display()))
}

fn read_github_latest(repository: &str) -> Result<String, String> {
    let url = format!("{GITHUB_API}/{repository}/releases/latest");
    let text = read_github_api_within(&url, LOOKUP_SECONDS)
        .map_err(|message| explain_newest_refusal(repository, &[message]))?;
    parse_latest_tag(repository, &text)
}

fn read_crate_version(name: &str) -> Result<String, String> {
    let url = format!("{CRATES_API}/{name}");
    let text = read_url_within(&url, LOOKUP_SECONDS).map_err(|message| {
        format!("crates.io could not be asked what {name} has published: {message}")
    })?;
    parse_crate_version(name, &text)
}

fn parse_latest_tag(repository: &str, text: &str) -> Result<String, String> {
    let document: Value = serde_json::from_str(text).map_err(|error| {
        format!("what github answered about {repository} does not read: {error}")
    })?;
    document
        .get("tag_name")
        .and_then(Value::as_str)
        .map(|tag| strip_leading_v(tag).to_string())
        .ok_or_else(|| format!("what github answered about {repository} names no tag"))
}

fn parse_crate_version(name: &str, text: &str) -> Result<String, String> {
    let document: Value = serde_json::from_str(text)
        .map_err(|error| format!("what crates.io answered about {name} does not read: {error}"))?;
    document
        .get("crate")
        .and_then(|found| found.get("max_stable_version"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("crates.io lists no stable version of {name}"))
}

fn explain_newest_refusal(repository: &str, refused: &[String]) -> String {
    if let Some(explained) = explain_github_refusal(repository, refused) {
        return explained;
    }
    if mentions_any(refused, &NOT_FOUND_MARKS) {
        return format!("github lists no latest release for {repository}");
    }
    format!(
        "github could not be asked what {repository} has released: {}",
        refused.join("; ")
    )
}

fn strip_leading_v(text: &str) -> &str {
    text.strip_prefix('v')
        .filter(|rest| rest.starts_with(|c: char| c.is_ascii_digit()))
        .unwrap_or(text)
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::path::PathBuf;
    use std::sync::Mutex;

    use super::*;
    use crate::counters::parse_definition;
    use crate::fetch::Fetched;

    const SCC: &str = "\
name = \"scc\"

[acquisition]
channel = \"github-release-asset\"
name    = \"boyter/scc\"
version = \"4.0.0\"

[run]
args      = [\"{target}\"]
languages = [\"-i\", \"{extensions}\"]

[read]
files    = \"Count\"
lines    = \"Lines\"
code     = \"Code\"
comments = \"Comment\"
blanks   = \"Blank\"
";

    const CLOC: &str = "\
name = \"cloc\"

[acquisition]
channel = \"github-release-file\"
name    = \"AlDanial/cloc\"
version = \"2.10\"

[acquisition.file]
other = \"cloc-{version}.pl\"

[run]
args      = [\"{target}\"]
languages = [\"--include-ext={extensions}\"]

[read]
files    = \"header.n_files\"
lines    = \"header.n_lines\"
code     = \"SUM.code\"
comments = \"SUM.comment\"
blanks   = \"SUM.blank\"
";

    const TOKEI: &str = "\
name = \"tokei\"

[acquisition]
channel = \"crates-io\"
name    = \"tokei\"
version = \"14.0.0\"

[run]
args      = [\"{target}\"]
languages = [\"-t\", \"{extensions}\"]

[read]
files    = \"Total.reports[]\"
lines    = \"Total.lines\"
code     = \"Total.code\"
comments = \"Total.comments\"
blanks   = \"Total.blanks\"
";

    #[test]
    fn versions_compare_by_number_per_component_and_a_tag_prefix_v_is_ignored() {
        assert_eq!(compare_versions("2.10", "2.9"), Some(Ordering::Greater));
        assert_eq!(compare_versions("4.0.0", "4.0"), Some(Ordering::Equal));
        assert_eq!(compare_versions("v4.1.0", "4.0.0"), Some(Ordering::Greater));
        assert_eq!(compare_versions("4.0.0", "4.10.0"), Some(Ordering::Less));
        assert_eq!(compare_versions("2.10a", "2.9"), None);
        assert_eq!(compare_versions("version", "2.9"), None);
    }

    #[test]
    fn the_newest_is_judged_against_the_pin_and_described_the_same_way() {
        let scc = parse_definition(SCC, &PathBuf::from("scc.toml")).unwrap();
        let how = scc.acquisition.as_ref().unwrap();
        assert_eq!(judge_newest(how, "4.0.0"), Standing::Same);
        assert_eq!(
            describe_newest("scc", how, None, "4.0.0"),
            "4.0.0 is the newest release"
        );
        assert_eq!(judge_newest(how, "4.1.0"), Standing::Newer);
        assert_eq!(
            describe_newest("scc", how, None, "4.1.0"),
            "the newest release is 4.1.0; the definition pins 4.0.0; fetch --counters scc \
             --newest fetches it"
        );
        assert_eq!(judge_newest(how, "3.9.0"), Standing::Behind);
        assert_eq!(
            describe_newest("scc", how, None, "3.9.0"),
            "the definition pins 4.0.0, ahead of the newest release 3.9.0"
        );
        assert_eq!(judge_newest(how, "4.1.0-rc1"), Standing::Differs);
        assert_eq!(
            describe_newest("scc", how, None, "4.1.0-rc1"),
            "the newest release is tagged 4.1.0-rc1; the definition pins 4.0.0"
        );
        assert_eq!(
            describe_newest("scc", how, Some("3.7.0"), "4.0.0"),
            "4.0.0 is the newest release (fetch --newest pinned 4.0.0 over the 3.7.0 the \
             definition ships with)"
        );
        assert_eq!(
            describe_newest("scc", how, Some("3.7.0"), "4.2.0"),
            "the newest release is 4.2.0; the pin in effect is 4.0.0; fetch --counters scc \
             --newest fetches it (fetch --newest pinned 4.0.0 over the 3.7.0 the definition \
             ships with)"
        );
        let cloc = parse_definition(CLOC, &PathBuf::from("cloc.toml")).unwrap();
        assert_eq!(
            judge_newest(cloc.acquisition.as_ref().unwrap(), "v2.10"),
            Standing::Same
        );
        let tokei = parse_definition(TOKEI, &PathBuf::from("tokei.toml")).unwrap();
        assert_eq!(
            describe_newest("tokei", tokei.acquisition.as_ref().unwrap(), None, "15.0.0"),
            "the newest crates.io release is 15.0.0; the definition pins 14.0.0; fetch \
             --counters tokei --newest fetches it"
        );
    }

    #[test]
    fn a_newest_pin_applies_over_a_shipped_definition_it_is_ahead_of_and_nowhere_else() {
        let scc = parse_definition(SCC, &PathBuf::from("scc.toml")).unwrap();
        let mut caught_up = scc.clone();
        caught_up.acquisition.as_mut().unwrap().version = "4.1.0".to_string();
        let mut own = scc.clone();
        own.name = "own".to_string();
        own.added = true;
        own.path = PathBuf::from("mine/own.toml");
        let mut moved = scc.clone();
        moved.name = "moved".to_string();
        moved.acquisition.as_mut().unwrap().channel = Channel::CratesIo;
        let mut plain = parse_definition(TOKEI, &PathBuf::from("tokei.toml")).unwrap();
        plain.name = "plain".to_string();
        let entry = |name: &str, version: &str, channel: Channel, newest: bool| {
            (
                name.to_string(),
                Fetched {
                    channel,
                    version: version.to_string(),
                    source: String::new(),
                    sha256: String::new(),
                    built_with: None,
                    newest,
                },
            )
        };
        let manifest = Manifest(
            [
                entry("scc", "4.1.0", Channel::GithubReleaseAsset, true),
                entry("own", "4.1.0", Channel::GithubReleaseAsset, true),
                entry("moved", "4.1.0", Channel::GithubReleaseAsset, true),
                entry("plain", "15.0.0", Channel::CratesIo, false),
            ]
            .into_iter()
            .collect(),
        );
        let mut pinned = scc.clone();
        let lines = apply_newest_pins(std::slice::from_mut(&mut pinned), &manifest);
        assert!(lines.is_empty(), "{lines:?}");
        assert_eq!(pinned.acquisition.as_ref().unwrap().version, "4.1.0");
        assert_eq!(pinned.shipped_version.as_deref(), Some("4.0.0"));
        let mut untouched = [caught_up, own, moved, plain];
        let lines = apply_newest_pins(&mut untouched, &manifest);
        assert_eq!(
            lines,
            [
                "scc: the definition now pins 4.1.0, so the pin of 4.1.0 from fetch --newest is \
                 set aside",
                "own: the pin of 4.1.0 from fetch --newest is set aside, since the definition \
                 comes from mine/own.toml",
                "moved: the pin of 4.1.0 from fetch --newest is set aside, since the definition \
                 now fetches from another channel",
            ]
        );
        for definition in &untouched {
            assert_eq!(definition.shipped_version, None, "{}", definition.name);
        }
        assert_eq!(untouched[1].acquisition.as_ref().unwrap().version, "4.0.0");
        assert_eq!(untouched[2].acquisition.as_ref().unwrap().version, "4.0.0");
        assert_eq!(untouched[3].acquisition.as_ref().unwrap().version, "14.0.0");
    }

    #[test]
    fn what_github_and_crates_io_answer_is_read_down_to_a_version() {
        assert_eq!(
            parse_latest_tag("boyter/scc", r#"{"tag_name":"v4.1.0","name":"scc"}"#).unwrap(),
            "4.1.0"
        );
        assert!(
            parse_latest_tag("boyter/scc", r#"{"name":"scc"}"#)
                .unwrap_err()
                .contains("names no tag")
        );
        assert_eq!(
            parse_crate_version(
                "tokei",
                r#"{"crate":{"max_version":"15.0.0-alpha","max_stable_version":"14.0.0"}}"#
            )
            .unwrap(),
            "14.0.0"
        );
        assert!(
            parse_crate_version("tokei", r#"{"crate":{"max_stable_version":null}}"#)
                .unwrap_err()
                .contains("no stable version")
        );
        let missing = ["curl: (22) The requested URL returned error: 404".to_string()];
        assert_eq!(
            explain_newest_refusal("boyter/scc", &missing),
            "github lists no latest release for boyter/scc"
        );
        let limited = ["curl: (22) The requested URL returned error: 403".to_string()];
        assert!(explain_newest_refusal("boyter/scc", &limited).contains("GITHUB_TOKEN"));
        let stalled = ["curl: (28) Operation timed out after 10001 milliseconds".to_string()];
        assert!(
            explain_newest_refusal("boyter/scc", &stalled)
                .starts_with("github could not be asked what boyter/scc has released")
        );
    }

    #[test]
    fn one_lookup_per_counter_and_a_fresh_answer_for_the_same_source_comes_from_the_cache() {
        let scc = parse_definition(SCC, &PathBuf::from("scc.toml")).unwrap();
        let mut scc_again = scc.clone();
        scc_again.path = PathBuf::from("other/scc.toml");
        let mut bare = scc.clone();
        bare.name = "mine".to_string();
        bare.acquisition = None;
        let tokei = parse_definition(TOKEI, &PathBuf::from("tokei.toml")).unwrap();
        let chosen = choose_counters_to_look_up([&scc, &scc_again, &bare, &tokei]);
        let names: Vec<&str> = chosen.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, ["scc", "tokei"]);

        let dir = env::temp_dir().join("linebench-one_lookup_per_counter");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let asked = Mutex::new(Vec::new());
        let look_up = |definition: &Definition| {
            asked.lock().unwrap().push(definition.name.clone());
            match definition.name.as_str() {
                "scc" => Ok("4.1.0".to_string()),
                other => Err(format!("{other} is offline")),
            }
        };
        let (first, warning) = collect_newest_releases(&chosen, &dir, 1_000_000, look_up);
        assert_eq!(warning, None);
        assert_eq!(first.len(), 2);
        assert_eq!(first[0].outcome.as_ref().unwrap(), "4.1.0");
        assert_eq!(first[0].age_seconds, None);
        assert_eq!(first[1].outcome.as_ref().unwrap_err(), "tokei is offline");
        let (second, _) = collect_newest_releases(&chosen, &dir, 1_000_000 + 3_600, look_up);
        assert_eq!(second[0].age_seconds, Some(3_600));
        assert_eq!(second[0].outcome.as_ref().unwrap(), "4.1.0");
        assert_eq!(second[1].age_seconds, None);
        let mut fork = scc.clone();
        fork.acquisition.as_mut().unwrap().name = "someone/scc".to_string();
        let (forked, _) = collect_newest_releases(&[&fork], &dir, 1_000_000 + 3_600, look_up);
        assert_eq!(forked[0].age_seconds, None);
        let (third, _) = collect_newest_releases(&chosen, &dir, 1_000_000 + CACHE_SECONDS, look_up);
        assert_eq!(third[0].age_seconds, None);
        let (_, refused) =
            collect_newest_releases(&chosen, &dir.join("missing"), 2_000_000, look_up);
        assert!(refused.unwrap().contains("could not be written"));
        let how = tokei.acquisition.as_ref().unwrap();
        remember_newest(&dir, "tokei", how, "15.0.0", 2_000_000).unwrap();
        let (remembered, _) = collect_newest_releases(&chosen, &dir, 2_000_100, look_up);
        assert_eq!(remembered[1].age_seconds, Some(100));
        assert_eq!(remembered[1].outcome.as_ref().unwrap(), "15.0.0");
        fs::remove_dir_all(&dir).unwrap();
        let mut asked = asked.into_inner().unwrap();
        asked.sort();
        assert_eq!(
            asked,
            [
                "scc", "scc", "scc", "scc", "scc", "tokei", "tokei", "tokei", "tokei"
            ]
        );
    }
}
