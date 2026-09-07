use std::collections::BTreeMap;
use std::env;
use std::env::consts::EXE_SUFFIX;
use std::fs;
use std::io::{self, Write};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::corpus::shorten_hash;
use crate::counters::{Acquisition, Channel, Definition};
use crate::files::read_toml;
use crate::machine::Platform;
use crate::measure::print_line;
use crate::os::{Unfinished, capture_with_status};

pub const MANIFEST_FILE: &str = "linebench-fetched.toml";
pub const GIVEN_DIR: &str = "given";
pub const INSTANCE_SEPARATOR: char = '@';
const GITHUB_API: &str = "https://api.github.com/repos";
const GITHUB_TOKEN_ENVS: [&str; 2] = ["GITHUB_TOKEN", "GH_TOKEN"];
const RATE_LIMIT_MARKS: [&str; 4] = ["error: 403", "error 403", "error: 429", "error 429"];
const BAD_TOKEN_MARKS: [&str; 2] = ["error: 401", "error 401"];
const CHECKSUM_WORDS: [&str; 2] = ["checksum", "sha256"];
const PARTIAL_PREFIX: &str = ".partial-";
const USER_AGENT: &str = concat!(
    "linebench/",
    env!("CARGO_PKG_VERSION"),
    " (+",
    env!("CARGO_PKG_REPOSITORY"),
    ")"
);

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Manifest(pub BTreeMap<String, Fetched>);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Fetched {
    pub channel: Channel,
    pub version: String,
    pub source: String,
    pub sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub built_with: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "origin")]
pub enum Origin {
    Fetched {
        channel: Channel,
        version: String,
        source: String,
    },
    Built {
        version: String,
        built_with: String,
    },
    Given {
        label: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identity {
    pub instance: String,
    pub counter: String,
    pub binary: PathBuf,
    pub sha256: String,
    pub version: String,
    pub origin: Origin,
}

impl Identity {
    pub fn is_a_local_build(&self) -> bool {
        matches!(self.origin, Origin::Given { .. })
    }

    pub fn describe_origin(&self) -> String {
        match &self.origin {
            Origin::Fetched { source, .. } => format!("fetched, {source}"),
            Origin::Built { built_with, .. } => format!("built with {built_with}"),
            Origin::Given { label } => {
                format!("LOCAL BUILD {label}, sha256 {}", shorten_hash(&self.sha256))
            }
        }
    }
}

pub fn read_manifest(dir: &Path) -> Result<Manifest, String> {
    let path = dir.join(MANIFEST_FILE);
    if !path.is_file() {
        return Ok(Manifest::default());
    }
    read_toml(&path)
}

pub fn write_manifest(dir: &Path, manifest: &Manifest) -> Result<(), String> {
    let path = dir.join(MANIFEST_FILE);
    let text = toml::to_string(manifest)
        .map_err(|error| format!("the manifest could not be written out: {error}"))?;
    fs::write(&path, text)
        .map_err(|error| format!("{} could not be written: {error}", path.display()))
}

pub fn fetch_counter(
    out: &mut dyn Write,
    definition: &Definition,
    platform: Platform,
    arch: &str,
    dir: &Path,
    manifest: &mut Manifest,
) -> Result<PathBuf, String> {
    let system = platform.as_system();
    let named = definition.get_binary_name(system)?;
    let target = dir.join(&named);
    let Some(how) = &definition.acquisition else {
        return Err(format!(
            "{} says nothing about where to fetch it from, so a build of it is measured with \
             --given {}@<tag>=<path>",
            definition.path.display(),
            definition.name
        ));
    };
    if let Some(fetched) = manifest.0.get(&definition.name)
        && fetched.version == how.version
        && target.is_file()
        && calculate_sha256(&target)? == fetched.sha256
    {
        print_line(
            out,
            &format!("  {} {} is already here", definition.name, how.version),
        )?;
        return Ok(target);
    }
    let partial = dir.join(format!("{PARTIAL_PREFIX}{}", definition.name));
    let _ = fs::remove_dir_all(&partial);
    fs::create_dir_all(&partial)
        .map_err(|error| format!("{} could not be created: {error}", partial.display()))?;
    let assembled = match how.channel {
        Channel::CratesIo => build_from_crates_io(out, definition, how, &partial, &named),
        Channel::GithubReleaseAsset => {
            download_release_asset(out, how, &partial, system, arch, &named)
        }
        Channel::GithubReleaseFile => download_release_file(
            out,
            how,
            &partial,
            &definition.get_release_file_name(system)?,
        ),
    };
    let (binary, source, built_with) = assembled.inspect_err(|_| {
        let _ = fs::remove_dir_all(&partial);
    })?;
    let printed = read_version(definition, &binary).inspect_err(|_| {
        let _ = fs::remove_dir_all(&partial);
    })?;
    if !is_the_declared_version(&how.version, &printed) {
        let _ = fs::remove_dir_all(&partial);
        return Err(format!(
            "{} {} was asked for and what arrived says it is \"{printed}\", so the channel \
             handed over something else",
            definition.name, how.version
        ));
    }
    let _ = fs::remove_file(&target);
    fs::rename(&binary, &target).map_err(|error| {
        format!(
            "{} could not be moved into place: {error}",
            binary.display()
        )
    })?;
    let _ = fs::remove_dir_all(&partial);
    let sha256 = calculate_sha256(&target)?;
    manifest.0.insert(
        definition.name.clone(),
        Fetched {
            channel: how.channel,
            version: how.version.clone(),
            source,
            sha256,
            built_with,
        },
    );
    write_manifest(dir, manifest)?;
    print_line(
        out,
        &format!("  {} {} is ready", definition.name, how.version),
    )?;
    Ok(target)
}

pub fn identify_counter(
    definition: &Definition,
    platform: Platform,
    dir: &Path,
    manifest: &Manifest,
) -> Result<Identity, String> {
    let binary = dir.join(definition.get_binary_name(platform.as_system())?);
    if !binary.is_file() {
        let how = match &definition.acquisition {
            Some(how) => format!("run setup to fetch {} {}", definition.name, how.version),
            None => format!(
                "a build of it is measured with --given {}@<tag>=<path>",
                definition.name
            ),
        };
        return Err(format!(
            "no {} in {}: {how}",
            binary.display(),
            dir.display()
        ));
    }
    let sha256 = calculate_sha256(&binary)?;
    let version = read_version(definition, &binary)?;
    let origin = decide_origin(
        definition,
        &binary,
        manifest.0.get(&definition.name),
        &sha256,
    )?;
    if let Some(how) = &definition.acquisition
        && !is_the_declared_version(&how.version, &version)
    {
        return Err(format!(
            "{} declares {} and the binary at {} says it is \"{version}\"; run setup again",
            definition.path.display(),
            how.version,
            binary.display()
        ));
    }
    Ok(Identity {
        instance: definition.name.clone(),
        counter: definition.name.clone(),
        binary,
        sha256,
        version,
        origin,
    })
}

pub fn stage_given(
    definition: &Definition,
    tag: &str,
    source: &Path,
    platform: Platform,
    dir: &Path,
) -> Result<Identity, String> {
    if !source.is_file() {
        return Err(format!("{} is not a file", source.display()));
    }
    let instance = format!("{}{INSTANCE_SEPARATOR}{tag}", definition.name);
    let staged = dir.join(GIVEN_DIR).join(&instance);
    fs::create_dir_all(&staged)
        .map_err(|error| format!("{} could not be created: {error}", staged.display()))?;
    let binary = staged.join(definition.get_binary_name(platform.as_system())?);
    fs::copy(source, &binary).map_err(|error| {
        format!(
            "{} could not be copied to {}: {error}",
            source.display(),
            binary.display()
        )
    })?;
    mark_runnable(&binary)?;
    let sha256 = calculate_sha256(&binary)?;
    let version = read_version(definition, &binary)?;
    Ok(Identity {
        instance,
        counter: definition.name.clone(),
        binary,
        sha256,
        version,
        origin: Origin::Given {
            label: tag.to_string(),
        },
    })
}

pub fn decide_origin(
    definition: &Definition,
    binary: &Path,
    fetched: Option<&Fetched>,
    sha256: &str,
) -> Result<Origin, String> {
    let Some(fetched) = fetched.filter(|fetched| fetched.sha256 == sha256) else {
        let what = match fetched {
            Some(_) => "is not what setup fetched, its sha256 differs",
            None => "was not fetched by setup",
        };
        return Err(format!(
            "{} {what}: run setup again, or measure your own build with --given \
             {}@<tag>=<path>",
            binary.display(),
            definition.name
        ));
    };
    if let Some(how) = &definition.acquisition
        && how.version != fetched.version
    {
        return Err(format!(
            "{} declares {} and what setup fetched is {}; run setup again",
            definition.path.display(),
            how.version,
            fetched.version
        ));
    }
    Ok(match fetched.channel {
        Channel::CratesIo => Origin::Built {
            version: fetched.version.clone(),
            built_with: fetched.built_with.clone().unwrap_or_default(),
        },
        channel => Origin::Fetched {
            channel,
            version: fetched.version.clone(),
            source: fetched.source.clone(),
        },
    })
}

pub fn read_version(definition: &Definition, binary: &Path) -> Result<String, String> {
    let program = binary.to_string_lossy();
    let (ok, out) = capture_with_status(&program, &[definition.version_flag.as_str()]).map_err(
        |unfinished| match unfinished {
            Unfinished::NotFound if binary.is_file() => format!(
                "{program} is there but cannot be started: the interpreter named on its first \
                 line, or the loader its build was linked against, is not on this machine"
            ),
            other => other.describe(&format!("{program} {}", definition.version_flag)),
        },
    )?;
    let printed = out.split_whitespace().collect::<Vec<_>>().join(" ");
    if !ok || printed.is_empty() {
        return Err(format!(
            "{} answered nothing to {}",
            binary.display(),
            definition.version_flag
        ));
    }
    Ok(printed)
}

pub fn is_the_declared_version(declared: &str, printed: &str) -> bool {
    let stands_alone =
        |beside: Option<char>| !beside.is_some_and(|c| c.is_ascii_digit() || c == '.');
    printed.match_indices(declared).any(|(at, _)| {
        stands_alone(printed[..at].chars().next_back())
            && stands_alone(printed[at + declared.len()..].chars().next())
    })
}

pub fn calculate_sha256(file: &Path) -> Result<String, String> {
    let bytes =
        fs::read(file).map_err(|error| format!("{} could not be read: {error}", file.display()))?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

enum Refusal {
    Missing(String),
    Refused(String),
}

impl Refusal {
    fn into_words(self) -> String {
        match self {
            Refusal::Missing(message) | Refusal::Refused(message) => message,
        }
    }
}

#[derive(Deserialize)]
struct Release {
    assets: Vec<Asset>,
}

#[derive(Debug, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

type Assembled = (PathBuf, String, Option<String>);

fn build_from_crates_io(
    out: &mut dyn Write,
    definition: &Definition,
    how: &Acquisition,
    into: &Path,
    named: &str,
) -> Result<Assembled, String> {
    let root = into.join("cargo");
    print_line(
        out,
        &format!(
            "  building {} {} from crates.io, which takes a while",
            how.name, how.version
        ),
    )?;
    let root_shown = root.display().to_string();
    let asked = [
        "install",
        &how.name,
        "--version",
        &how.version,
        "--root",
        &root_shown,
    ];
    run_program("cargo", &asked, None).map_err(|refused| match refused {
        Refusal::Missing(_) => format!(
            "{} {} ships no binaries and cargo is not on this machine to build it. Install cargo, \
             or run `cargo install {} --version {}` where it exists and measure the binary with \
             --given {}@<tag>=<path>",
            definition.name, how.version, how.name, how.version, definition.name
        ),
        Refusal::Refused(message) => message,
    })?;
    let built = root
        .join("bin")
        .join(format!("{}{EXE_SUFFIX}", definition.name));
    if !built.is_file() {
        return Err(format!(
            "cargo built {} {} and no {named} came out of it, so the crate installs its command \
             under another name",
            how.name, how.version
        ));
    }
    let binary = into.join(named);
    fs::rename(&built, &binary)
        .map_err(|error| format!("{} could not be moved into place: {error}", built.display()))?;
    let built_with = capture_with_status("rustc", &["--version"])
        .ok()
        .map(|(_, out)| out);
    Ok((
        binary,
        format!("crates.io {} {}", how.name, how.version),
        built_with,
    ))
}

fn download_release_asset(
    out: &mut dyn Write,
    how: &Acquisition,
    into: &Path,
    system: &str,
    arch: &str,
    named: &str,
) -> Result<Assembled, String> {
    let release = find_release(&how.name, &how.version)?;
    let asset = find_asset_for(&release.assets, system, arch)?;
    let downloaded = check_file_name(asset)?;
    print_line(out, &format!("  downloading {downloaded}"))?;
    let archive = into.join(&downloaded);
    download_to(&asset.browser_download_url, &archive)?;
    check_arrived_whole(&release.assets, &downloaded, &archive)?;
    let archive_shown = archive.display().to_string();
    let into_shown = into.display().to_string();
    run_program("tar", &["-xf", &archive_shown, "-C", &into_shown], None).map_err(|refused| {
        format!(
            "{downloaded} could not be unpacked: {}",
            refused.into_words()
        )
    })?;
    let _ = fs::remove_file(&archive);
    let binary = find_file_named(named, into).ok_or_else(|| {
        format!("{downloaded} holds no {named}, so this counter is packed under another name")
    })?;
    Ok((binary, downloaded, None))
}

fn download_release_file(
    out: &mut dyn Write,
    how: &Acquisition,
    into: &Path,
    named: &str,
) -> Result<Assembled, String> {
    let release = find_release(&how.name, &how.version)?;
    let asset = release
        .assets
        .iter()
        .find(|asset| asset.name == named)
        .ok_or_else(|| {
            format!(
                "this release carries no {named}, only {}",
                release
                    .assets
                    .iter()
                    .map(|a| a.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;
    print_line(out, &format!("  downloading {named}"))?;
    let binary = into.join(named);
    download_to(&asset.browser_download_url, &binary)?;
    check_arrived_whole(&release.assets, named, &binary)?;
    mark_runnable(&binary)?;
    Ok((binary, named.to_string(), None))
}

#[cfg(unix)]
fn mark_runnable(binary: &Path) -> Result<(), String> {
    let mut runnable = fs::metadata(binary)
        .map_err(|error| format!("{} could not be read: {error}", binary.display()))?
        .permissions();
    runnable.set_mode(runnable.mode() | 0o755);
    fs::set_permissions(binary, runnable)
        .map_err(|error| format!("{} could not be marked runnable: {error}", binary.display()))
}

#[cfg(not(unix))]
fn mark_runnable(_binary: &Path) -> Result<(), String> {
    Ok(())
}

fn check_file_name(asset: &Asset) -> Result<String, String> {
    let plain = Path::new(&asset.name)
        .file_name()
        .and_then(|name| name.to_str());
    match plain == Some(asset.name.as_str()) {
        true => Ok(asset.name.clone()),
        false => Err(format!(
            "{} is a file name with a path inside it",
            asset.name
        )),
    }
}

fn find_checksums_among(assets: &[Asset]) -> Option<&Asset> {
    assets.iter().find(|asset| {
        let name = asset.name.to_lowercase();
        CHECKSUM_WORDS.iter().any(|word| name.contains(word))
    })
}

fn check_arrived_whole(assets: &[Asset], downloaded: &str, file: &Path) -> Result<(), String> {
    let Some(list) = find_checksums_among(assets) else {
        return Ok(());
    };
    let published = read_url(&list.browser_download_url)?;
    let Some(wanted) = find_checksum_of(downloaded, &published) else {
        return Ok(());
    };
    let arrived = calculate_sha256(file)?;
    match arrived == wanted {
        true => Ok(()),
        false => Err(format!(
            "{downloaded} did not arrive whole: the release says its checksum is {wanted} and \
             the file that arrived is {arrived}"
        )),
    }
}

fn find_release(repository: &str, version: &str) -> Result<Release, String> {
    let mut refused = Vec::new();
    for tag in [format!("v{version}"), version.to_string()] {
        let url = format!("{GITHUB_API}/{repository}/releases/tags/{tag}");
        match read_github_api(&url) {
            Ok(document) => {
                return serde_json::from_str(&document).map_err(|error| {
                    format!("what github answered about {repository} {tag} does not read: {error}")
                });
            }
            Err(message) => refused.push(message),
        }
    }
    refused.dedup();
    Err(explain_lookup_refusal(repository, version, &refused))
}

fn explain_lookup_refusal(repository: &str, version: &str, refused: &[String]) -> String {
    let mentions = |marks: &[&str]| {
        refused.iter().any(|message| {
            let message = message.to_ascii_lowercase();
            marks.iter().any(|mark| message.contains(mark))
        })
    };
    if mentions(&BAD_TOKEN_MARKS) {
        return format!(
            "github refused the token in GITHUB_TOKEN or GH_TOKEN ({}): unset it, or set a valid \
             one",
            refused.join("; ")
        );
    }
    if mentions(&RATE_LIMIT_MARKS) {
        return format!(
            "github refused to say what {repository} has released ({}): anonymous lookups are \
             limited per address, and a token in GITHUB_TOKEN or GH_TOKEN lifts that",
            refused.join("; ")
        );
    }
    format!(
        "{repository} has no release tagged v{version} or {version}: {}",
        refused.join("; ")
    )
}

struct ApiRequest {
    curl: Vec<String>,
    config: Option<String>,
    wget: Vec<String>,
}

fn read_github_api(url: &str) -> Result<String, String> {
    let token = GITHUB_TOKEN_ENVS
        .iter()
        .find_map(|name| env::var(name).ok())
        .filter(|token| !token.trim().is_empty());
    let request = build_api_args(url, token.as_deref());
    let curl: Vec<&str> = request.curl.iter().map(String::as_str).collect();
    let wget: Vec<&str> = request.wget.iter().map(String::as_str).collect();
    download_with_curl_or_wget(&curl, request.config.as_deref(), &wget)
}

fn build_api_args(url: &str, token: Option<&str>) -> ApiRequest {
    let mut curl = vec!["-sSfL".to_string()];
    let mut config = None;
    let mut wget = vec!["-nv".to_string(), "-O-".to_string()];
    if let Some(token) = token {
        curl.push("-K".to_string());
        curl.push("-".to_string());
        config = Some(format!("header = \"Authorization: Bearer {token}\"\n"));
        wget.push(format!("--header=Authorization: Bearer {token}"));
    }
    curl.push(url.to_string());
    wget.push(url.to_string());
    ApiRequest { curl, config, wget }
}

fn find_asset_for<'a>(assets: &'a [Asset], system: &str, arch: &str) -> Result<&'a Asset, String> {
    let system_words: &[&str] = match system {
        "windows" => &["windows", "win"],
        "macos" => &["darwin", "macos", "apple", "osx"],
        "linux" => &["linux"],
        other => {
            return Err(format!(
                "{other} is a system this program cannot pick a file for"
            ));
        }
    };
    let arch_words: &[&str] = match arch {
        "x86_64" => &["x86_64", "amd64", "x64"],
        "arm64" | "aarch64" => &["arm64", "aarch64"],
        other => {
            return Err(format!(
                "{other} is an architecture this program cannot pick a file for"
            ));
        }
    };
    let named =
        |name: &str, words: &[&str]| words.iter().any(|word| name.contains(&format!("_{word}_")));
    let mut fitting: Vec<&Asset> = assets
        .iter()
        .filter(|asset| {
            let name = cut_into_words(&asset.name);
            named(&name, system_words) && named(&name, arch_words)
        })
        .collect();
    fitting.sort_by(|a, b| a.name.cmp(&b.name));
    match fitting.first() {
        Some(asset) => Ok(asset),
        None => Err(format!(
            "this release carries nothing for {system} on {arch}, only {}",
            assets
                .iter()
                .map(|a| a.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

fn cut_into_words(name: &str) -> String {
    let mut cut = String::from("_");
    for letter in name.chars() {
        cut.push(match letter.is_ascii_alphanumeric() {
            true => letter.to_ascii_lowercase(),
            false => '_',
        });
    }
    cut.push('_');
    cut
}

fn find_checksum_of(asset: &str, published: &str) -> Option<String> {
    published.lines().find_map(|line| {
        let (checksum, named) = line.split_once(char::is_whitespace)?;
        (named.trim() == asset).then(|| checksum.to_lowercase())
    })
}

fn find_file_named(named: &str, under: &Path) -> Option<PathBuf> {
    let here = under.join(named);
    if here.is_file() {
        return Some(here);
    }
    fs::read_dir(under)
        .ok()?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .find_map(|dir| find_file_named(named, &dir))
}

fn read_url(url: &str) -> Result<String, String> {
    download_with_curl_or_wget(&["-sSfL", url], None, &["-qO-", url])
}

fn download_to(url: &str, into: &Path) -> Result<(), String> {
    let named = into.display().to_string();
    download_with_curl_or_wget(&["-sSfL", "-o", &named, url], None, &["-qO", &named, url])?;
    Ok(())
}

fn download_with_curl_or_wget(
    curl: &[&str],
    curl_config: Option<&str>,
    wget: &[&str],
) -> Result<String, String> {
    let mut for_curl = vec!["-A", USER_AGENT];
    for_curl.extend_from_slice(curl);
    let mut for_wget = vec!["-U", USER_AGENT];
    for_wget.extend_from_slice(wget);
    let missing = match run_program("curl", &for_curl, curl_config) {
        Ok(printed) => return Ok(printed),
        Err(Refusal::Refused(message)) => return Err(message),
        Err(Refusal::Missing(message)) => message,
    };
    match run_program("wget", &for_wget, None) {
        Ok(printed) => Ok(printed),
        Err(Refusal::Refused(message)) => Err(message),
        Err(Refusal::Missing(_)) => Err(format!(
            "{missing}, and neither is wget, so there is nothing to download with"
        )),
    }
}

fn run_program(program: &str, args: &[&str], feed: Option<&str>) -> Result<String, Refusal> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(match feed {
            Some(_) => Stdio::piped(),
            None => Stdio::null(),
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| match error.kind() == io::ErrorKind::NotFound {
            true => Refusal::Missing(format!("{program} is not on this machine")),
            false => Refusal::Refused(format!("{program} could not be run: {error}")),
        })?;
    if let (Some(text), Some(mut stdin)) = (feed, child.stdin.take()) {
        let _ = stdin.write_all(text.as_bytes());
    }
    let finished = child
        .wait_with_output()
        .map_err(|error| Refusal::Refused(format!("{program} could not be run: {error}")))?;
    if !finished.status.success() {
        let said = String::from_utf8_lossy(&finished.stderr);
        let said = said.trim();
        return Err(Refusal::Refused(match said.is_empty() {
            true => format!("{program} refused"),
            false => said.to_string(),
        }));
    }
    Ok(String::from_utf8_lossy(&finished.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use std::env;

    use super::*;
    use crate::counters::parse_definition;
    use crate::machine::detect_platform;

    #[test]
    fn the_github_api_is_asked_with_the_token_when_there_is_one_and_a_403_names_the_rate_limit() {
        let url = "https://api.github.com/repos/boyter/scc/releases/tags/v4.0.0";
        let request = build_api_args(url, Some("t0k"));
        assert_eq!(request.curl, ["-sSfL", "-K", "-", url]);
        assert_eq!(
            request.config.as_deref(),
            Some("header = \"Authorization: Bearer t0k\"\n")
        );
        assert_eq!(
            request.wget,
            ["-nv", "-O-", "--header=Authorization: Bearer t0k", url]
        );
        let request = build_api_args(url, None);
        assert_eq!(request.curl, ["-sSfL", url]);
        assert_eq!(request.config, None);
        assert_eq!(request.wget, ["-nv", "-O-", url]);
        let limited = ["curl: (22) The requested URL returned error: 403".to_string()];
        assert!(explain_lookup_refusal("boyter/scc", "4.0.0", &limited).contains("GITHUB_TOKEN"));
        let by_wget = ["https://api.github.com/x:\n ERROR 429: Too Many Requests.".to_string()];
        assert!(explain_lookup_refusal("boyter/scc", "4.0.0", &by_wget).contains("GITHUB_TOKEN"));
        let stalled = ["curl: (28) Connection timed out after 21403 milliseconds".to_string()];
        assert!(!explain_lookup_refusal("boyter/scc", "4.0.0", &stalled).contains("GITHUB_TOKEN"));
        let bad_token = ["curl: (22) The requested URL returned error: 401".to_string()];
        assert!(
            explain_lookup_refusal("boyter/scc", "4.0.0", &bad_token).contains("refused the token")
        );
        let missing = ["curl: (22) The requested URL returned error: 404".to_string()];
        assert!(
            explain_lookup_refusal("boyter/scc", "4.0.0", &missing)
                .starts_with("boyter/scc has no release tagged v4.0.0 or 4.0.0: curl")
        );
    }

    const SCC: &str = "\
name = \"scc\"

[acquisition]
channel = \"github-release-asset\"
name    = \"boyter/scc\"
version = \"4.0.0\"

[run]
args      = [\"{target}\"]
languages = [\"-i\", \"{extensions}\"]

[read]
files    = \"Count\"
lines    = \"Lines\"
code     = \"Code\"
comments = \"Comment\"
blanks   = \"Blank\"
";

    #[test]
    fn the_file_for_a_machine_is_picked_out_of_what_a_release_carries() {
        let assets = build_a_release();
        for (system, arch, wanted) in [
            ("windows", "x86_64", "scc_Windows_x86_64.zip"),
            ("windows", "arm64", "scc_Windows_arm64.zip"),
            ("linux", "x86_64", "scc_Linux_x86_64.tar.gz"),
            ("linux", "arm64", "scc_Linux_arm64.tar.gz"),
            ("macos", "x86_64", "scc_Darwin_x86_64.tar.gz"),
            ("macos", "arm64", "scc_Darwin_arm64.tar.gz"),
        ] {
            let picked = find_asset_for(&assets, system, arch).unwrap();
            assert_eq!(picked.name, wanted, "{system} on {arch}");
        }
        let refused = find_asset_for(&assets[..1], "linux", "x86_64").unwrap_err();
        assert!(
            refused.contains("nothing for linux on x86_64, only checksums.txt"),
            "{refused}"
        );
        for crooked in ["../../somewhere-else.zip", "a/b.zip", ""] {
            assert!(
                check_file_name(&build_an_asset(crooked)).is_err(),
                "{crooked}"
            );
        }
    }

    #[test]
    fn a_declared_version_is_matched_whole_and_never_inside_a_longer_number() {
        assert!(is_the_declared_version("4.0.0", "scc version 4.0.0"));
        assert!(is_the_declared_version("3.0.0", "v3.0.0 (unreleased)"));
        assert!(is_the_declared_version(
            "14.0.0",
            "tokei 14.0.0 compiled with serialization support: json"
        ));
        assert!(!is_the_declared_version("4.0.0", "tokei 14.0.0"));
        assert!(!is_the_declared_version("3.0", "v3.0.0"));
    }

    #[test]
    fn the_checksum_of_one_file_is_read_out_of_the_published_list_and_matches_sha256() {
        let published = "9c8f2b1e  scc_Linux_x86_64.tar.gz\nAB12CD34  scc_Windows_x86_64.zip\n";
        assert_eq!(
            find_checksum_of("scc_Windows_x86_64.zip", published).as_deref(),
            Some("ab12cd34")
        );
        assert_eq!(find_checksum_of("scc_Darwin_arm64.tar.gz", published), None);
        assert_eq!(
            find_checksums_among(&build_a_release()).unwrap().name,
            "checksums.txt"
        );
        assert!(find_checksums_among(&[build_an_asset("foo_linux_x86_64.tar.gz")]).is_none());

        let file = env::temp_dir().join("linebench-a_checksum_of_a_known_file");
        fs::write(&file, "abc").unwrap();
        let calculated = calculate_sha256(&file).unwrap();
        fs::remove_file(&file).unwrap();
        assert_eq!(
            calculated,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn a_binary_in_the_counters_dir_is_fetched_or_built_by_its_hash_or_refused() {
        let scc = parse_definition(SCC, Path::new("scc.toml")).unwrap();
        let binary = Path::new("counters/scc.exe");
        let fetched = Fetched {
            channel: Channel::GithubReleaseAsset,
            version: "4.0.0".to_string(),
            source: "scc_Windows_x86_64.zip".to_string(),
            sha256: "abc".to_string(),
            built_with: None,
        };
        let origin = decide_origin(&scc, binary, Some(&fetched), "abc").unwrap();
        assert_eq!(
            origin,
            Origin::Fetched {
                channel: Channel::GithubReleaseAsset,
                version: "4.0.0".to_string(),
                source: "scc_Windows_x86_64.zip".to_string()
            }
        );

        let built = Fetched {
            channel: Channel::CratesIo,
            built_with: Some("rustc 1.97.1".to_string()),
            ..fetched.clone()
        };
        let origin = decide_origin(&scc, binary, Some(&built), "abc").unwrap();
        assert_eq!(
            origin,
            Origin::Built {
                version: "4.0.0".to_string(),
                built_with: "rustc 1.97.1".to_string()
            }
        );

        let behind = Fetched {
            version: "3.7.0".to_string(),
            ..fetched.clone()
        };
        let refused = decide_origin(&scc, binary, Some(&behind), "abc").unwrap_err();
        assert!(
            refused.contains("declares 4.0.0 and what setup fetched is 3.7.0; run setup again"),
            "{refused}"
        );

        let refused = decide_origin(&scc, binary, Some(&fetched), "def").unwrap_err();
        assert!(
            refused.contains("is not what setup fetched, its sha256 differs"),
            "{refused}"
        );
        assert!(refused.contains("--given scc@<tag>=<path>"), "{refused}");

        let refused = decide_origin(&scc, binary, None, "def").unwrap_err();
        assert!(refused.contains("was not fetched by setup"), "{refused}");
    }

    #[test]
    fn a_given_build_is_copied_under_its_instance_name_and_identified_from_the_copy() {
        let definition = parse_definition(
            &SCC.replace(
                "name = \"scc\"",
                "name = \"scc\"\nversion-flag = \"--help\"",
            ),
            Path::new("scc.toml"),
        )
        .unwrap();
        let dir = env::temp_dir().join("linebench-a_given_build_is_staged");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let source = env::current_exe().unwrap();
        let platform = detect_platform().unwrap();
        let identity = stage_given(&definition, "dev", &source, platform, &dir).unwrap();
        let copied = identity.binary.is_file();
        let refused = stage_given(&definition, "dev", &dir.join("nowhere"), platform, &dir);
        fs::remove_dir_all(&dir).unwrap();
        assert!(copied);
        assert_eq!(identity.instance, "scc@dev");
        assert_eq!(identity.counter, "scc");
        assert_eq!(
            identity.origin,
            Origin::Given {
                label: "dev".to_string()
            }
        );
        assert_eq!(identity.sha256, calculate_sha256(&source).unwrap());
        assert!(
            identity
                .binary
                .starts_with(dir.join(GIVEN_DIR).join("scc@dev"))
        );
        assert!(refused.unwrap_err().contains("is not a file"));
    }

    #[test]
    fn a_manifest_round_trips_through_its_file() {
        let dir = env::temp_dir().join("linebench-a_manifest_round_trips");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        assert_eq!(read_manifest(&dir).unwrap(), Manifest::default());
        let mut manifest = Manifest::default();
        manifest.0.insert(
            "tokei".to_string(),
            Fetched {
                channel: Channel::CratesIo,
                version: "14.0.0".to_string(),
                source: "crates.io tokei 14.0.0".to_string(),
                sha256: "abc".to_string(),
                built_with: Some("rustc 1.97.1".to_string()),
            },
        );
        manifest.0.insert(
            "scc".to_string(),
            Fetched {
                channel: Channel::GithubReleaseAsset,
                version: "4.0.0".to_string(),
                source: "scc_Windows_x86_64.zip".to_string(),
                sha256: "def".to_string(),
                built_with: None,
            },
        );
        write_manifest(&dir, &manifest).unwrap();
        let written = fs::read_to_string(dir.join(MANIFEST_FILE)).unwrap();
        let read = read_manifest(&dir).unwrap();
        fs::remove_dir_all(&dir).unwrap();
        assert!(
            written.contains("[scc]") && written.contains("channel = \"crates-io\""),
            "{written}"
        );
        assert_eq!(read, manifest);
    }

    #[test]
    fn the_executable_is_found_whether_the_archive_held_a_directory_or_not() {
        let root = env::temp_dir().join("linebench-an_unpacked_archive");
        let inner = root.join("scc_4.0.0");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&inner).unwrap();
        fs::write(inner.join("scc"), "a binary").unwrap();
        let found = find_file_named("scc", &root);
        let missing = find_file_named("tokei", &root);
        fs::remove_dir_all(&root).unwrap();
        assert_eq!(found.unwrap(), inner.join("scc"));
        assert!(missing.is_none());
    }

    #[cfg(unix)]
    #[test]
    fn a_script_whose_interpreter_is_missing_is_told_apart_from_a_missing_file() {
        use std::os::unix::fs::PermissionsExt;

        let dir = env::temp_dir().join("linebench-a_script_whose_interpreter_is_missing");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let script = dir.join("cloc.pl");
        fs::write(&script, "#!/no-such-interpreter-linebench\nprint 1;\n").unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        let definition = parse_definition(SCC, Path::new("scc.toml")).unwrap();
        let refused = read_version(&definition, &script).unwrap_err();
        assert!(
            refused.contains("the interpreter named on its first line"),
            "{refused}"
        );
        let gone = read_version(&definition, &dir.join("absent")).unwrap_err();
        assert!(
            gone.ends_with("could not be run: no such program"),
            "{gone}"
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    fn build_a_release() -> Vec<Asset> {
        [
            "checksums.txt",
            "scc_Darwin_arm64.tar.gz",
            "scc_Darwin_x86_64.tar.gz",
            "scc_Linux_arm64.tar.gz",
            "scc_Linux_i386.tar.gz",
            "scc_Linux_x86_64.tar.gz",
            "scc_Windows_arm64.zip",
            "scc_Windows_i386.zip",
            "scc_Windows_x86_64.zip",
        ]
        .into_iter()
        .map(build_an_asset)
        .collect()
    }

    fn build_an_asset(name: &str) -> Asset {
        Asset {
            name: name.to_string(),
            browser_download_url: format!("https://example.invalid/{name}"),
        }
    }
}
