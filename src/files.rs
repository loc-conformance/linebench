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
