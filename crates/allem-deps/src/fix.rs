//! Bounded, single-finding fixes. Allem never bulk-fixes: each call changes exactly one
//! dependency in one manifest, and only for cases we can do safely and deterministically.
//! Anything else returns `applied: false` with a message so a human/LLM handles it manually.

use allem_core::{AllemError, Result};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct FixOutcome {
    pub applied: bool,
    pub message: String,
}

impl FixOutcome {
    fn not_applied(message: impl Into<String>) -> Self {
        FixOutcome {
            applied: false,
            message: message.into(),
        }
    }
}

/// Upgrade a single package to `to_version` in the appropriate manifest, if supported.
pub fn apply_upgrade(
    root: &Path,
    ecosystem: &str,
    name: &str,
    to_version: &str,
) -> Result<FixOutcome> {
    match ecosystem {
        "PyPI" => upgrade_requirements(root, name, to_version),
        "crates.io" => upgrade_cargo(root, name, to_version),
        other => Ok(FixOutcome::not_applied(format!(
            "no automated fix for ecosystem `{other}` yet — upgrade `{name}` to {to_version} manually"
        ))),
    }
}

/// Upgrade one dependency's version in `Cargo.toml`, preserving formatting via `toml_edit`.
/// Handles both the string form (`name = "1.0"`) and the table form (`name = { version = .. }`).
fn upgrade_cargo(root: &Path, name: &str, to: &str) -> Result<FixOutcome> {
    let path = root.join("Cargo.toml");
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(FixOutcome::not_applied("Cargo.toml not found"));
        }
        Err(source) => return Err(AllemError::Io { path, source }),
    };

    let mut doc =
        text.parse::<toml_edit::DocumentMut>()
            .map_err(|e| AllemError::ManifestParse {
                kind: "Cargo.toml".into(),
                path: path.clone(),
                reason: e.to_string(),
            })?;

    for table in ["dependencies", "dev-dependencies", "build-dependencies"] {
        let Some(deps) = doc.get_mut(table).and_then(|t| t.as_table_like_mut()) else {
            continue;
        };
        let Some(item) = deps.get_mut(name) else {
            continue;
        };
        if item.is_str() {
            *item = toml_edit::value(to);
        } else if let Some(t) = item.as_table_like_mut() {
            if t.get("version").is_some() {
                t.insert("version", toml_edit::value(to));
            } else {
                // git/path dependency without a version — refuse to touch it.
                return Ok(FixOutcome::not_applied(format!(
                    "`{name}` is a git/path dependency; upgrade it manually"
                )));
            }
        }
        write(&path, &doc.to_string())?;
        return Ok(FixOutcome {
            applied: true,
            message: format!("set `{name}` to {to} in Cargo.toml"),
        });
    }

    Ok(FixOutcome::not_applied(format!(
        "`{name}` not found in Cargo.toml dependencies"
    )))
}

fn upgrade_requirements(root: &Path, name: &str, to: &str) -> Result<FixOutcome> {
    let path = root.join("requirements.txt");
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(FixOutcome::not_applied("requirements.txt not found"));
        }
        Err(source) => return Err(AllemError::Io { path, source }),
    };

    let mut changed = false;
    let mut lines: Vec<String> = Vec::with_capacity(text.lines().count());
    for line in text.lines() {
        if !changed && line_package(line).as_deref() == Some(name) {
            lines.push(format!("{name}=={to}"));
            changed = true;
        } else {
            lines.push(line.to_string());
        }
    }

    if !changed {
        return Ok(FixOutcome::not_applied(format!(
            "`{name}` not found as a direct entry in requirements.txt"
        )));
    }

    let mut out = lines.join("\n");
    out.push('\n');
    write(&path, &out)?;
    Ok(FixOutcome {
        applied: true,
        message: format!("pinned `{name}` to {to} in requirements.txt"),
    })
}

/// Extract the leading package name from a requirements line, or None for comments/options.
fn line_package(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('-') {
        return None;
    }
    let end = trimmed
        .find(|c: char| "=<>!~[ #".contains(c))
        .unwrap_or(trimmed.len());
    let name = trimmed[..end].trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

fn write(path: &PathBuf, content: &str) -> Result<()> {
    std::fs::write(path, content).map_err(|source| AllemError::Io {
        path: path.clone(),
        source,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn upgrades_only_the_target_line() {
        let dir = std::env::temp_dir().join(format!("allem-fix-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let req = dir.join("requirements.txt");
        std::fs::write(&req, "requests==2.19.0\nflask>=2,<3\n# note\n").unwrap();

        let outcome = apply_upgrade(&dir, "PyPI", "requests", "2.32.0").unwrap();
        assert!(outcome.applied);

        let after = std::fs::read_to_string(&req).unwrap();
        assert!(after.contains("requests==2.32.0"));
        assert!(after.contains("flask>=2,<3")); // untouched
        assert!(after.contains("# note")); // untouched
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_package_is_not_applied() {
        let dir = std::env::temp_dir().join(format!("allem-fix-miss-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("requirements.txt"), "flask\n").unwrap();
        let outcome = apply_upgrade(&dir, "PyPI", "requests", "2.32.0").unwrap();
        assert!(!outcome.applied);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn unsupported_ecosystem_is_not_applied() {
        let outcome = apply_upgrade(Path::new("."), "npm", "left-pad", "1.0.0").unwrap();
        assert!(!outcome.applied);
    }

    #[test]
    fn upgrades_cargo_string_and_preserves_others() {
        let dir = std::env::temp_dir().join(format!("allem-cargo-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let manifest = "[dependencies]\nserde = \"1.0.100\"\ntokio = { version = \"1.0\", features = [\"full\"] }\n";
        std::fs::write(dir.join("Cargo.toml"), manifest).unwrap();

        let outcome = apply_upgrade(&dir, "crates.io", "serde", "1.0.200").unwrap();
        assert!(outcome.applied);

        let after = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap();
        assert!(after.contains("serde = \"1.0.200\""));
        assert!(after.contains("features = [\"full\"]")); // table dep untouched
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn upgrades_cargo_table_version() {
        let dir = std::env::temp_dir().join(format!("allem-cargo-tbl-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("Cargo.toml"),
            "[dependencies]\ntokio = { version = \"1.0\", features = [\"full\"] }\n",
        )
        .unwrap();

        let outcome = apply_upgrade(&dir, "crates.io", "tokio", "1.40.0").unwrap();
        assert!(outcome.applied);
        let after = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap();
        assert!(after.contains("version = \"1.40.0\""));
        assert!(after.contains("features = [\"full\"]"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
