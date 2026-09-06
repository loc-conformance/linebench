use std::collections::BTreeMap;

use serde_json::Value;

use crate::counters::{COUNTS, EACH};
use crate::counters::{Definition, Output};

const ELEMENTS: &str = "[]";
const STEP: char = '.';
const TOKEI_TOTAL: &str = "Total";
const TOKEI_REPORTS: &str = "reports";
const TOKEI_CHILDREN: &str = "children";
const TOKEI_STATS: &str = "stats";
const TOKEI_BLOBS: &str = "blobs";
const TOKEI_FIELDS: [&str; 3] = ["code", "comments", "blanks"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Counts {
    pub files: u64,
    pub lines: u64,
    pub code: u64,
    pub comments: u64,
    pub buckets: BTreeMap<String, u64>,
}

impl Counts {
    pub fn sum_buckets(&self) -> u64 {
        self.buckets.values().sum()
    }
}

pub fn read_counts(definition: &Definition, text: &str) -> Result<Counts, String> {
    let document: Value = serde_json::from_str(text).map_err(|error| {
        format!(
            "{} printed something that is not JSON: {error}",
            definition.name
        )
    })?;
    let counts = match definition.output {
        Some(Output::TokeiJson) => read_tokei_document(definition, &document)?,
        None => read_by_paths(definition, &document)?,
    };
    let accounted = counts.code + counts.comments + counts.sum_buckets();
    if accounted != counts.lines {
        let mut parts = vec![
            format!("{} code", counts.code),
            format!("{} comments", counts.comments),
        ];
        parts.extend(
            counts
                .buckets
                .iter()
                .map(|(name, value)| format!("{value} {name}")),
        );
        return Err(format!(
            "{} printed {} lines and {} = {accounted}, so a bucket is missing from [read] in {} \
             or the counter does not add up",
            definition.name,
            counts.lines,
            parts.join(" + "),
            definition.path.display()
        ));
    }
    Ok(counts)
}

enum Step {
    Key(String),
    Every,
}

enum Missed {
    Key,
    List,
}

fn read_by_paths(definition: &Definition, document: &Value) -> Result<Counts, String> {
    let read = definition
        .read
        .as_ref()
        .ok_or_else(|| format!("{} has no [read] block", definition.path.display()))?;
    let elements: Vec<&Value> = match read.get(EACH) {
        Some(each) => walk_to_elements(definition, each, document)?,
        None => vec![document],
    };
    let sum_of = |name: &str| -> Result<u64, String> {
        let path = read.get(name).ok_or_else(|| {
            format!(
                "{} [read] names no path for {name}",
                definition.path.display()
            )
        })?;
        Ok(sum_over_elements(definition, &elements, name, path, true)?.unwrap_or(0))
    };
    let (files, lines, code, comments) = (
        sum_of("files")?,
        sum_of("lines")?,
        sum_of("code")?,
        sum_of("comments")?,
    );
    let mut buckets = BTreeMap::new();
    for (name, path) in read {
        if name == EACH || COUNTS.contains(&name.as_str()) {
            continue;
        }
        if let Some(found) = sum_over_elements(definition, &elements, name, path, false)? {
            buckets.insert(name.clone(), found);
        }
    }
    if !elements.is_empty() && buckets.is_empty() {
        let names: Vec<&str> = read
            .keys()
            .map(String::as_str)
            .filter(|key| *key != EACH && !COUNTS.contains(key))
            .collect();
        return Err(format!(
            "none of {} sits in what {} printed",
            names.join(", "),
            definition.name
        ));
    }
    Ok(Counts {
        files,
        lines,
        code,
        comments,
        buckets,
    })
}

fn read_tokei_document(definition: &Definition, document: &Value) -> Result<Counts, String> {
    let no_total = || format!("{} printed no {TOKEI_TOTAL} object", definition.name);
    let languages = document.as_object().ok_or_else(no_total)?;
    let total = languages
        .get(TOKEI_TOTAL)
        .and_then(Value::as_object)
        .ok_or_else(no_total)?;
    let number = |at: &str, object: &serde_json::Map<String, Value>, field: &str| {
        read_whole_number(
            definition,
            &format!("{at}.{field}"),
            object.get(field).unwrap_or(&Value::Null),
        )
    };
    let mut files = 0;
    let mut added_up = [0u64; 3];
    for (name, language) in languages
        .iter()
        .filter(|(name, _)| name.as_str() != TOKEI_TOTAL)
    {
        let language = language.as_object().ok_or_else(|| {
            format!(
                "{} printed {language} at {name}, which is not a language",
                definition.name
            )
        })?;
        files += language
            .get(TOKEI_REPORTS)
            .and_then(Value::as_array)
            .map_or(0, |reports| reports.len() as u64);
        for (slot, field) in TOKEI_FIELDS.iter().enumerate() {
            added_up[slot] += number(name, language, field)?;
        }
        let children = language.get(TOKEI_CHILDREN).and_then(Value::as_object);
        for entry in children
            .into_iter()
            .flat_map(|c| c.values())
            .filter_map(Value::as_array)
            .flatten()
        {
            let stats = entry
                .get(TOKEI_STATS)
                .and_then(Value::as_object)
                .ok_or_else(|| {
                    format!(
                        "{} printed a child of {name} without stats",
                        definition.name
                    )
                })?;
            add_tokei_stats(definition, &format!("{name} child"), stats, &mut added_up)?;
        }
    }
    let mut totals = [0u64; 3];
    for (slot, field) in TOKEI_FIELDS.iter().enumerate() {
        totals[slot] = number(TOKEI_TOTAL, total, field)?;
    }
    if totals != added_up {
        return Err(format!(
            "{} printed a {TOKEI_TOTAL} of {} code, {} comments, {} blanks and its languages with \
             their children add up to {}, {}, {}",
            definition.name, totals[0], totals[1], totals[2], added_up[0], added_up[1], added_up[2]
        ));
    }
    let [code, comments, blanks] = totals;
    Ok(Counts {
        files,
        lines: code + comments + blanks,
        code,
        comments,
        buckets: BTreeMap::from([("blanks".to_string(), blanks)]),
    })
}

fn add_tokei_stats(
    definition: &Definition,
    at: &str,
    stats: &serde_json::Map<String, Value>,
    added_up: &mut [u64; 3],
) -> Result<(), String> {
    for (slot, field) in TOKEI_FIELDS.iter().enumerate() {
        added_up[slot] += read_whole_number(
            definition,
            &format!("{at}.{field}"),
            stats.get(*field).unwrap_or(&Value::Null),
        )?;
    }
    let blobs = stats.get(TOKEI_BLOBS).and_then(Value::as_object);
    for (language, blob) in blobs.into_iter().flatten() {
        let blob = blob.as_object().ok_or_else(|| {
            format!(
                "{} printed {blob} as the {language} inside {at}, which is not a stats object",
                definition.name
            )
        })?;
        add_tokei_stats(definition, &format!("{at}.{language}"), blob, added_up)?;
    }
    Ok(())
}

fn walk_to_elements<'a>(
    definition: &Definition,
    path: &str,
    document: &'a Value,
) -> Result<Vec<&'a Value>, String> {
    let steps = parse_path(definition, EACH, path)?;
    if !matches!(steps.last(), Some(Step::Every)) {
        return Err(format!(
            "{}: [read] each = \"{path}\" does not end in {ELEMENTS}, so it names one value where \
             a list of elements is needed",
            definition.path.display()
        ));
    }
    walk(vec![document], &steps).map_err(|missed| match missed {
        Missed::Key => format!("nothing sits at {path} in what {} printed", definition.name),
        Missed::List => format!("{} printed no list at {path}", definition.name),
    })
}

fn sum_over_elements(
    definition: &Definition,
    elements: &[&Value],
    name: &str,
    path: &str,
    required: bool,
) -> Result<Option<u64>, String> {
    let steps = parse_path(definition, name, path)?;
    if steps.iter().any(|step| matches!(step, Step::Every)) {
        return Err(format!(
            "{}: [read] {name} = \"{path}\" fans out over {ELEMENTS}, and a count is one value \
             per element",
            definition.path.display()
        ));
    }
    let mut found = Vec::new();
    let mut missing = 0;
    for element in elements {
        match walk(vec![element], &steps) {
            Ok(values) => {
                for value in values {
                    found.push(read_whole_number(definition, path, value)?);
                }
            }
            Err(_) => missing += 1,
        }
    }
    if found.is_empty() {
        if required && !elements.is_empty() {
            return Err(format!(
                "nothing sits at {path} in what {} printed",
                definition.name
            ));
        }
        return Ok(None);
    }
    if missing > 0 {
        return Err(format!(
            "{path} sits in {} of what {} printed and not in the other {missing}",
            found.len(),
            definition.name
        ));
    }
    Ok(Some(found.iter().sum()))
}

fn walk<'a>(from: Vec<&'a Value>, steps: &[Step]) -> Result<Vec<&'a Value>, Missed> {
    let mut current = from;
    for step in steps {
        let mut following = Vec::new();
        match step {
            Step::Key(key) => {
                following.extend(current.iter().filter_map(|node| node.get(key)));
                if !current.is_empty() && following.is_empty() {
                    return Err(Missed::Key);
                }
            }
            Step::Every => {
                for node in &current {
                    following.extend(node.as_array().ok_or(Missed::List)?.iter());
                }
            }
        }
        current = following;
    }
    Ok(current)
}

fn parse_path(definition: &Definition, name: &str, path: &str) -> Result<Vec<Step>, String> {
    let mut steps = Vec::new();
    for part in path.split(STEP) {
        let (key, every) = match part.strip_suffix(ELEMENTS) {
            Some(key) => (key, true),
            None => (part, false),
        };
        if !key.is_empty() {
            steps.push(Step::Key(key.to_string()));
        } else if !every || part != ELEMENTS {
            return Err(format!(
                "{}: [read] {name} = \"{path}\" is not a path: dot-separated names, each \
                 optionally ending in {ELEMENTS}, or {ELEMENTS} alone for the document itself",
                definition.path.display()
            ));
        }
        if every {
            steps.push(Step::Every);
        }
    }
    Ok(steps)
}

fn read_whole_number(definition: &Definition, path: &str, value: &Value) -> Result<u64, String> {
    value.as_u64().ok_or_else(|| {
        format!(
            "{} printed {value} at {path}, which is not a whole number",
            definition.name
        )
    })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::*;
    use crate::counters::{parse_definition, read_definition};

    const SHIPPED: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/counters");
    const MEZURA_CONTENT: &str = r#"{"scope":{"counting":"content"},"total":{"files":2,"lines":1925,"code":1638,"comments":4,"extra":283,"bytes":83309}}"#;
    const MEZURA_REGION: &str = r#"{"scope":{"counting":"region"},"total":{"files":2,"lines":1925,"code":1638,"comments":4,"blanks":283}}"#;
    const SCC: &str = r#"[{"Name":"JSON","Lines":47,"Code":47,"Comment":0,"Blank":0,"Count":48,"Files":[]},{"Name":"Python","Lines":1878,"Code":1591,"Comment":4,"Blank":283,"Count":2,"Files":[]}]"#;
    const CLOC: &str = r#"{"header":{"cloc_version":"2.10","n_files":112,"n_lines":3439,"elapsed_seconds":0.67},"Python":{"nFiles":2,"blank":283,"comment":4,"code":1591},"SUM":{"blank":454,"comment":4,"code":2981,"nFiles":112}}"#;
    const TOKEI: &str = r#"{"HTML":{"blanks":1,"code":46,"comments":0,"reports":[{"name":"index.html"}],"children":{"CSS":[{"name":"index.html","stats":{"blanks":5,"code":72,"comments":8,"blobs":{"JavaScript":{"blanks":1,"code":3,"comments":0,"blobs":{}}}}}]},"inaccurate":false},"Python":{"blanks":283,"code":1591,"comments":4,"reports":[{"name":"x.py"},{"name":"y.py"}],"children":{},"inaccurate":false},"Total":{"blanks":290,"code":1712,"comments":12,"reports":[],"children":{"HTML":[]},"inaccurate":false}}"#;

    #[test]
    fn the_shipped_read_blocks_read_what_the_four_counters_print() {
        let content = read_counts(&read_shipped("mezura"), MEZURA_CONTENT).unwrap();
        assert_eq!(get_four_counts(&content), (2, 1925, 1638, 4));
        assert_eq!(content.buckets, build_bucket("extra", 283));
        let region = read_counts(&read_shipped("mezura"), MEZURA_REGION).unwrap();
        assert_eq!(region.buckets, build_bucket("blanks", 283));

        let scc = read_counts(&read_shipped("scc"), SCC).unwrap();
        assert_eq!(get_four_counts(&scc), (50, 1925, 1638, 4));
        assert_eq!(scc.buckets, build_bucket("blanks", 283));

        let cloc = read_counts(&read_shipped("cloc"), CLOC).unwrap();
        assert_eq!(get_four_counts(&cloc), (112, 3439, 2981, 4));
        assert_eq!(cloc.buckets, build_bucket("blanks", 454));

        let tokei = read_counts(&read_shipped("tokei"), TOKEI).unwrap();
        assert_eq!(get_four_counts(&tokei), (3, 2014, 1712, 12));
        assert_eq!(tokei.buckets, build_bucket("blanks", 290));
    }

    #[test]
    fn an_each_descends_before_fanning_out_and_an_empty_list_counts_nothing() {
        let nested = parse_scc_with("each     = \"[]\"", "each = \"report.languages[]\"");
        let document = r#"{"report":{"languages":[{"Count":1,"Lines":3,"Code":1,"Comment":1,"Blank":1},{"Count":2,"Lines":4,"Code":4,"Comment":0,"Blank":0}]}}"#;
        let counts = read_counts(&nested, document).unwrap();
        assert_eq!(get_four_counts(&counts), (3, 7, 5, 1));
        assert_eq!(counts.buckets, build_bucket("blanks", 1));

        let bare_step = parse_scc_with("each     = \"[]\"", "each = \"report.languages.[]\"");
        assert_eq!(read_counts(&bare_step, document).unwrap(), counts);

        let empty = read_counts(&read_shipped("scc"), "[]").unwrap();
        assert_eq!(
            (get_four_counts(&empty), empty.buckets.len()),
            ((0, 0, 0, 0), 0)
        );
        let nested_empty = read_counts(&nested, r#"{"report":{"languages":[]}}"#).unwrap();
        assert_eq!(get_four_counts(&nested_empty), (0, 0, 0, 0));
    }

    #[test]
    fn a_document_the_read_block_does_not_fit_is_refused_naming_the_path() {
        assert_refused(
            &read_shipped("scc"),
            "Lines: 47",
            "scc printed something that is not JSON",
        );
        assert_refused(
            &read_shipped("mezura"),
            r#"{"total":{"files":1,"code":1,"comments":0,"blanks":0}}"#,
            "nothing sits at total.lines in what mezura printed",
        );
        assert_refused(
            &read_shipped("mezura"),
            r#"{"total":{"files":1,"lines":1,"code":1,"comments":0}}"#,
            "none of blanks, extra sits in what mezura printed",
        );
        assert_refused(
            &read_shipped("scc"),
            r#"[{"Count":1,"Lines":1,"Code":1,"Comment":0,"Blank":0},{"Count":1,"Lines":1,"Code":1,"Comment":0}]"#,
            "Blank sits in 1 of what scc printed and not in the other 1",
        );
        assert_refused(
            &read_shipped("mezura"),
            r#"{"total":{"files":1,"lines":"1","code":1,"comments":0,"blanks":0}}"#,
            "printed \"1\" at total.lines, which is not a whole number",
        );
        assert_refused(
            &read_shipped("mezura"),
            r#"{"total":{"files":true,"lines":1,"code":1,"comments":0,"blanks":0}}"#,
            "printed true at total.files",
        );
        assert_refused(
            &read_shipped("scc"),
            r#"{"Lines":1}"#,
            "scc printed no list at []",
        );
        let nested = parse_scc_with("each     = \"[]\"", "each = \"report.languages[]\"");
        assert_refused(
            &nested,
            r#"{"error":"bad flag"}"#,
            "nothing sits at report.languages[] in what scc printed",
        );
        assert_refused(
            &read_shipped("tokei"),
            r#"{"Python":{"blanks":0,"code":1,"comments":0,"reports":[]}}"#,
            "tokei printed no Total object",
        );
    }

    #[test]
    fn counts_that_do_not_add_up_are_refused_with_the_arithmetic() {
        assert_refused(
            &read_shipped("mezura"),
            r#"{"total":{"files":1,"lines":10,"code":5,"comments":2,"blanks":1}}"#,
            "mezura printed 10 lines and 5 code + 2 comments + 1 blanks = 8",
        );
        assert_refused(
            &read_shipped("tokei"),
            &TOKEI.replace(r#""blanks":290,"code":1712"#, r#""blanks":290,"code":1700"#),
            "tokei printed a Total of 1700 code, 12 comments, 290 blanks and its languages with their children add up to 1712, 12, 290",
        );
    }

    #[test]
    fn a_read_block_that_is_not_made_of_paths_is_refused_at_the_definition() {
        assert_refused(
            &parse_scc_with("lines    = \"Lines\"", "lines = \"rows[].Lines\""),
            SCC,
            "lines = \"rows[].Lines\" fans out over []",
        );
        assert_refused(
            &parse_scc_with("each     = \"[]\"", "each = \"rows\""),
            SCC,
            "each = \"rows\" does not end in []",
        );
        assert_refused(
            &parse_scc_with("lines    = \"Lines\"", "lines = \"a..b\""),
            SCC,
            "lines = \"a..b\" is not a path",
        );
        assert_refused(
            &parse_scc_with("lines    = \"Lines\"", "lines = \"\""),
            SCC,
            "lines = \"\" is not a path",
        );
    }

    fn read_shipped(name: &str) -> Definition {
        read_definition(&Path::new(SHIPPED).join(format!("{name}.toml")))
            .expect("a shipped definition")
    }

    fn parse_scc_with(from: &str, to: &str) -> Definition {
        let text = fs::read_to_string(Path::new(SHIPPED).join("scc.toml"))
            .expect("the shipped scc definition");
        assert!(
            text.contains(from),
            "the shipped scc definition no longer says {from:?}"
        );
        parse_definition(&text.replace(from, to), Path::new("scc.toml")).expect("a definition")
    }

    fn assert_refused(definition: &Definition, text: &str, expected: &str) {
        let refusal =
            read_counts(definition, text).expect_err("counts that should have been refused");
        assert!(
            refusal.contains(expected),
            "expected {expected:?} in {refusal:?}"
        );
    }

    fn get_four_counts(counts: &Counts) -> (u64, u64, u64, u64) {
        (counts.files, counts.lines, counts.code, counts.comments)
    }

    fn build_bucket(name: &str, value: u64) -> BTreeMap<String, u64> {
        BTreeMap::from([(name.to_string(), value)])
    }
}
