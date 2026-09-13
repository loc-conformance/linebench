use std::fs;
use std::path::Path;

use serde::de::DeserializeOwned;

pub fn read_text(path: &Path) -> Result<String, String> {
    fs::read_to_string(path)
        .map_err(|error| format!("{} could not be read: {error}", path.display()))
}

pub fn read_toml<T: DeserializeOwned>(path: &Path) -> Result<T, String> {
    let text = read_text(path)?;
    parse_toml(&text, path)
}

pub fn parse_toml<T: DeserializeOwned>(text: &str, path: &Path) -> Result<T, String> {
    toml::from_str(text).map_err(|error| format!("{}: {error}", path.display()))
}

/// A record or an insights session. Both carry the format they were written as, so a file from
/// another build is told apart from a file this build simply cannot read.
pub fn read_json<T: DeserializeOwned>(path: &Path, what: &str, reads: u32) -> Result<T, String> {
    let text = read_text(path)?;
    serde_json::from_str(&text).map_err(|error| {
        let written_by = serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|value| value.get("format").and_then(serde_json::Value::as_u64));
        match written_by {
            Some(format) if format != u64::from(reads) => format!(
                "{}: written as {what} format {format}, and this build reads format {reads}: \
                 {error}",
                path.display()
            ),
            _ => format!("{}: {error}", path.display()),
        }
    })
}
