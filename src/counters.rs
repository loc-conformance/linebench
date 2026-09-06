use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::files::{parse_toml, read_text};
use crate::machine::WINDOWS;

pub const COUNTS: [&str; 4] = ["files", "lines", "code", "comments"];
pub const EACH: &str = "each";
pub const TARGET: &str = "{target}";
pub const EXTENSIONS: &str = "{extensions}";
pub const NAMES: &str = "{names}";
const VERSION: &str = "{version}";
const OTHER_SYSTEM: &str = "other";
const RELEASE_FILE_SYSTEMS: [&str; 4] = ["windows", "linux", "macos", "other"];
const DEFINITION_SUFFIX: &str = "toml";
const DEFAULT_VERSION_FLAG: &str = "--version";
const EXE_SUFFIX: &str = ".exe";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Channel {
    GithubReleaseAsset,
    GithubReleaseFile,
    CratesIo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Output {
    TokeiJson,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExtensionCase {
    #[default]
    Any,
    Exact,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Acquisition {
    pub channel: Channel,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub file: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Run {
    pub args: Vec<String>,
    #[serde(default)]
    pub json: Vec<String>,
    pub languages: Vec<String>,
    #[serde(default)]
    pub extension_case: ExtensionCase,
    #[serde(default)]
    pub same_work: Vec<String>,
    #[serde(default)]
    pub same_work_note: String,
    #[serde(default)]
    pub scrub_env: Vec<String>,
}

impl Run {
    pub fn spells_by_name(&self) -> bool {
        self.languages.iter().any(|part| part.contains(NAMES))
    }

    pub fn spell_extensions(&self, extensions: &[String]) -> String {
        if self.extension_case == ExtensionCase::Any {
            return extensions.join(",");
        }
        let mut spelled: Vec<String> = Vec::new();
        for extension in extensions {
            for spelling in [
                extension.clone(),
                extension.to_lowercase(),
                extension.to_uppercase(),
            ] {
                if !spelled.contains(&spelling) {
                    spelled.push(spelling);
                }
            }
        }
        spelled.join(",")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Definition {
    pub name: String,
    #[serde(skip)]
    pub path: PathBuf,
    pub repository: Option<String>,
    #[serde(default = "get_default_version_flag")]
    pub version_flag: String,
    pub output: Option<Output>,
    pub acquisition: Option<Acquisition>,
    pub run: Run,
    pub read: Option<BTreeMap<String, String>>,
    #[serde(default)]
    pub language_names: BTreeMap<String, String>,
}

impl Definition {
    pub fn build_args(
        &self,
        target: &Path,
        extensions: &[String],
        same_work: bool,
        as_json: bool,
    ) -> Result<Vec<String>, String> {
        let target = target.to_string_lossy();
        let mut args: Vec<String> = self
            .run
            .args
            .iter()
            .map(|part| part.replace(TARGET, &target))
            .collect();
        if same_work {
            args.extend(self.spell_languages(extensions)?);
            args.extend(self.run.same_work.iter().cloned());
        }
        if as_json {
            args.extend(self.run.json.iter().cloned());
        }
        Ok(args)
    }

    pub fn spell_languages(&self, extensions: &[String]) -> Result<Vec<String>, String> {
        let (placeholder, spelled) = if self.run.spells_by_name() {
            let mut names = Vec::new();
            let mut missing = Vec::new();
            for extension in extensions {
                match self.find_language_name(extension) {
                    Some(name) => names.push(name),
                    None => missing.push(format!(".{extension}")),
                }
            }
            if !missing.is_empty() {
                return Err(format!(
                    "{} does not say how it names {}: add them to [language-names] in {}",
                    self.name,
                    missing.join(", "),
                    self.path.display()
                ));
            }
            (NAMES, names.join(","))
        } else {
            (EXTENSIONS, self.run.spell_extensions(extensions))
        };
        Ok(self
            .run
            .languages
            .iter()
            .map(|part| part.replace(placeholder, &spelled))
            .collect())
    }

    pub fn get_binary_name(&self, system: &str) -> Result<String, String> {
        let released = if self
            .acquisition
            .as_ref()
            .is_some_and(|how| !how.file.is_empty())
        {
            Some(self.get_release_file_name(system)?)
        } else {
            None
        };
        let extension = released
            .as_deref()
            .and_then(|released| Path::new(released).extension())
            .and_then(|extension| extension.to_str())
            .filter(|extension| extension.chars().all(|c| c.is_ascii_alphabetic()))
            .map(|extension| format!(".{extension}"))
            .unwrap_or_else(|| {
                if system == WINDOWS {
                    EXE_SUFFIX.to_string()
                } else {
                    String::new()
                }
            });
        Ok(format!("{}{extension}", self.name))
    }

    pub fn find_language_name(&self, extension: &str) -> Option<&str> {
        self.language_names
            .iter()
            .find(|(known, _)| known.eq_ignore_ascii_case(extension))
            .map(|(_, name)| name.as_str())
    }

    pub fn get_release_file_name(&self, system: &str) -> Result<String, String> {
        let Some(acquisition) = self.acquisition.as_ref().filter(|how| !how.file.is_empty()) else {
            return Err(format!(
                "{}: has no [acquisition.file] table, so no release file is named",
                self.path.display()
            ));
        };
        let named = acquisition
            .file
            .get(system)
            .or_else(|| acquisition.file.get(OTHER_SYSTEM))
            .ok_or_else(|| {
                let keys: Vec<&str> = acquisition.file.keys().map(String::as_str).collect();
                format!(
                    "{}: [acquisition.file] names no release file for {system}, only for {}; \
                     add {system} or other",
                    self.path.display(),
                    keys.join(", ")
                )
            })?;
        Ok(named.replace(VERSION, &acquisition.version))
    }

    pub fn get_process_name(&self, system: &str) -> Result<String, String> {
        let binary = self.get_binary_name(system)?;
        Ok(Path::new(&binary)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or(binary))
    }
}

pub fn read_definitions(dir: &Path) -> Result<Vec<Definition>, String> {
    let entries = fs::read_dir(dir)
        .map_err(|error| format!("no counter definitions at {}: {error}", dir.display()))?;
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == DEFINITION_SUFFIX))
        .filter(|path| path.is_file())
        .collect();
    paths.sort();
    paths.iter().map(|path| read_definition(path)).collect()
}

pub fn read_definition(path: &Path) -> Result<Definition, String> {
    parse_definition(&read_text(path)?, path)
}

pub fn read_definition_for(path: &Path, counter: &str) -> Result<Definition, String> {
    parse_definition_for(&read_text(path)?, path, counter)
}

pub fn parse_definition(text: &str, path: &Path) -> Result<Definition, String> {
    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    parse_definition_for(text, path, &stem)
}

pub fn parse_definition_for(text: &str, path: &Path, counter: &str) -> Result<Definition, String> {
    let mut definition: Definition = parse_toml(text, path)?;
    definition.path = path.to_path_buf();
    check_definition(&definition, counter)?;
    Ok(definition)
}

fn check_definition(definition: &Definition, counter: &str) -> Result<(), String> {
    let at = format!("{}: ", definition.path.display());
    if definition.name != counter {
        return Err(format!(
            "{at}name = \"{}\" and this is meant to be {counter}; the two have to agree",
            definition.name
        ));
    }
    match (&definition.output, &definition.read) {
        (Some(_), Some(_)) => {
            return Err(format!(
                "{at}names both an output and a [read] block. A [read] block says where the \
                 counts sit in the counter's own JSON and an output names a reader compiled \
                 here, so it is one or the other"
            ));
        }
        (None, None) => {
            return Err(format!(
                "{at}says nothing about how to read the counts: give a [read] block or an output"
            ));
        }
        (None, Some(read)) => check_read_block(&at, read)?,
        (Some(_), None) => {}
    }
    check_run_block(&at, definition)?;
    if let Some(acquisition) = &definition.acquisition {
        check_acquisition(&at, acquisition)?;
    }
    Ok(())
}

fn check_read_block(at: &str, read: &BTreeMap<String, String>) -> Result<(), String> {
    let missing: Vec<&str> = COUNTS
        .iter()
        .copied()
        .filter(|count| !read.contains_key(*count))
        .collect();
    if !missing.is_empty() {
        return Err(format!(
            "{at}[read] names no path for {}",
            missing.join(", ")
        ));
    }
    if !read
        .keys()
        .any(|key| key != EACH && !COUNTS.contains(&key.as_str()))
    {
        return Err(format!(
            "{at}[read] names code and comments and no third bucket, and every counter has one, \
             blanks for most"
        ));
    }
    Ok(())
}

fn check_run_block(at: &str, definition: &Definition) -> Result<(), String> {
    let run = &definition.run;
    if run.args.iter().filter(|part| part.contains(TARGET)).count() != 1 {
        return Err(format!(
            "{at}[run] args has to carry {TARGET} exactly once, where the directory being \
             counted goes"
        ));
    }
    let holders: Vec<&String> = run
        .languages
        .iter()
        .filter(|part| part.contains(EXTENSIONS) || part.contains(NAMES))
        .collect();
    if holders.len() != 1 || (holders[0].contains(EXTENSIONS) && holders[0].contains(NAMES)) {
        return Err(format!(
            "{at}[run] languages has to carry exactly one of {EXTENSIONS} or {NAMES}, where the \
             corpus's languages go"
        ));
    }
    if run.spells_by_name() && definition.language_names.is_empty() {
        return Err(format!(
            "{at}[run] languages says {NAMES} and there is no [language-names] table saying \
             what this counter calls each extension"
        ));
    }
    if !run.spells_by_name() && !definition.language_names.is_empty() {
        return Err(format!(
            "{at}has a [language-names] table and [run] languages never says {NAMES}, so it \
             would never be read"
        ));
    }
    if run.spells_by_name() && run.extension_case == ExtensionCase::Exact {
        return Err(format!(
            "{at}[run] says extension-case = \"exact\" and languages says {NAMES}, so no \
             extension is ever spelled and the setting would never be read"
        ));
    }
    Ok(())
}

fn check_acquisition(at: &str, acquisition: &Acquisition) -> Result<(), String> {
    let is_release_file = acquisition.channel == Channel::GithubReleaseFile;
    if is_release_file && acquisition.file.is_empty() {
        return Err(format!(
            "{at}github-release-file needs an [acquisition.file] table saying which release \
             file to take"
        ));
    }
    if !is_release_file && !acquisition.file.is_empty() {
        return Err(format!(
            "{at}an [acquisition.file] table belongs to github-release-file, and this is not"
        ));
    }
    for system in acquisition.file.keys() {
        if !RELEASE_FILE_SYSTEMS.contains(&system.as_str()) {
            return Err(format!(
                "{at}[acquisition.file] {system} is not a system a release file can be named \
                 for; it knows {}",
                RELEASE_FILE_SYSTEMS.join(", ")
            ));
        }
    }
    Ok(())
}

fn get_default_version_flag() -> String {
    DEFAULT_VERSION_FLAG.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHIPPED: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/counters");
    const LINUX: [&str; 7] = ["c", "h", "s", "py", "pl", "rs", "sh"];

    const SCC: &str = r#"
name         = "scc"
repository   = "https://github.com/boyter/scc"
version-flag = "--version"

[acquisition]
channel = "github-release-asset"
name    = "boyter/scc"
version = "4.0.0"

[run]
args           = ["{target}"]
json           = ["--format", "json"]
languages      = ["-i", "{extensions}"]
same-work      = ["-c", "--no-cocomo", "--no-config"]
same-work-note = "complexity and cost estimates off, no config file read"
scrub-env      = ["SCC_CONFIG_PATH"]

[read]
each     = "[]"
files    = "Count"
lines    = "Lines"
code     = "Code"
comments = "Comment"
blanks   = "Blank"
"#;

    const BY_NAME: &str = r#"
name = "namer"

[run]
args      = ["{target}"]
languages = ["-t", "{names}"]

[read]
files    = "files"
lines    = "lines"
code     = "code"
comments = "comments"
blanks   = "blanks"

[language-names]
c = "C"
h = "C Header"
"#;

    #[test]
    fn every_shipped_definition_parses_and_spells_the_linux_corpus() {
        let found = read_definitions(Path::new(SHIPPED)).expect("the shipped definitions");
        let names: Vec<&str> = found.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, ["cloc", "mezura", "scc", "tokei"]);
        for definition in &found {
            let spelled = definition
                .spell_languages(&build_strings(&LINUX))
                .expect(&definition.name);
            assert!(!spelled.is_empty(), "{}", definition.name);
            assert!(
                spelled.iter().all(|part| !part.contains('{')),
                "{}: {spelled:?}",
                definition.name
            );
        }
    }

    #[test]
    fn a_full_definition_reads_back_and_what_is_left_out_defaults() {
        let scc = parse(SCC, "scc").expect("scc");
        assert_eq!(
            scc.repository.as_deref(),
            Some("https://github.com/boyter/scc")
        );
        let acquisition = scc.acquisition.as_ref().expect("an acquisition");
        assert_eq!(
            (acquisition.channel, acquisition.version.as_str()),
            (Channel::GithubReleaseAsset, "4.0.0")
        );
        assert_eq!(scc.run.same_work, ["-c", "--no-cocomo", "--no-config"]);
        assert_eq!(scc.run.scrub_env, ["SCC_CONFIG_PATH"]);
        assert_eq!(
            scc.read
                .as_ref()
                .and_then(|read| read.get("blanks"))
                .map(String::as_str),
            Some("Blank")
        );

        let namer = parse(BY_NAME, "namer").expect("namer");
        assert_eq!(namer.version_flag, "--version");
        assert!(namer.repository.is_none() && namer.acquisition.is_none());
        let elsewhere = parse_definition_for(SCC, Path::new("mezura/.linebench/dev.toml"), "scc");
        assert!(elsewhere.is_ok(), "{elsewhere:?}");
        let refusal = parse_definition_for(SCC, Path::new("dev.toml"), "tokei").unwrap_err();
        assert!(refusal.contains("meant to be tokei"), "{refusal}");
        assert!(namer.run.json.is_empty() && namer.run.same_work.is_empty());
        assert_eq!(namer.run.same_work_note, "");
    }

    #[test]
    fn a_misspelled_key_is_refused_so_no_flag_is_silently_dropped() {
        assert_refused(
            &SCC.replace("scrub-env", "scrub_env"),
            "scc",
            "unknown field `scrub_env`",
        );
    }

    #[test]
    fn the_run_block_and_the_acquisition_are_checked_beyond_their_shape() {
        assert_refused(SCC, "other", "meant to be other");
        assert_refused(
            &SCC.replace(r#"args           = ["{target}"]"#, r#"args = ["--stats"]"#),
            "scc",
            "{target} exactly once",
        );
        assert_refused(
            &SCC.replace(r#"["-i", "{extensions}"]"#, r#"["-i"]"#),
            "scc",
            "exactly one of {extensions} or {names}",
        );
        assert_refused(
            &SCC.replace(
                r#"["-i", "{extensions}"]"#,
                r#"["{extensions}", "{names}"]"#,
            ),
            "scc",
            "exactly one of {extensions} or {names}",
        );
        assert_refused(
            BY_NAME.split("[language-names]").next().unwrap(),
            "namer",
            "no [language-names] table",
        );
        assert_refused(
            &BY_NAME.replace("{names}", "{extensions}"),
            "namer",
            "would never be read",
        );
        assert_refused(
            &SCC.replace("github-release-asset", "github-release-file"),
            "scc",
            "needs an [acquisition.file] table",
        );
        assert_refused(
            &SCC.replace(
                "version = \"4.0.0\"",
                "version = \"4.0.0\"\n[acquisition.file]\nother = \"scc\"",
            ),
            "scc",
            "belongs to github-release-file",
        );
        let named_for_bsd = SCC
            .replace("github-release-asset", "github-release-file")
            .replace(
                "version = \"4.0.0\"",
                "version = \"4.0.0\"\n[acquisition.file]\nbsd = \"scc\"",
            );
        assert_refused(&named_for_bsd, "scc", "bsd is not a system");
    }

    #[test]
    fn how_the_counts_are_read_is_declared_exactly_one_way() {
        assert_refused(
            &SCC.replace("comments = \"Comment\"\n", ""),
            "scc",
            "no path for comments",
        );
        assert_refused(
            &SCC.replace("blanks   = \"Blank\"\n", ""),
            "scc",
            "no third bucket",
        );
        assert_refused(
            &SCC.replace(
                "name         = \"scc\"",
                "name = \"scc\"\noutput = \"tokei-json\"",
            ),
            "scc",
            "one or the other",
        );
        assert_refused(
            SCC.split("[read]").next().unwrap(),
            "scc",
            "nothing about how to read",
        );
    }

    #[test]
    fn languages_are_spelled_by_extension_or_by_the_counter_s_own_names() {
        let scc = parse(SCC, "scc").unwrap();
        assert_eq!(
            scc.spell_languages(&build_strings(&["c", "h"])).unwrap(),
            ["-i", "c,h"]
        );
        let namer = parse(BY_NAME, "namer").unwrap();
        assert_eq!(
            namer.spell_languages(&build_strings(&["c", "H"])).unwrap(),
            ["-t", "C,C Header"]
        );
        let refusal = namer
            .spell_languages(&build_strings(&["c", "go", "rs"]))
            .unwrap_err();
        assert!(
            refusal.contains("namer does not say how it names .go, .rs"),
            "{refusal}"
        );
        let inline = parse(
            &SCC.replace(
                r#"["-i", "{extensions}"]"#,
                r#"["--include-ext={extensions}"]"#,
            ),
            "scc",
        )
        .unwrap();
        assert_eq!(
            inline.spell_languages(&build_strings(&["c", "h"])).unwrap(),
            ["--include-ext=c,h"]
        );
        let exact = parse(
            &SCC.replace(
                r#"["-i", "{extensions}"]"#,
                r#"["-i", "{extensions}"]
extension-case = "exact""#,
            ),
            "scc",
        )
        .unwrap();
        assert_eq!(
            exact
                .spell_languages(&build_strings(&["S", "py", "Rmd", "7z"]))
                .unwrap(),
            ["-i", "S,s,py,PY,Rmd,rmd,RMD,7z,7Z"]
        );
        let unread = parse(
            &BY_NAME.replace("[run]", "[run]\nextension-case = \"exact\""),
            "namer",
        )
        .unwrap_err();
        assert!(unread.contains("would never be read"), "{unread}");
    }

    #[test]
    fn the_timed_command_is_target_then_languages_then_same_work_then_json() {
        let scc = parse(SCC, "scc").unwrap();
        let tree = Path::new("/tree");
        let c = build_strings(&["c"]);
        assert_eq!(scc.build_args(tree, &c, false, false).unwrap(), ["/tree"]);
        assert_eq!(
            scc.build_args(tree, &c, false, true).unwrap(),
            ["/tree", "--format", "json"]
        );
        assert_eq!(
            scc.build_args(tree, &c, true, true).unwrap(),
            [
                "/tree",
                "-i",
                "c",
                "-c",
                "--no-cocomo",
                "--no-config",
                "--format",
                "json"
            ]
        );
    }

    #[test]
    fn the_binary_and_the_process_are_named_per_system() {
        let scc = parse(SCC, "scc").unwrap();
        assert_eq!(scc.get_binary_name("windows").unwrap(), "scc.exe");
        assert_eq!(scc.get_binary_name("linux").unwrap(), "scc");
        assert_eq!(scc.get_process_name("windows").unwrap(), "scc.exe");

        let release_file = SCC
            .replace("github-release-asset", "github-release-file")
            .replace(
                "version = \"4.0.0\"",
                "version = \"4.0.0\"\n[acquisition.file]\nwindows = \"scc-{version}.exe\"\n\
                 other = \"scc-{version}.pl\"",
            );
        let cloc_like = parse(&release_file, "scc").unwrap();
        assert_eq!(
            cloc_like.get_release_file_name("windows").unwrap(),
            "scc-4.0.0.exe"
        );
        assert_eq!(cloc_like.get_binary_name("windows").unwrap(), "scc.exe");
        assert_eq!(cloc_like.get_binary_name("macos").unwrap(), "scc.pl");
        assert!(scc.get_release_file_name("windows").is_err());
        for bare in ["scc-{version}", "scc-{version}-linux"] {
            let unsuffixed = parse(
                &release_file
                    .replace(
                        "other = \"scc-{version}.pl\"",
                        &format!("other = \"{bare}\""),
                    )
                    .replace(
                        "windows = \"scc-{version}.exe\"",
                        &format!("windows = \"{bare}\""),
                    ),
                "scc",
            )
            .unwrap();
            assert_eq!(unsuffixed.get_binary_name("linux").unwrap(), "scc");
            assert_eq!(unsuffixed.get_binary_name("windows").unwrap(), "scc.exe");
        }

        let windows_only = parse(&release_file.replace("other = ", "windows2 = "), "scc");
        assert!(windows_only.is_err());
        let windows_only = parse(
            &release_file.replace("other = \"scc-{version}.pl\"", ""),
            "scc",
        )
        .unwrap();
        let refusal = windows_only.get_binary_name("linux").unwrap_err();
        assert!(
            refusal.contains("names no release file for linux, only for windows"),
            "{refusal}"
        );
        assert_eq!(cloc_like.get_process_name("macos").unwrap(), "scc.pl");
    }

    fn parse(text: &str, name: &str) -> Result<Definition, String> {
        parse_definition(text, Path::new(&format!("{name}.toml")))
    }

    fn assert_refused(text: &str, name: &str, expected: &str) {
        let refusal = parse(text, name).expect_err("a definition that should have been refused");
        assert!(
            refusal.contains(expected),
            "expected {expected:?} in {refusal:?}"
        );
    }

    fn build_strings(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|part| part.to_string()).collect()
    }
}
