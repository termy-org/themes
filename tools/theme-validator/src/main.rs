use std::{
    env, fs,
    path::{Path, PathBuf},
};

use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const ANSI_COLOR_NAMES: [&str; 16] = [
    "black",
    "red",
    "green",
    "yellow",
    "blue",
    "magenta",
    "cyan",
    "white",
    "bright_black",
    "bright_red",
    "bright_green",
    "bright_yellow",
    "bright_blue",
    "bright_magenta",
    "bright_cyan",
    "bright_white",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ThemeIndex {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    themes: Vec<IndexTheme>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IndexTheme {
    name: String,
    slug: String,
    #[serde(default)]
    description: String,
    latest_version: String,
    file: String,
    checksum_sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Metadata {
    name: String,
    slug: String,
    #[serde(default)]
    description: String,
    latest_version: String,
    #[serde(default)]
    versions: Vec<MetadataVersion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MetadataVersion {
    version: String,
    file: String,
    checksum_sha256: Option<String>,
}

fn main() {
    let repo = parse_repo_arg().unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(2);
    });

    let mut errors = Vec::new();
    let generated = build_index(&repo, &mut errors);

    if let Some(generated) = generated {
        match fs::read_to_string(repo.join("index.json")) {
            Ok(contents) => match serde_json::from_str::<ThemeIndex>(&contents) {
                Ok(index) if index == generated => {}
                Ok(_) => errors.push("index.json is stale or does not match metadata".to_string()),
                Err(error) => errors.push(format!("index.json is invalid: {error}")),
            },
            Err(error) => errors.push(format!("failed to read index.json: {error}")),
        }
    }

    if errors.is_empty() {
        println!("Theme registry is valid");
    } else {
        eprintln!("Theme registry is invalid");
        for error in errors {
            eprintln!("  {error}");
        }
        std::process::exit(1);
    }
}

fn parse_repo_arg() -> Result<PathBuf, String> {
    let mut args = env::args().skip(1);
    match (args.next().as_deref(), args.next()) {
        (Some("--repo"), Some(path)) => Ok(PathBuf::from(path)),
        _ => Err("usage: theme-validator --repo <path>".to_string()),
    }
}

fn build_index(repo: &Path, errors: &mut Vec<String>) -> Option<ThemeIndex> {
    let themes_dir = repo.join("themes");
    let entries = match fs::read_dir(&themes_dir) {
        Ok(entries) => entries,
        Err(error) => {
            errors.push(format!("failed to read themes directory: {error}"));
            return None;
        }
    };

    let mut themes = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let dir_slug = entry.file_name().to_string_lossy().to_string();
        let metadata_path = path.join("metadata.json");
        let metadata = match read_metadata(&metadata_path, errors) {
            Some(metadata) => metadata,
            None => continue,
        };

        validate_metadata(repo, &dir_slug, &metadata, errors);
        if let Some(version) = metadata
            .versions
            .iter()
            .find(|version| version.version == metadata.latest_version)
        {
            themes.push(IndexTheme {
                name: metadata.name,
                slug: metadata.slug.clone(),
                description: metadata.description,
                latest_version: metadata.latest_version,
                file: format!("themes/{}/{}", metadata.slug, version.file),
                checksum_sha256: version.checksum_sha256.clone(),
            });
        }
    }

    themes.sort_unstable_by(|left, right| {
        left.name
            .to_ascii_lowercase()
            .cmp(&right.name.to_ascii_lowercase())
            .then_with(|| left.slug.cmp(&right.slug))
    });

    Some(ThemeIndex { version: 1, themes })
}

fn read_metadata(path: &Path, errors: &mut Vec<String>) -> Option<Metadata> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) => {
            errors.push(format!("failed to read {}: {error}", path.display()));
            return None;
        }
    };

    match serde_json::from_str(&contents) {
        Ok(metadata) => Some(metadata),
        Err(error) => {
            errors.push(format!("failed to parse {}: {error}", path.display()));
            None
        }
    }
}

fn validate_metadata(repo: &Path, dir_slug: &str, metadata: &Metadata, errors: &mut Vec<String>) {
    if metadata.slug != normalize_slug(&metadata.slug) {
        errors.push(format!("themes/{dir_slug}/metadata.json has invalid slug"));
    }
    if metadata.slug != dir_slug {
        errors.push(format!(
            "themes/{dir_slug}/metadata.json slug must match directory"
        ));
    }
    if metadata.name.trim().is_empty() {
        errors.push(format!("themes/{dir_slug}/metadata.json name is required"));
    }
    if !metadata
        .versions
        .iter()
        .any(|version| version.version == metadata.latest_version)
    {
        errors.push(format!(
            "themes/{dir_slug}/metadata.json latestVersion is missing from versions"
        ));
    }

    for version in &metadata.versions {
        if let Err(error) = Version::parse(&version.version) {
            errors.push(format!(
                "themes/{dir_slug}/metadata.json version '{}' is invalid: {error}",
                version.version
            ));
        }

        let file_path = repo.join("themes").join(dir_slug).join(&version.file);
        let contents = match fs::read_to_string(&file_path) {
            Ok(contents) => contents,
            Err(error) => {
                errors.push(format!("failed to read {}: {error}", file_path.display()));
                continue;
            }
        };

        validate_theme_json(&file_path, &contents, errors);
        if let Some(expected) = &version.checksum_sha256 {
            let actual = sha256_hex(contents.as_bytes());
            if !expected.eq_ignore_ascii_case(&actual) {
                errors.push(format!(
                    "{} checksum mismatch: expected {expected}, got {actual}",
                    file_path.display()
                ));
            }
        }
    }
}

fn validate_theme_json(path: &Path, contents: &str, errors: &mut Vec<String>) {
    let value = match serde_json::from_str::<serde_json::Value>(contents) {
        Ok(value) => value,
        Err(error) => {
            errors.push(format!("{} is invalid JSON: {error}", path.display()));
            return;
        }
    };
    let Some(object) = value.as_object() else {
        errors.push(format!("{} must be a JSON object", path.display()));
        return;
    };

    for key in ["foreground", "background", "cursor"]
        .into_iter()
        .chain(ANSI_COLOR_NAMES)
    {
        let Some(value) = object.get(key).and_then(|value| value.as_str()) else {
            errors.push(format!("{} is missing color '{key}'", path.display()));
            continue;
        };
        if !is_hex_color(value) {
            errors.push(format!("{} color '{key}' must be #RRGGBB", path.display()));
        }
    }
}

fn is_hex_color(value: &str) -> bool {
    value.len() == 7
        && value.starts_with('#')
        && value[1..]
            .chars()
            .all(|character| character.is_ascii_hexdigit())
}

fn normalize_slug(value: &str) -> String {
    let mut normalized = String::new();
    let mut last_dash = false;
    for character in value
        .trim()
        .chars()
        .map(|character| character.to_ascii_lowercase())
    {
        match character {
            'a'..='z' | '0'..='9' => {
                normalized.push(character);
                last_dash = false;
            }
            '-' | '_' | ' ' => {
                if !normalized.is_empty() && !last_dash {
                    normalized.push('-');
                    last_dash = true;
                }
            }
            _ => {}
        }
    }
    while normalized.ends_with('-') {
        normalized.pop();
    }
    normalized
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}
