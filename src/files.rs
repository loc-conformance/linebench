use std::fs;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;

const GIT_DIR: &str = ".git";
const ROOT_DIR: &str = ".";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SizedFile {
    pub path: PathBuf,
    pub relative: String,
    pub directory: String,
    pub extension: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LargestFiles {
    pub total_files: u64,
    pub total_bytes: u64,
    pub files: Vec<SizedFile>,
}

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

pub fn find_largest_files(
    root: &Path,
    extensions: &[String],
    limit: usize,
) -> Result<LargestFiles, String> {
    let mut stack = vec![root.to_path_buf()];
    let mut total_files = 0;
    let mut total_bytes = 0;
    let mut files = Vec::new();
    while let Some(dir) = stack.pop() {
        let entries = fs::read_dir(&dir)
            .map_err(|error| format!("{} could not be read: {error}", dir.display()))?;
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() {
                continue;
            }
            let path = entry.path();
            if file_type.is_dir() {
                if entry.file_name() != GIT_DIR {
                    stack.push(path);
                }
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            let Some(extension) = path
                .extension()
                .and_then(|ext| ext.to_str())
                .filter(|ext| !ext.is_empty())
            else {
                continue;
            };
            if !extensions.is_empty()
                && !extensions
                    .iter()
                    .any(|wanted| wanted.eq_ignore_ascii_case(extension))
            {
                continue;
            }
            let bytes = entry.metadata().map(|meta| meta.len()).unwrap_or(0);
            total_files += 1;
            total_bytes += bytes;
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let directory = match relative.rfind('/') {
                Some(at) => relative[..at].to_string(),
                None => ROOT_DIR.to_string(),
            };
            let extension = extension.to_string();
            files.push(SizedFile {
                path,
                relative,
                directory,
                extension,
                bytes,
            });
        }
    }
    if files.is_empty() {
        return Err(match extensions.is_empty() {
            true => format!(
                "no files with an extension were found under {}",
                root.display()
            ),
            false => format!(
                "no files matching {} were found under {}",
                extensions.join(", "),
                root.display()
            ),
        });
    }
    files.sort_by(|left, right| {
        right
            .bytes
            .cmp(&left.bytes)
            .then_with(|| left.relative.cmp(&right.relative))
    });
    files.truncate(limit);
    Ok(LargestFiles {
        total_files,
        total_bytes,
        files,
    })
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::process;

    use super::*;

    #[test]
    fn the_largest_files_are_kept_in_size_order_and_filtered_by_extension() {
        let root = env::temp_dir().join(format!("linebench-files-{}", process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("sub")).unwrap();
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(root.join("top.rs"), "123456").unwrap();
        fs::write(root.join("sub").join("big.rs"), "1234567890").unwrap();
        fs::write(root.join("sub").join("tie_b.h"), "1234").unwrap();
        fs::write(root.join("sub").join("tie_a.H"), "1234").unwrap();
        fs::write(root.join("Makefile"), "12345678901234567890").unwrap();
        fs::write(root.join("ignored.txt"), "12345678901234567890").unwrap();
        fs::write(root.join(".git").join("huge.rs"), "12345678901234567890").unwrap();

        let extensions = vec!["rs".to_string(), "h".to_string()];
        let found = find_largest_files(&root, &extensions, 3).unwrap();
        let unrestricted = find_largest_files(&root, &[], 2).unwrap();
        let none_matching = find_largest_files(&root, &["py".to_string()], 3).unwrap_err();
        let _ = fs::remove_dir_all(&root);

        assert_eq!(found.total_files, 4);
        assert_eq!(found.total_bytes, 10 + 6 + 4 + 4);
        let relatives: Vec<(&str, &str, u64)> = found
            .files
            .iter()
            .map(|f| (f.relative.as_str(), f.directory.as_str(), f.bytes))
            .collect();
        assert_eq!(
            relatives,
            vec![
                ("sub/big.rs", "sub", 10),
                ("top.rs", ".", 6),
                ("sub/tie_a.H", "sub", 4),
            ]
        );
        assert_eq!(unrestricted.total_files, 5);
        assert_eq!(unrestricted.files[0].relative, "ignored.txt");
        assert!(
            none_matching.starts_with("no files matching py were found under "),
            "{none_matching}"
        );
    }
}
