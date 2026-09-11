use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use linebench::corpus::{Corpus, parse_corpus};
use linebench::counters::{Definition, parse_definition};
use linebench::files::{parse_toml, read_text};
use linebench::measure::print_line;

include!(concat!(env!("OUT_DIR"), "/shipped.rs"));

const DEFINITION_SUFFIX: &str = "toml";
const COUNTER_KEY: &str = "run";
const CORPUS_KEY: &str = "extensions";

#[derive(Debug)]
pub struct Definitions {
    pub counters: Vec<Definition>,
    pub corpora: Vec<Corpus>,
}

pub fn collect_definitions(out: &mut dyn Write, added: &[PathBuf]) -> Result<Definitions, String> {
    let mut counters: Vec<Definition> = COUNTERS
        .iter()
        .map(|(name, text)| parse_definition(text, Path::new(name)))
        .collect::<Result<_, _>>()?;
    let mut corpora: Vec<Corpus> = CORPORA
        .iter()
        .map(|(name, text)| parse_corpus(text, Path::new(name)))
        .collect::<Result<_, _>>()?;
    for path in added {
        for file in list_definition_files(path)? {
            let text = read_text(&file)?;
            let document: toml::Value = parse_toml(&text, &file)?;
            let has = |key: &str| {
                document
                    .as_table()
                    .is_some_and(|table| table.contains_key(key))
            };
            if has(COUNTER_KEY) {
                let mut definition = parse_definition(&text, &file)?;
                definition.added = true;
                match counters.iter().position(|d| d.name == definition.name) {
                    Some(at) => {
                        print_line(
                            out,
                            &format!("{}: taken from {}", definition.name, file.display()),
                        )?;
                        counters[at] = definition;
                    }
                    None => counters.push(definition),
                }
            } else if has(CORPUS_KEY) {
                let mut corpus = parse_corpus(&text, &file)?;
                corpus.added = true;
                match corpora.iter().position(|c| c.name == corpus.name) {
                    Some(at) => {
                        print_line(
                            out,
                            &format!("{}: taken from {}", corpus.name, file.display()),
                        )?;
                        corpora[at] = corpus;
                    }
                    None => corpora.push(corpus),
                }
            } else {
                return Err(format!(
                    "{}: neither a counter definition, which has a [run] table, nor a corpus \
                     definition, which has extensions",
                    file.display()
                ));
            }
        }
    }
    counters.sort_by(|a, b| a.name.cmp(&b.name));
    corpora.sort_by(|a, b| a.name.cmp(&b.name));
    for corpus in &corpora {
        for (system, names) in &corpus.skip {
            if let Some(name) = names
                .iter()
                .find(|name| !counters.iter().any(|d| &d.name == *name))
            {
                return Err(format!(
                    "{}: [skip] names {name} under {system}, and no counter definition has that \
                     name",
                    corpus.path.display()
                ));
            }
        }
    }
    Ok(Definitions { counters, corpora })
}

fn list_definition_files(path: &Path) -> Result<Vec<PathBuf>, String> {
    if path.is_file() {
        return Ok(vec![path.to_path_buf()]);
    }
    if !path.is_dir() {
        return Err(format!(
            "{} is not there, so nothing can be added from it",
            path.display()
        ));
    }
    let entries = fs::read_dir(path)
        .map_err(|error| format!("{} could not be read: {error}", path.display()))?;
    let mut files: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|file| file.is_file())
        .filter(|file| file.extension().is_some_and(|ext| ext == DEFINITION_SUFFIX))
        .collect();
    files.sort();
    Ok(files)
}

#[cfg(test)]
mod tests {
    use std::env;

    use super::*;

    const OLDER_SCC: &str = "\
name = \"scc\"

[acquisition]
channel = \"github-release-asset\"
name    = \"boyter/scc\"
version = \"3.7.0\"

[run]
args      = [\"{target}\"]
json      = [\"--format\", \"json\"]
languages = [\"-i\", \"{extensions}\"]

[read]
each     = \"[]\"
files    = \"Count\"
lines    = \"Lines\"
code     = \"Code\"
comments = \"Comment\"
blanks   = \"Blank\"
";

    #[test]
    fn an_added_file_takes_the_place_of_the_built_in_one_of_its_name_and_a_new_one_joins() {
        let dir = env::temp_dir().join("linebench-an_added_file_takes_the_place");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("scc.toml"), OLDER_SCC).unwrap();
        fs::write(
            dir.join("mine.toml"),
            "name = \"mine\"\nextensions = [\"rs\"]\n",
        )
        .unwrap();
        let mut printed = Vec::new();
        let found = collect_definitions(&mut printed, std::slice::from_ref(&dir)).unwrap();
        let scc = found.counters.iter().find(|d| d.name == "scc").unwrap();
        assert_eq!(scc.acquisition.as_ref().unwrap().version, "3.7.0");
        assert_eq!(scc.path, dir.join("scc.toml"));
        assert_eq!(found.counters.len(), COUNTERS.len());
        assert!(found.corpora.iter().any(|c| c.name == "mine"));
        assert_eq!(found.corpora.len(), CORPORA.len() + 1);
        assert!(
            String::from_utf8(printed)
                .unwrap()
                .starts_with("scc: taken from")
        );
        fs::write(dir.join("odd.toml"), "name = \"odd\"\n").unwrap();
        let refused = collect_definitions(&mut Vec::new(), &[dir.join("odd.toml")]).unwrap_err();
        assert!(
            refused.contains("neither a counter definition"),
            "{refused}"
        );
        let missing = collect_definitions(&mut Vec::new(), &[dir.join("gone")]).unwrap_err();
        assert!(missing.contains("is not there"), "{missing}");
        fs::write(
            dir.join("big.toml"),
            "name = \"big\"\nextensions = [\"c\"]\n[skip]\nwindows = [\"nope\"]\n",
        )
        .unwrap();
        let unknown = collect_definitions(&mut Vec::new(), &[dir.join("big.toml")]).unwrap_err();
        assert!(
            unknown.contains("no counter definition has that name"),
            "{unknown}"
        );
        fs::remove_dir_all(&dir).unwrap();
    }
}
