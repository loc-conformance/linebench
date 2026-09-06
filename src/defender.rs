use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::machine::Platform;
use crate::machine::UNKNOWN;
use crate::os::run_powershell_with_status;

const PLACEHOLDER: &str = "N/A";
const LIST_SEPARATOR: char = '|';
const BLOCK_SEPARATOR: &str = "---";
const QUERY: &str = "$ErrorActionPreference = \"Stop\"; \
    try { $p = Get-MpPreference; \
    $p.ExclusionProcess -join \"|\"; \"---\"; \
    $p.ExclusionPath -join \"|\"; \"---\"; \
    (Get-MpComputerStatus).RealTimeProtectionEnabled } catch { exit 3 }";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Answer {
    #[serde(rename = "yes")]
    Yes,
    #[serde(rename = "no")]
    No,
    #[serde(rename = "not applicable")]
    NotApplicable,
    #[serde(rename = "not checked")]
    NotChecked,
    #[serde(rename = "needs admin")]
    NeedsAdmin,
    #[serde(rename = "could not query")]
    CouldNotQuery,
}

impl Answer {
    pub fn of(excluded: bool) -> Answer {
        if excluded { Answer::Yes } else { Answer::No }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Answer::Yes => "yes",
            Answer::No => "no",
            Answer::NotApplicable => "not applicable",
            Answer::NotChecked => "not checked",
            Answer::NeedsAdmin => "needs admin",
            Answer::CouldNotQuery => "could not query",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Exclusions {
    pub process: Answer,
    pub binary: Answer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DefenderState {
    pub realtime: String,
    pub corpus_excluded: Answer,
    pub counters: BTreeMap<String, Exclusions>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CounterBinary {
    pub name: String,
    pub path: PathBuf,
    pub process: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessExclusions {
    NoCounters,
    AllExcluded,
    NoneExcluded,
    Unknown(Answer),
    Unequal,
}

pub fn judge_process_exclusions(state: &DefenderState) -> ProcessExclusions {
    let answers: Vec<Answer> = state.counters.values().map(|e| e.process).collect();
    match answers.as_slice() {
        [] => ProcessExclusions::NoCounters,
        [first, rest @ ..] if rest.iter().all(|a| a == first) => match first {
            Answer::Yes => ProcessExclusions::AllExcluded,
            Answer::No => ProcessExclusions::NoneExcluded,
            other => ProcessExclusions::Unknown(*other),
        },
        _ => ProcessExclusions::Unequal,
    }
}

pub fn read_defender_state(
    platform: Platform,
    privileged: bool,
    corpus: &Path,
    binaries: &[CounterBinary],
) -> DefenderState {
    let everywhere = |answer: Answer| DefenderState {
        realtime: answer.as_str().to_string(),
        corpus_excluded: answer,
        counters: binaries
            .iter()
            .map(|binary| {
                let both = Exclusions {
                    process: answer,
                    binary: answer,
                };
                (binary.name.clone(), both)
            })
            .collect(),
    };
    match platform {
        Platform::Windows => {}
        Platform::Wsl => return everywhere(Answer::NotChecked),
        Platform::Linux | Platform::Macos => return everywhere(Answer::NotApplicable),
    }
    let unknown = if privileged {
        Answer::CouldNotQuery
    } else {
        Answer::NeedsAdmin
    };
    let ExclusionLists {
        processes,
        paths,
        realtime,
    } = match read_exclusion_lists() {
        Some(found) => found,
        None => return everywhere(unknown),
    };
    let processes: Option<Vec<String>> =
        processes.map(|list| list.iter().map(|entry| normalize_path(entry)).collect());
    let paths: Option<Vec<String>> =
        paths.map(|list| list.iter().map(|entry| normalize_path(entry)).collect());
    let corpus_excluded = paths
        .as_deref()
        .map_or(unknown, |roots| Answer::of(sits_under(corpus, roots)));
    let counters = binaries
        .iter()
        .map(|binary| {
            let process = processes.as_deref().map_or(unknown, |entries| {
                let shown = normalize_path(&binary.path.to_string_lossy());
                Answer::of(
                    entries.contains(&normalize_path(&binary.process)) || entries.contains(&shown),
                )
            });
            let excluded_path = paths
                .as_deref()
                .map_or(unknown, |roots| Answer::of(sits_under(&binary.path, roots)));
            (
                binary.name.clone(),
                Exclusions {
                    process,
                    binary: excluded_path,
                },
            )
        })
        .collect();
    DefenderState {
        realtime,
        corpus_excluded,
        counters,
    }
}

pub fn find_unequal_exclusions(state: &DefenderState) -> Option<String> {
    let mut problems = Vec::new();
    if let Some((excluded, included)) = split_by_answer(state, |e| e.process) {
        problems.push(describe_split("process", &excluded, &included));
    }
    if let Some((excluded, included)) = split_by_answer(state, |e| e.binary) {
        problems.push(describe_split("binary path", &excluded, &included));
    }
    if problems.is_empty() {
        None
    } else {
        Some(problems.join(". "))
    }
}

pub fn explain_unequal_exclusions(unequal: &str) -> String {
    format!(
        "MS Defender does not treat the counters equally:\n{unequal}.\n\n\
         Files opened by an excluded process are never scanned in real time, so this\n\
         run would measure which counter escaped the antivirus, not which counts faster.\n\n\
         \x20 Get-MpPreference | Select -ExpandProperty ExclusionProcess\n\
         \x20 Get-MpPreference | Select -ExpandProperty ExclusionPath\n\
         \x20 Remove-MpPreference -ExclusionProcess <name>.exe\n\n\
         Either exclude every counter or none, or rerun with --allow-unequal-exclusions\n\
         to measure anyway and have the results marked as such."
    )
}

pub fn parse_exclusion_list(raw: &str) -> Option<Vec<String>> {
    if raw.trim().to_uppercase().starts_with(PLACEHOLDER) {
        return None;
    }
    Some(
        raw.split(LIST_SEPARATOR)
            .map(str::trim)
            .filter(|entry| !entry.is_empty())
            .map(str::to_string)
            .collect(),
    )
}

struct ExclusionLists {
    processes: Option<Vec<String>>,
    paths: Option<Vec<String>>,
    realtime: String,
}

fn read_exclusion_lists() -> Option<ExclusionLists> {
    let (ok, out) = run_powershell_with_status(QUERY)?;
    if !ok {
        return None;
    }
    let blocks = split_blocks(&out);
    let mut blocks = blocks.iter().map(|block| block.trim());
    let processes = parse_exclusion_list(blocks.next()?);
    let paths = parse_exclusion_list(blocks.next()?);
    let realtime = blocks
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(UNKNOWN)
        .to_string();
    Some(ExclusionLists {
        processes,
        paths,
        realtime,
    })
}

fn split_blocks(out: &str) -> Vec<String> {
    let mut blocks = vec![String::new()];
    for line in out.lines() {
        if line.trim() == BLOCK_SEPARATOR {
            blocks.push(String::new());
        } else if let Some(block) = blocks.last_mut() {
            block.push_str(line);
            block.push('\n');
        }
    }
    blocks
}

fn split_by_answer(
    state: &DefenderState,
    pick: impl Fn(&Exclusions) -> Answer,
) -> Option<(Vec<&str>, Vec<&str>)> {
    let mut excluded = Vec::new();
    let mut included = Vec::new();
    for (name, exclusions) in &state.counters {
        match pick(exclusions) {
            Answer::Yes => excluded.push(name.as_str()),
            Answer::No => included.push(name.as_str()),
            _ => return None,
        }
    }
    if excluded.is_empty() || included.is_empty() {
        return None;
    }
    Some((excluded, included))
}

fn describe_split(kind: &str, excluded: &[&str], included: &[&str]) -> String {
    format!(
        "{kind} exclusions cover {} and not {}",
        excluded.join(", "),
        included.join(", ")
    )
}

fn sits_under(path: &Path, normalized_roots: &[String]) -> bool {
    let text = normalize_path(&path.to_string_lossy());
    normalized_roots.iter().any(|root| {
        text.strip_prefix(root.as_str())
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('\\'))
    })
}

fn normalize_path(text: &str) -> String {
    text.replace('/', "\\")
        .to_lowercase()
        .trim_end_matches('\\')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_placeholder_defender_prints_without_admin_is_not_an_empty_list() {
        assert_eq!(
            parse_exclusion_list("N/A: Must be an administrator to view exclusions"),
            None
        );
        assert_eq!(parse_exclusion_list("").as_deref(), Some(&[][..]));
        assert_eq!(
            parse_exclusion_list("mezura.exe | D:\\counters\\scc.exe|").as_deref(),
            Some(
                &[
                    "mezura.exe".to_string(),
                    "D:\\counters\\scc.exe".to_string()
                ][..]
            )
        );
    }

    #[test]
    fn exclusions_are_unequal_only_when_every_answer_is_known_and_they_differ() {
        let known = |process: bool, binary: bool| Exclusions {
            process: Answer::of(process),
            binary: Answer::of(binary),
        };
        let mut state = DefenderState {
            realtime: "True".to_string(),
            corpus_excluded: Answer::No,
            counters: BTreeMap::from([
                ("cloc".to_string(), known(false, false)),
                ("mezura".to_string(), known(true, false)),
                ("scc".to_string(), known(true, true)),
                ("tokei".to_string(), known(false, false)),
            ]),
        };
        assert_eq!(
            find_unequal_exclusions(&state).as_deref(),
            Some(
                "process exclusions cover mezura, scc and not cloc, tokei. binary path \
                 exclusions cover scc and not cloc, mezura, tokei"
            )
        );
        state.counters.insert(
            "tokei".to_string(),
            Exclusions {
                process: Answer::NeedsAdmin,
                binary: Answer::Yes,
            },
        );
        assert_eq!(
            find_unequal_exclusions(&state).as_deref(),
            Some("binary path exclusions cover scc, tokei and not cloc, mezura")
        );
        for name in ["cloc", "mezura", "scc", "tokei"] {
            state.counters.insert(name.to_string(), known(true, true));
        }
        assert_eq!(find_unequal_exclusions(&state), None);
    }

    #[test]
    fn the_blocks_are_split_on_a_separator_line_and_never_inside_a_path() {
        let out = "mezura.exe|scc.exe\n---\nC:\\build---cache|D:\\corpora\n---\nTrue\n";
        let blocks = split_blocks(out);
        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[1].trim(), "C:\\build---cache|D:\\corpora");
        assert_eq!(blocks[2].trim(), "True");
    }

    #[test]
    fn a_path_sits_under_a_root_whatever_the_slashes_and_the_case() {
        let roots: Vec<String> = ["D:/Counters", "C:\\corpus\\"]
            .iter()
            .map(|root| normalize_path(root))
            .collect();
        assert!(sits_under(Path::new("d:\\counters\\scc.exe"), &roots));
        assert!(sits_under(Path::new("C:/corpus"), &roots));
        assert!(!sits_under(Path::new("D:\\countersmith\\scc.exe"), &roots));
    }

    #[test]
    fn an_answer_reads_back_from_the_words_the_record_holds() {
        let written = serde_json::to_string(&Answer::NeedsAdmin).unwrap();
        assert_eq!(written, format!("\"{}\"", Answer::NeedsAdmin.as_str()));
        let read: Answer = serde_json::from_str("\"not checked\"").unwrap();
        assert_eq!(read, Answer::NotChecked);
    }
}
