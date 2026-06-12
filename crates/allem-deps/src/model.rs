//! Normalized representation shared by every ecosystem adapter, so the danger/outdated/
//! vulnerable checks are written once and reused (clean-code: DRY).

/// How a dependency's version was specified — central to risk scoring.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionSpec {
    /// A concrete, resolvable version, e.g. "2.31.0".
    Pinned(String),
    /// A range/constraint, e.g. "^1.0", ">=2,<3".
    Range(String),
    /// Wildcard / "any" — accepts anything (`*`, empty, `latest`).
    Wildcard,
    /// Sourced from a VCS URL (git/hg) — bypasses the registry.
    Vcs(String),
    /// Sourced from an arbitrary URL or local path — bypasses the registry.
    Url(String),
}

impl VersionSpec {
    /// The concrete version if known, for registry/OSV lookups.
    pub fn concrete(&self) -> Option<&str> {
        match self {
            VersionSpec::Pinned(v) => Some(v),
            _ => None,
        }
    }

    pub fn raw(&self) -> &str {
        match self {
            VersionSpec::Pinned(v)
            | VersionSpec::Range(v)
            | VersionSpec::Vcs(v)
            | VersionSpec::Url(v) => v,
            VersionSpec::Wildcard => "*",
        }
    }
}

/// A single declared dependency, normalized across ecosystems.
#[derive(Debug, Clone)]
pub struct Package {
    /// OSV ecosystem id, e.g. "PyPI", "crates.io".
    pub ecosystem: &'static str,
    pub name: String,
    pub version: VersionSpec,
    /// Source manifest path (display form) and 1-based line, for evidence.
    pub manifest: String,
    pub line: Option<u32>,
}

/// A vulnerability advisory affecting a package (from OSV or another source).
#[derive(Debug, Clone)]
pub struct Advisory {
    /// Canonical id, e.g. "CVE-2024-0001" or "GHSA-xxxx".
    pub id: String,
    pub summary: String,
    /// First fixed version, if the source reports one.
    pub fixed_in: Option<String>,
    /// OSV severity label if present ("CRITICAL"/"HIGH"/...), else None.
    pub severity: Option<String>,
}
