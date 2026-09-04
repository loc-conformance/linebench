use std::path::Path;

use linebench::corpus::{Corpus, parse_corpus, read_corpora};
use linebench::counters::{Definition, parse_definition, read_definitions};

include!(concat!(env!("OUT_DIR"), "/shipped.rs"));

pub fn read_shipped_definitions(from: Option<&Path>) -> Result<Vec<Definition>, String> {
    match from {
        Some(dir) => read_definitions(&dir.join("counters")),
        None => COUNTERS
            .iter()
            .map(|(name, text)| parse_definition(text, Path::new(name)))
            .collect(),
    }
}

pub fn read_shipped_corpora(from: Option<&Path>) -> Result<Vec<Corpus>, String> {
    match from {
        Some(dir) => read_corpora(&dir.join("corpora")),
        None => CORPORA
            .iter()
            .map(|(name, text)| parse_corpus(text, Path::new(name)))
            .collect(),
    }
}
