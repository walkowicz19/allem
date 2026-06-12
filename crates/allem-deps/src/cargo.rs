//! Rust ecosystem adapter. Parses `Cargo.toml` `[dependencies]` (string and table forms)
//! into normalized packages.

use crate::model::{Package, VersionSpec};
use allem_core::{AllemError, Result};
use std::path::{Path, PathBuf};

pub const ECOSYSTEM: &str = "crates.io";

pub fn detect(root: &Path) -> bool {
    root.join("Cargo.toml").is_file()
}

/// Parse a `Cargo.toml` body. Pure and offline — unit-testable.
pub fn parse_cargo_toml(manifest: &str, text: &str) -> Result<Vec<Package>> {
    let doc: toml::Value = text
        .parse::<toml::Value>()
        .map_err(|e| AllemError::ManifestParse {
            kind: "Cargo.toml".into(),
            path: PathBuf::from(manifest),
            reason: e.to_string(),
        })?;

    let mut packages = Vec::new();
    for table in ["dependencies", "dev-dependencies", "build-dependencies"] {
        let Some(deps) = doc.get(table).and_then(|d| d.as_table()) else {
            continue;
        };
        for (name, spec) in deps {
            packages.push(Package {
                ecosystem: ECOSYSTEM,
                name: name.clone(),
                version: classify(spec),
                manifest: manifest.to_string(),
                line: None, // toml crate doesn't expose spans; left for a future span parser.
            });
        }
    }
    Ok(packages)
}

fn classify(spec: &toml::Value) -> VersionSpec {
    match spec {
        toml::Value::String(s) => caret_or_pinned(s),
        toml::Value::Table(t) => {
            if let Some(g) = t.get("git").and_then(|v| v.as_str()) {
                VersionSpec::Vcs(g.to_string())
            } else if let Some(p) = t.get("path").and_then(|v| v.as_str()) {
                VersionSpec::Url(p.to_string())
            } else if let Some(v) = t.get("version").and_then(|v| v.as_str()) {
                caret_or_pinned(v)
            } else {
                VersionSpec::Wildcard
            }
        }
        _ => VersionSpec::Wildcard,
    }
}

/// In Cargo, a bare `"1.2.3"` is really `^1.2.3` (a range). Only `=1.2.3` is a true pin.
/// `*` is a wildcard.
fn caret_or_pinned(s: &str) -> VersionSpec {
    let s = s.trim();
    if s == "*" || s.is_empty() {
        VersionSpec::Wildcard
    } else if let Some(rest) = s.strip_prefix('=') {
        VersionSpec::Pinned(rest.trim().to_string())
    } else {
        VersionSpec::Range(s.to_string())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn parses_string_and_table_deps() {
        let text = r#"
[dependencies]
serde = "1.0"
tokio = { version = "1", features = ["full"] }
local = { path = "../local" }
forked = { git = "https://example.com/forked.git" }
anything = "*"
exact = "=1.2.3"
"#;
        let pkgs = parse_cargo_toml("Cargo.toml", text).unwrap();
        let by = |n: &str| pkgs.iter().find(|p| p.name == n).unwrap().version.clone();
        assert!(matches!(by("serde"), VersionSpec::Range(_)));
        assert!(matches!(by("local"), VersionSpec::Url(_)));
        assert!(matches!(by("forked"), VersionSpec::Vcs(_)));
        assert!(matches!(by("anything"), VersionSpec::Wildcard));
        assert_eq!(by("exact"), VersionSpec::Pinned("1.2.3".into()));
    }
}
