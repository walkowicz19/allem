//! Python ecosystem adapter. Parses `requirements.txt` into normalized packages and runs
//! the shared checks. (pyproject/poetry/Pipfile land later — same `Package` model.)

use crate::model::{Package, VersionSpec};
use std::path::Path;

pub const ECOSYSTEM: &str = "PyPI";

/// Parse a `requirements.txt` body into packages. Pure and offline — unit-testable.
pub fn parse_requirements(manifest: &str, text: &str) -> Vec<Package> {
    let mut packages = Vec::new();
    for (idx, raw) in text.lines().enumerate() {
        let line = strip_comment(raw).trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('-') {
            // Skip blank lines and pip options (-r, -e handled as URL/VCS below if direct).
            if !(line.starts_with("-e ") || line.contains("git+")) {
                continue;
            }
        }
        let line_no = Some((idx + 1) as u32);

        // VCS / URL direct references.
        if line.contains("git+") {
            if let Some(name) = vcs_egg_name(line) {
                packages.push(Package {
                    ecosystem: ECOSYSTEM,
                    name,
                    version: VersionSpec::Vcs(line.trim_start_matches("-e ").trim().to_string()),
                    manifest: manifest.to_string(),
                    line: line_no,
                });
            }
            continue;
        }

        if let Some((name, spec)) = split_requirement(line) {
            packages.push(Package {
                ecosystem: ECOSYSTEM,
                name,
                version: spec,
                manifest: manifest.to_string(),
                line: line_no,
            });
        }
    }
    packages
}

pub fn detect(root: &Path) -> bool {
    root.join("requirements.txt").is_file()
}

fn strip_comment(line: &str) -> &str {
    match line.find(" #") {
        Some(i) => &line[..i],
        None => line,
    }
}

fn vcs_egg_name(line: &str) -> Option<String> {
    line.split("#egg=")
        .nth(1)
        .map(|s| s.trim().to_string())
        .or_else(|| Some("unknown".to_string()))
}

/// Split `name==1.2.3` / `name>=1,<2` / `name` into (name, VersionSpec).
fn split_requirement(line: &str) -> Option<(String, VersionSpec)> {
    let operators = ["==", ">=", "<=", "~=", "!=", ">", "<"];
    for op in operators {
        if let Some(pos) = line.find(op) {
            let name = line[..pos].trim().to_string();
            let rest = line[pos..].trim().to_string();
            if name.is_empty() {
                return None;
            }
            let spec = if op == "==" && !rest[2..].contains(['*', ',']) {
                VersionSpec::Pinned(rest[2..].trim().to_string())
            } else {
                VersionSpec::Range(rest)
            };
            return Some((sanitize_name(&name), spec));
        }
    }
    // Bare name with no constraint → wildcard (accepts any version).
    let name = sanitize_name(line);
    if name.is_empty() {
        None
    } else {
        Some((name, VersionSpec::Wildcard))
    }
}

/// Drop extras like `requests[security]` → `requests`.
fn sanitize_name(name: &str) -> String {
    name.split('[').next().unwrap_or(name).trim().to_string()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn parses_pinned_range_and_bare() {
        let text = "requests==2.31.0\nflask>=2,<3\nnumpy\n# comment\n";
        let pkgs = parse_requirements("requirements.txt", text);
        assert_eq!(pkgs.len(), 3);
        assert_eq!(pkgs[0].name, "requests");
        assert_eq!(pkgs[0].version, VersionSpec::Pinned("2.31.0".into()));
        assert!(matches!(pkgs[1].version, VersionSpec::Range(_)));
        assert_eq!(pkgs[2].version, VersionSpec::Wildcard);
    }

    #[test]
    fn parses_vcs_dependency() {
        let text = "-e git+https://example.com/pkg.git#egg=pkg\n";
        let pkgs = parse_requirements("requirements.txt", text);
        assert_eq!(pkgs.len(), 1);
        assert!(matches!(pkgs[0].version, VersionSpec::Vcs(_)));
        assert_eq!(pkgs[0].name, "pkg");
    }
}
