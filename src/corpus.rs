use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Deserializer, Serialize};

use crate::files::{parse_toml, read_text};
use crate::measure::print_line;
use crate::os::{capture_output, capture_with_status};

pub const DEFAULT_TOLERANCE: f64 = 0.01;
pub const NOTHING_COMPARED_NO_COUNTS: &str =
    "nothing compared: no instance counted both files and lines";
pub const NOTHING_COMPARED_ALONE: &str =
    "nothing compared: one instance with counts and no declared file count to hold it against";
const CORPUS_SUFFIX: &str = "toml";
const AD_HOC_CORPUS: &str = "tree";
const SKIP_SYSTEMS: [&str; 3] = ["windows", "linux", "macos"];
const GIT_DIR: &str = ".git";
const FULL_COMMIT: usize = 40;
const SHORT_HASH: usize = 9;

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Corpus {
    pub name: String,
    #[serde(skip)]
    pub path: PathBuf,
    #[serde(default)]
    pub remote: String,
    #[serde(default)]
    pub commit: String,
    #[serde(default)]
    pub files: Option<u64>,
    pub extensions: Vec<String>,
    #[serde(
        default = "get_default_tolerance",
        deserialize_with = "parse_tolerance"
    )]
    pub tolerance: f64,
    #[serde(default)]
    pub skip: BTreeMap<String, Vec<String>>,
}

impl Corpus {
    pub fn is_pinned(&self) -> bool {
        !self.commit.is_empty()
    }

    pub fn skips(&self, system: &str, counter: &str) -> bool {
        self.skip
            .get(system)
            .is_some_and(|names| names.iter().any(|name| name == counter))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitState {
    pub head: Option<String>,
    pub clean: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Parity {
    pub reference_files: Option<u64>,
    pub tolerance: f64,
    pub problems: Vec<String>,
    #[serde(default)]
    pub missing: Vec<String>,
    #[serde(default)]
    pub counted: usize,
    #[serde(default = "get_compared_default")]
    pub compared: bool,
}

impl Parity {
    pub fn judge(&self) -> Verdict<'_> {
        let nothing_compared = (!self.compared).then(|| self.explain_nothing_compared());
        if !self.problems.is_empty() {
            Verdict::NotEqual {
                problems: &self.problems,
                nothing_compared,
            }
        } else if let Some(why) = nothing_compared {
            Verdict::NotCompared(why)
        } else {
            Verdict::Equal {
                reference: self.reference_files.filter(|files| *files > 0),
                instances: self.counted,
            }
        }
    }

    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        let verdict = self.judge();
        match verdict {
            Verdict::NotEqual {
                problems,
                nothing_compared,
            } => {
                parts.push(format!("not equal: {}", problems.join("; ")));
                parts.extend(nothing_compared.map(str::to_string));
            }
            Verdict::NotCompared(why) => parts.push(why.to_string()),
            Verdict::Equal { .. } => parts.extend(verdict.describe_equality(self.tolerance)),
        }
        if !self.missing.is_empty() {
            parts.push(format!(
                "{} gave no readable counts",
                self.missing.join(", ")
            ));
        }
        parts.join("; ")
    }

    fn explain_nothing_compared(&self) -> &'static str {
        if self.counted == 0 {
            NOTHING_COMPARED_NO_COUNTS
        } else {
            NOTHING_COMPARED_ALONE
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict<'a> {
    NotEqual {
        problems: &'a [String],
        nothing_compared: Option<&'static str>,
    },
    NotCompared(&'static str),
    Equal {
        reference: Option<u64>,
        instances: usize,
    },
}

impl Verdict<'_> {
    pub fn describe_equality(self, tolerance: f64) -> Option<String> {
        let tolerance = format_percent(tolerance);
        match self {
            Verdict::Equal {
                reference: Some(_),
                instances: 1,
            } => Some(format!(
                "within {tolerance} of the corpus, one instance so no line comparison"
            )),
            Verdict::Equal {
                reference: Some(_), ..
            } => Some(format!("within {tolerance} of the corpus")),
            Verdict::Equal {
                reference: None, ..
            } => Some(format!("within {tolerance} of each other")),
            Verdict::NotEqual { .. } | Verdict::NotCompared(_) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Counted {
    pub instance: String,
    pub files: u64,
    pub lines: u64,
}

pub fn read_corpora(dir: &Path) -> Result<Vec<Corpus>, String> {
    let entries = fs::read_dir(dir)
        .map_err(|error| format!("no corpus definitions at {}: {error}", dir.display()))?;
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == CORPUS_SUFFIX))
        .filter(|path| path.is_file())
        .collect();
    paths.sort();
    paths.iter().map(|path| read_corpus(path)).collect()
}

pub fn read_corpus(path: &Path) -> Result<Corpus, String> {
    parse_corpus(&read_text(path)?, path)
}

pub fn parse_corpus(text: &str, path: &Path) -> Result<Corpus, String> {
    let mut corpus: Corpus = parse_toml(text, path)?;
    corpus.path = path.to_path_buf();
    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy())
        .unwrap_or_default();
    if corpus.name != stem {
        return Err(format!(
            "{}: name = \"{}\" and the file is named {stem}; the two have to agree",
            path.display(),
            corpus.name
        ));
    }
    if corpus.extensions.is_empty() {
        return Err(format!(
            "{}: extensions is empty, so nothing would be counted",
            path.display()
        ));
    }
    let is_a_full_hash = corpus.commit.len() == FULL_COMMIT
        && corpus
            .commit
            .chars()
            .all(|c| c.is_ascii_digit() || (c.is_ascii_lowercase() && c.is_ascii_hexdigit()));
    if corpus.is_pinned() && !is_a_full_hash {
        return Err(format!(
            "{}: commit = \"{}\" has to be the full {FULL_COMMIT}-character hash in lower case, \
             as git rev-parse prints it",
            path.display(),
            corpus.commit
        ));
    }
    if corpus.files == Some(0) {
        return Err(format!(
            "{}: files = 0 would hold every count against nothing",
            path.display()
        ));
    }
    if let Some(system) = corpus
        .skip
        .keys()
        .find(|system| !SKIP_SYSTEMS.contains(&system.as_str()))
    {
        return Err(format!(
            "{}: [skip] names the system {system}; the systems are {}, and WSL counts as linux",
            path.display(),
            SKIP_SYSTEMS.join(", ")
        ));
    }
    Ok(corpus)
}

pub fn build_corpus_of(checkout: &Path, extensions: &[String]) -> Result<Corpus, String> {
    let name = fs::canonicalize(checkout)
        .ok()
        .as_deref()
        .unwrap_or(checkout)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| AD_HOC_CORPUS.to_string());
    let extensions: Vec<String> = extensions
        .iter()
        .map(|extension| extension.trim_start_matches('.').to_string())
        .collect();
    if extensions.iter().any(String::is_empty) {
        return Err("--extensions holds an empty extension, and nothing carries that".to_string());
    }
    Ok(Corpus {
        name,
        path: checkout.to_path_buf(),
        remote: String::new(),
        commit: String::new(),
        files: None,
        extensions,
        tolerance: DEFAULT_TOLERANCE,
        skip: BTreeMap::new(),
    })
}

pub fn read_git_state(checkout: &Path) -> GitState {
    let shown = checkout.to_string_lossy();
    let head = read_head(checkout);
    let clean = match capture_with_status("git", &["-C", &shown, "status", "--porcelain"]) {
        Ok((true, out)) => Some(is_clean(&out)),
        _ => None,
    };
    GitState { head, clean }
}

pub fn check_commit(corpus: &Corpus, checkout: &Path) -> Result<(), String> {
    if !checkout.is_dir() {
        return Err(format!(
            "{} is not there: run setup to fetch {}",
            checkout.display(),
            corpus.name
        ));
    }
    if !corpus.is_pinned() {
        return Ok(());
    }
    match read_head(checkout).as_deref() {
        Some(head) if head == corpus.commit => Ok(()),
        Some(head) => Err(format!(
            "{} is at {} and {} pins {}: run setup, or check that commit out",
            checkout.display(),
            shorten_hash(head),
            corpus.name,
            shorten_hash(&corpus.commit)
        )),
        None => Err(format!(
            "{} is not a git checkout and {} pins {}: run setup",
            checkout.display(),
            corpus.name,
            shorten_hash(&corpus.commit)
        )),
    }
}

pub fn check_declares_files(corpus: &Corpus) -> Result<(), String> {
    if corpus.is_pinned() && corpus.files.is_none() {
        return Err(format!(
            "{}: commit is set and files is not: run check over the checkout at that commit and \
             write in the files = line it prints",
            corpus.path.display()
        ));
    }
    Ok(())
}

pub fn count_tracked_files(checkout: &Path, extensions: &[String]) -> Result<u64, String> {
    let shown = checkout.to_string_lossy();
    let listing = match capture_with_status(
        "git",
        &["-C", &shown, "ls-tree", "-r", "-z", "--name-only", "HEAD"],
    ) {
        Ok((true, listing)) => listing,
        _ => {
            return Err(format!(
                "git could not list the files committed at HEAD in {shown}, and the reference \
                 count comes from that listing"
            ));
        }
    };
    let carries_one = |path: &str| {
        Path::new(path)
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|found| {
                extensions
                    .iter()
                    .any(|wanted| wanted.eq_ignore_ascii_case(found))
            })
    };
    Ok(listing
        .split('\0')
        .filter(|path| !path.is_empty() && carries_one(path))
        .count() as u64)
}

pub fn setup_corpus(out: &mut dyn Write, corpus: &Corpus, checkout: &Path) -> Result<(), String> {
    let shown = checkout.to_string_lossy().into_owned();
    let head = if checkout.is_dir() {
        read_head(checkout)
    } else {
        None
    };
    if corpus.is_pinned() && head.as_deref() == Some(corpus.commit.as_str()) {
        return print_line(
            out,
            &format!(
                "  corpus already pinned at {}",
                shorten_hash(&corpus.commit)
            ),
        );
    }
    let is_checkout = checkout.join(GIT_DIR).exists() && head.is_some();
    if !corpus.is_pinned() && (is_checkout || (checkout.is_dir() && corpus.remote.is_empty())) {
        return print_line(
            out,
            &format!("  {} is taken as it stands at {shown}", corpus.name),
        );
    }
    if corpus.remote.is_empty() {
        if !checkout.is_dir() {
            return Err(format!(
                "{} names no remote to fetch from, so {shown} has to be there already",
                corpus.name
            ));
        }
        return Err(format!(
            "{shown}\nis at {}, and {} pins {}.\nThere is no remote to fetch it from, so put \
             the checkout on that commit yourself, or clear the commit from the definition.",
            head.as_deref()
                .map_or("no commit at all".to_string(), shorten_hash),
            corpus.name,
            shorten_hash(&corpus.commit)
        ));
    }
    if !corpus.is_pinned() && !is_checkout && holds_anything_but_git(checkout) {
        return Err(format!(
            "{shown}\nalready holds files and is not a git checkout, so the setup will not \
             write over them. Empty it, point the definition elsewhere, or clear its remote to \
             measure it as it stands."
        ));
    }
    fs::create_dir_all(checkout)
        .map_err(|error| format!("{shown} could not be created: {error}"))?;
    if !checkout.join(GIT_DIR).exists() {
        run_git(&["init", "-q", &shown])?;
        run_git(&["-C", &shown, "remote", "add", "origin", &corpus.remote])?;
    }
    if corpus.is_pinned() {
        print_line(
            out,
            &format!(
                "  fetching {} from {}",
                shorten_hash(&corpus.commit),
                corpus.remote
            ),
        )?;
    } else {
        print_line(
            out,
            &format!("  cloning the default branch of {}", corpus.remote),
        )?;
    }
    let wanted = if corpus.is_pinned() {
        corpus.commit.as_str()
    } else {
        "HEAD"
    };
    run_git(&["-C", &shown, "fetch", "--depth", "1", "origin", wanted])?;
    run_git(&["-C", &shown, "checkout", "-q", "FETCH_HEAD"])
}

pub fn judge_parity(
    reference_files: Option<u64>,
    counted: &[Counted],
    expected: &[String],
    tolerance: f64,
) -> Parity {
    let missing: Vec<String> = expected
        .iter()
        .filter(|name| !counted.iter().any(|count| count.instance == **name))
        .cloned()
        .collect();
    let mut problems: Vec<String> = counted
        .iter()
        .filter_map(|count| {
            describe_empty_count(count.files, count.lines)
                .map(|what| format!("{} {what}", count.instance))
        })
        .collect();
    let with_counts: Vec<Counted> = counted
        .iter()
        .filter(|count| count.files > 0 && count.lines > 0)
        .cloned()
        .collect();
    let compared = match reference_files {
        Some(reference) => {
            for count in &with_counts {
                let deviation = count.files as f64 / reference as f64 - 1.0;
                if deviation.abs() > tolerance {
                    let direction = if deviation < 0.0 { "under" } else { "over" };
                    problems.push(format!(
                        "{} files {} {direction} the corpus ({} against {reference} declared)",
                        count.instance,
                        format_percent(deviation.abs()),
                        count.files
                    ));
                }
            }
            !with_counts.is_empty()
        }
        None if with_counts.len() < 2 => false,
        None => {
            if let Some(problem) = describe_spread("files", &with_counts, |c| c.files, tolerance) {
                problems.push(problem);
            }
            true
        }
    };
    if with_counts.len() >= 2
        && let Some(problem) = describe_spread("lines", &with_counts, |c| c.lines, tolerance)
    {
        problems.push(problem);
    }
    Parity {
        reference_files,
        tolerance,
        problems,
        missing,
        counted: with_counts.len(),
        compared,
    }
}

pub fn describe_empty_count(files: u64, lines: u64) -> Option<String> {
    match (files, lines) {
        (0, 0) => Some("counted nothing".to_string()),
        (0, _) => Some(format!("counted no files and {lines} lines")),
        (_, 0) => Some(format!("counted {files} files and no lines")),
        _ => None,
    }
}

pub fn format_percent(fraction: f64) -> String {
    format!("{:.1}%", fraction * 100.0)
}

pub fn shorten_hash(hash: &str) -> String {
    hash.chars().take(SHORT_HASH).collect()
}

fn describe_spread(
    what: &str,
    counted: &[Counted],
    pick: impl Fn(&Counted) -> u64,
    tolerance: f64,
) -> Option<String> {
    let least = counted.iter().min_by_key(|c| pick(c))?;
    let most = counted.iter().max_by_key(|c| pick(c))?;
    let (low, high) = (pick(least), pick(most));
    let spread = high as f64 / low as f64 - 1.0;
    (spread > tolerance).then(|| {
        format!(
            "{what} spread {} ({} {low} to {} {high})",
            format_percent(spread),
            least.instance,
            most.instance
        )
    })
}

fn read_head(checkout: &Path) -> Option<String> {
    capture_output(
        "git",
        &["-C", &checkout.to_string_lossy(), "rev-parse", "HEAD"],
    )
}

fn is_clean(porcelain: &str) -> bool {
    porcelain.lines().all(|line| line.trim().is_empty())
}

fn get_compared_default() -> bool {
    true
}

fn holds_anything_but_git(dir: &Path) -> bool {
    fs::read_dir(dir)
        .is_ok_and(|entries| entries.flatten().any(|entry| entry.file_name() != GIT_DIR))
}

fn run_git(args: &[&str]) -> Result<(), String> {
    let ran = Command::new("git")
        .args(args)
        .status()
        .map_err(|error| format!("git could not be run: {error}"))?;
    if ran.success() {
        return Ok(());
    }
    Err(format!(
        "this failed, and the corpus cannot be set up without it:\n  git {}",
        args.join(" ")
    ))
}

fn get_default_tolerance() -> f64 {
    DEFAULT_TOLERANCE
}

fn parse_tolerance<'de, D: Deserializer<'de>>(deserializer: D) -> Result<f64, D::Error> {
    let text = String::deserialize(deserializer)?;
    let number = text.trim().strip_suffix('%').ok_or_else(|| {
        serde::de::Error::custom(format!(
            "tolerance = \"{text}\" has to be a percentage, such as \"1%\""
        ))
    })?;
    let percent: f64 = number
        .trim()
        .parse()
        .map_err(|_| serde::de::Error::custom(format!("tolerance = \"{text}\" is not a number")))?;
    if !(0.0..=100.0).contains(&percent) {
        return Err(serde::de::Error::custom(format!(
            "tolerance = \"{text}\" is outside 0% to 100%"
        )));
    }
    Ok(percent / 100.0)
}

#[cfg(test)]
mod tests {
    use std::env;

    use super::*;

    const SHIPPED: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/corpora");

    #[test]
    fn the_tracked_files_carrying_the_extensions_are_counted_whatever_their_case() {
        let dir = env::temp_dir().join("linebench-the_tracked_files_are_counted");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("sub")).unwrap();
        let shown = dir.to_string_lossy().into_owned();
        run_git(&["init", "-q", &shown]).unwrap();
        for name in ["a.rs", "sub/B.RS", "c.txt", "d"] {
            fs::write(dir.join(name), "x").unwrap();
        }
        run_git(&["-C", &shown, "add", "-A"]).unwrap();
        run_git(&[
            "-C",
            &shown,
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-qm",
            "i",
        ])
        .unwrap();
        fs::write(dir.join("e.rs"), "x").unwrap();
        fs::write(dir.join("staged.rs"), "x").unwrap();
        run_git(&["-C", &shown, "add", "staged.rs"]).unwrap();
        let rs = ["rs".to_string()];
        let both = ["txt".to_string(), "rs".to_string()];
        assert_eq!(count_tracked_files(&dir, &rs).unwrap(), 2);
        assert_eq!(count_tracked_files(&dir, &both).unwrap(), 3);
        fs::remove_dir_all(&dir).unwrap();
        assert!(count_tracked_files(&dir, &rs).is_err());
    }

    #[test]
    fn a_corpus_leaves_a_counter_out_per_system_and_names_only_systems_that_exist() {
        let corpus = parse_corpus(
            "name = \"t\"\nextensions = [\"c\"]\n[skip]\nwindows = [\"cloc\"]\n",
            Path::new("t.toml"),
        )
        .unwrap();
        assert!(corpus.skips("windows", "cloc"));
        assert!(!corpus.skips("linux", "cloc") && !corpus.skips("windows", "scc"));
        let refused = parse_corpus(
            "name = \"t\"\nextensions = [\"c\"]\n[skip]\nwsl = [\"cloc\"]\n",
            Path::new("t.toml"),
        )
        .unwrap_err();
        assert!(refused.contains("WSL counts as linux"), "{refused}");
    }

    #[test]
    fn the_shipped_corpora_parse_and_the_tolerance_is_a_percentage() {
        let corpora = read_corpora(Path::new(SHIPPED)).unwrap();
        let names: Vec<&str> = corpora.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["linebench", "linux"]);
        let linux = corpora.iter().find(|c| c.name == "linux").unwrap();
        assert!(linux.is_pinned() && !corpora[0].is_pinned());
        assert_eq!(linux.tolerance, 0.01);
        assert_eq!(linux.files, Some(63765));
        assert_eq!(corpora[0].files, None);
        let pinned = "name = \"t\"\nextensions = [\"c\"]\ncommit = \"0000000000000000000000000000000000000000\"\n";
        let undeclared = parse_corpus(pinned, Path::new("t.toml")).unwrap();
        assert!(
            check_declares_files(&undeclared)
                .unwrap_err()
                .contains("commit is set and files is not")
        );
        assert!(check_declares_files(linux).is_ok());
        assert!(
            parse_corpus(
                "name = \"t\"\nextensions = [\"c\"]\nfiles = 0\n",
                Path::new("t.toml")
            )
            .unwrap_err()
            .contains("files = 0")
        );

        let bare = "name = \"t\"\nextensions = [\"c\"]\n";
        assert_eq!(
            parse_corpus(bare, Path::new("t.toml")).unwrap().tolerance,
            DEFAULT_TOLERANCE
        );
        let half = "name = \"t\"\nextensions = [\"c\"]\ntolerance = \"0.5%\"\n";
        assert_eq!(
            parse_corpus(half, Path::new("t.toml")).unwrap().tolerance,
            0.005
        );
        let plain = "name = \"t\"\nextensions = [\"c\"]\ntolerance = \"1\"\n";
        assert!(
            parse_corpus(plain, Path::new("t.toml"))
                .unwrap_err()
                .contains("percentage")
        );
        assert!(
            parse_corpus(bare, Path::new("other.toml"))
                .unwrap_err()
                .contains("named other")
        );
        assert!(
            parse_corpus("name = \"t\"\nextensions = []\n", Path::new("t.toml"))
                .unwrap_err()
                .contains("nothing would be counted")
        );
    }

    #[test]
    fn parity_names_who_is_off_and_by_how_much() {
        let counted = [
            count("mezura", 61234, 36_100_000),
            count("scc", 61230, 36_100_000),
            count("tokei", 61234, 36_120_000),
            count("cloc", 57900, 34_000_000),
        ];
        let expected = ["cloc", "mezura", "scc", "tokei"].map(String::from);
        let parity = judge_parity(Some(61234), &counted, &expected, 0.01);
        assert_eq!(
            parity.problems,
            [
                "cloc files 5.4% under the corpus (57900 against 61234 declared)",
                "lines spread 6.2% (cloc 34000000 to tokei 36120000)"
            ]
        );
        assert!(parity.compared && parity.missing.is_empty());
        assert_eq!(parity.counted, 4);
        let unread = judge_parity(Some(61234), &counted[..3], &expected, 0.01);
        assert!(unread.problems.is_empty() && unread.compared);
        assert_eq!(unread.missing, ["cloc"]);
        assert_eq!(
            unread.describe(),
            "within 1.0% of the corpus; cloc gave no readable counts"
        );
        let unreadable = judge_parity(Some(61234), &[], &expected, 0.01);
        assert!(unreadable.problems.is_empty() && !unreadable.compared);
        assert_eq!(
            unreadable.judge(),
            Verdict::NotCompared(NOTHING_COMPARED_NO_COUNTS)
        );

        let two = [
            count("mezura", 61234, 30_000_000),
            count("scc", 61234, 40_000_000),
        ];
        let parity = judge_parity(None, &two, &expected[1..3], 0.01);
        assert_eq!(
            parity.problems,
            ["lines spread 33.3% (mezura 30000000 to scc 40000000)"]
        );
        assert!(parity.compared);
        let alone = judge_parity(None, &two[..1], &expected[1..2], 0.01);
        assert!(alone.problems.is_empty() && alone.missing.is_empty() && !alone.compared);
        assert_eq!(alone.describe(), NOTHING_COMPARED_ALONE);
        let against_git = judge_parity(Some(61234), &two[..1], &expected[1..2], 0.01);
        assert!(against_git.compared);
        assert_eq!(
            against_git.describe(),
            "within 1.0% of the corpus, one instance so no line comparison"
        );

        let nothing = [count("mezura", 0, 0), count("scc", 61234, 30_000_000)];
        let one_empty = judge_parity(None, &nothing, &expected[1..3], 0.01);
        assert_eq!(one_empty.problems, ["mezura counted nothing"]);
        assert!(!one_empty.compared);
        assert_eq!(
            one_empty.describe(),
            format!("not equal: mezura counted nothing; {NOTHING_COMPARED_ALONE}")
        );
        let hollow = [count("mezura", 812, 0)];
        assert_eq!(
            judge_parity(Some(812), &hollow, &expected[1..2], 0.01).problems,
            ["mezura counted 812 files and no lines"]
        );
        let alike = [
            count("mezura", 61234, 30_000_000),
            count("scc", 61234, 30_000_000),
        ];
        let two_against_the_corpus = judge_parity(Some(61234), &alike, &expected[1..3], 0.01);
        assert_eq!(
            two_against_the_corpus.describe(),
            "within 1.0% of the corpus"
        );
        assert_eq!(
            judge_parity(None, &alike, &expected[1..3], 0.01).describe(),
            "within 1.0% of each other"
        );
    }

    #[test]
    fn an_empty_count_is_described_by_which_half_is_missing() {
        assert_eq!(
            describe_empty_count(0, 0).as_deref(),
            Some("counted nothing")
        );
        assert_eq!(
            describe_empty_count(0, 8635).as_deref(),
            Some("counted no files and 8635 lines")
        );
        assert_eq!(
            describe_empty_count(812, 0).as_deref(),
            Some("counted 812 files and no lines")
        );
        assert_eq!(describe_empty_count(812, 8635), None);
    }

    #[test]
    fn a_porcelain_status_is_clean_only_when_it_says_nothing() {
        assert!(is_clean("") && is_clean("\n  \n"));
        assert!(!is_clean(" M src/a.rs\n"));
        assert!(!is_clean("?? new.txt\n"));
    }

    fn count(instance: &str, files: u64, lines: u64) -> Counted {
        Counted {
            instance: instance.to_string(),
            files,
            lines,
        }
    }
}
