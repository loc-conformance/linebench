use std::env;
use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};

const SHIPPED_DIRS: [(&str, &str); 2] = [("COUNTERS", "counters"), ("CORPORA", "corpora")];

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("cargo sets this"));
    let out = Path::new(&env::var("OUT_DIR").expect("cargo sets this")).join("shipped.rs");
    let mut table = String::new();
    for (name, dir) in SHIPPED_DIRS {
        println!("cargo:rerun-if-changed={dir}");
        let mut files: Vec<PathBuf> = fs::read_dir(manifest_dir.join(dir))
            .expect("a shipped directory")
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "toml"))
            .collect();
        files.sort();
        writeln!(table, "pub const {name}: &[(&str, &str)] = &[").expect("a string grows");
        for file in files {
            let relative = format!(
                "{dir}/{}",
                file.file_name().expect("a file").to_string_lossy()
            );
            let absolute = file.display().to_string();
            writeln!(table, "    ({relative:?}, include_str!({absolute:?})),")
                .expect("a string grows");
        }
        writeln!(table, "];").expect("a string grows");
    }
    fs::write(&out, table).expect("the shipped table is written");
}
