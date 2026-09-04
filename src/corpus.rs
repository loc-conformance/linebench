use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Deserializer, Serialize};

use crate::os::{capture_output, capture_with_status};

pub const DEFAULT_TOLERANCE: f64 = 0.01;
const CORPUS_SUFFIX: &str = "toml";
const GIT_DIR: &str = ".git";
const SHORT_COMMIT: usize = 9;

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
    pub extensions: Vec<String>,
    #[serde(
        default = "get_default_tolerance",
        deserialize_with = "parse_tolerance"
    )]
    pub tolerance: f64,
}

impl Corpus {
    pub fn is_pinned(&self) -> bool {
        !self.commit.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitState {
    pub head: Option<String>,
    pub clean: Option<bool>,
    pub dirty: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Parity {
    pub reference_files: Option<u64>,
    pub tolerance: f64,
    pub problems: Vec<String>,
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
    let text = fs::read_to_string(path)
        .map_err(|error| format!("{}: could not be read: {error}", path.display()))?;
    parse_corpus(&text, path)
}

pub fn parse_corpus(text: &str, path: &Path) -> Result<Corpus, String> {
    let mut corpus: Corpus =
        toml::from_str(text).map_err(|error| format!("{}: {error}", path.display()))?;
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
    let looks_like_a_hash = (7..=40).contains(&corpus.commit.len())
        && corpus.commit.chars().all(|c| c.is_ascii_hexdigit());
    if corpus.is_pinned() && !looks_like_a_hash {
        return Err(format!(
            "{}: commit = \"{}\" is not a commit hash",
            path.display(),
            corpus.commit
        ));
    }
    Ok(corpus)
}

pub fn read_git_state(checkout: &Path, with_untracked: bool) -> GitState {
    let shown = checkout.to_string_lossy();
    let head = capture_output("git", &["-C", &shown, "rev-parse", "HEAD"]);
    let mut args = vec!["-C", &shown, "status", "--porcelain"];
    if !with_untracked {
        args.push("-uno");
    }
    match capture_with_status("git", &args) {
        Some((true, out)) => {
            let dirty = parse_porcelain(&out);
            GitState {
                head,
                clean: Some(dirty.is_empty()),
                dirty,
            }
        }
        _ => GitState {
            head,
            clean: None,
            dirty: Vec::new(),
        },
    }
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
    let state = read_git_state(checkout, false);
    match state.head.as_deref() {
        Some(head) if head == corpus.commit => Ok(()),
        Some(head) => Err(format!(
            "{} is at {} and {} pins {}: run setup, or check that commit out",
            checkout.display(),
            shorten_commit(head),
            corpus.name,
            shorten_commit(&corpus.commit)
        )),
        None => Err(format!(
            "{} is not a git checkout and {} pins {}: run setup",
            checkout.display(),
            corpus.name,
            shorten_commit(&corpus.commit)
        )),
    }
}

pub fn setup_corpus(out: &mut dyn Write, corpus: &Corpus, checkout: &Path) -> Result<(), String> {
    let shown = checkout.to_string_lossy().into_owned();
    let head = if checkout.is_dir() {
        capture_output("git", &["-C", &shown, "rev-parse", "HEAD"])
    } else {
        None
    };
    if corpus.is_pinned() && head.as_deref() == Some(corpus.commit.as_str()) {
        return print_line(
            out,
            &format!(
                "corpus already pinned at {}",
                shorten_commit(&corpus.commit)
            ),
        );
    }
    let is_checkout = checkout.join(GIT_DIR).exists() && head.is_some();
    if !corpus.is_pinned() && (is_checkout || (checkout.is_dir() && corpus.remote.is_empty())) {
        return print_line(
            out,
            &format!("{} is taken as it stands at {shown}", corpus.name),
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
                .map_or("no commit at all".to_string(), shorten_commit),
            corpus.name,
            shorten_commit(&corpus.commit)
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
                "fetching {} from {}",
                shorten_commit(&corpus.commit),
                corpus.remote
            ),
        )?;
    } else {
        print_line(
            out,
            &format!("cloning the default branch of {}", corpus.remote),
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

pub fn count_reference_files(checkout: &Path, extensions: &[String]) -> Option<u64> {
    let shown = checkout.to_string_lossy();
    let (ok, listing) = capture_with_status("git", &["-C", &shown, "ls-files", "-z"])?;
    if !ok {
        return None;
    }
    Some(count_files_with(&listing, extensions)).filter(|files| *files > 0)
}

pub fn count_files_with(nul_separated_listing: &str, extensions: &[String]) -> u64 {
    nul_separated_listing
        .split('\0')
        .filter(|entry| !entry.is_empty())
        .filter(|entry| {
            Path::new(entry)
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| {
                    extensions
                        .iter()
                        .any(|wanted| wanted.eq_ignore_ascii_case(ext))
                })
        })
        .count() as u64
}

pub fn judge_parity(reference_files: Option<u64>, counted: &[Counted], tolerance: f64) -> Parity {
    let mut problems = Vec::new();
    match reference_files {
        Some(reference) if reference > 0 => {
            for count in counted {
                let deviation = count.files as f64 / reference as f64 - 1.0;
                if deviation.abs() > tolerance {
                    let direction = if deviation < 0.0 { "under" } else { "over" };
                    problems.push(format!(
                        "{} files {} {direction} the corpus ({} against {reference})",
                        count.instance,
                        format_percent(deviation.abs()),
                        count.files
                    ));
                }
            }
        }
        _ => {
            if let Some(problem) = describe_spread("files", counted, |c| c.files, tolerance) {
                problems.push(problem);
            }
        }
    }
    if let Some(problem) = describe_spread("lines", counted, |c| c.lines, tolerance) {
        problems.push(problem);
    }
    Parity {
        reference_files,
        tolerance,
        problems,
    }
}

pub fn format_percent(fraction: f64) -> String {
    format!("{:.1}%", fraction * 100.0)
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
    if low == 0 {
        return None;
    }
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

fn parse_porcelain(out: &str) -> Vec<String> {
    out.lines()
        .filter_map(|line| {
            line.trim_start()
                .split_once(' ')
                .map(|(_, rest)| rest.trim())
        })
        .filter(|rest| !rest.is_empty())
        .map(|rest| {
            rest.rsplit(" -> ")
                .next()
                .unwrap_or(rest)
                .trim_matches('"')
                .to_string()
        })
        .collect()
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

fn shorten_commit(commit: &str) -> String {
    commit.chars().take(SHORT_COMMIT).collect()
}

fn print_line(out: &mut dyn Write, message: &str) -> Result<(), String> {
    writeln!(out, "  {message}")
        .map_err(|error| format!("this report could not be written: {error}"))
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
    use super::*;

    const SHIPPED: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/corpora");

    #[test]
    fn the_shipped_corpora_parse_and_the_tolerance_is_a_percentage() {
        let corpora = read_corpora(Path::new(SHIPPED)).unwrap();
        let names: Vec<&str> = corpora.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["linebench", "linux"]);
        let linux = corpora.iter().find(|c| c.name == "linux").unwrap();
        assert!(linux.is_pinned() && !corpora[0].is_pinned());
        assert_eq!(linux.tolerance, 0.01);

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
    fn the_reference_counts_tracked_files_by_extension_whatever_its_case() {
        let listing = "a/b.c\0d.S\0e.h\0f.txt\0dir.c/g.rs\0\0";
        let wanted = ["c", "h", "s"].map(String::from);
        assert_eq!(count_files_with(listing, &wanted), 3);
        assert_eq!(count_files_with("", &wanted), 0);
    }

    #[test]
    fn parity_names_who_is_off_and_by_how_much() {
        let counted = [
            count("mezura", 61234, 36_100_000),
            count("scc", 61230, 36_100_000),
            count("tokei", 61234, 36_120_000),
            count("cloc", 57900, 34_000_000),
        ];
        let parity = judge_parity(Some(61234), &counted, 0.01);
        assert_eq!(
            parity.problems,
            [
                "cloc files 5.4% under the corpus (57900 against 61234)",
                "lines spread 6.2% (cloc 34000000 to tokei 36120000)"
            ]
        );
        assert!(
            judge_parity(Some(61234), &counted[..3], 0.01)
                .problems
                .is_empty()
        );

        let two = [
            count("mezura", 61234, 30_000_000),
            count("scc", 61234, 40_000_000),
        ];
        let parity = judge_parity(None, &two, 0.01);
        assert_eq!(
            parity.problems,
            ["lines spread 33.3% (mezura 30000000 to scc 40000000)"]
        );
        assert!(judge_parity(None, &two[..1], 0.01).problems.is_empty());
        assert!(judge_parity(Some(0), &two[..1], 0.01).problems.is_empty());
    }

    #[test]
    fn porcelain_lines_give_the_path_after_the_status_and_after_a_rename() {
        let out = " M src/a.rs\n?? new.txt\nR  old.txt -> \"new name.txt\"\n";
        assert_eq!(
            parse_porcelain(out),
            ["src/a.rs", "new.txt", "new name.txt"]
        );
        assert!(parse_porcelain("").is_empty());
    }

    fn count(instance: &str, files: u64, lines: u64) -> Counted {
        Counted {
            instance: instance.to_string(),
            files,
            lines,
        }
    }
}
