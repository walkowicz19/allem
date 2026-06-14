//! The stable `Finding` contract — the single JSON shape consumed identically by the
//! CLI, CI formatters, the MCP server, and LLM agents. Allem's analyzer is deterministic
//! and never bulk-fixes: every finding starts life as a reviewable `candidate`.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Severity of a finding. Ordered so callers can gate on a threshold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

/// Lifecycle of a finding. Findings are surfaced one reviewable unit at a time so a
/// human or LLM can confirm a real issue vs. a false positive before anything changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingStatus {
    /// Default: emitted but not yet triaged.
    #[default]
    Candidate,
    /// A reviewer (human or LLM) confirmed this is a real issue.
    Confirmed,
    /// A reviewer judged this a false positive; excluded from fixes/gates.
    FalsePositive,
    /// A bounded, single-finding fix was applied after confirmation.
    Fixed,
}

/// High-level grouping of what produced the finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    DeadCode,
    Duplication,
    Complexity,
    Boundary,
    /// A syntax/parse error detected while parsing a source file.
    ParseError,
    /// A dangerous-call sink in first-party source (eval/exec/command execution, etc.).
    CodeInjection,
    DependencyHygiene,
    DependencyOutdated,
    DependencyVulnerable,
    DependencyDangerous,
    DependencyInjection,
}

/// A precise source location backing a finding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Location {
    pub path: PathBuf,
    pub line: Option<u32>,
    pub column: Option<u32>,
}

/// Identifies a dependency a finding is about (when applicable).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageRef {
    /// OSV ecosystem name, e.g. "pypi", "crates.io", "npm", "go", "maven", "rubygems".
    pub ecosystem: String,
    pub name: String,
    pub version: String,
}

/// A suggested remediation. It is *never* auto-applied; `confidence` lets a reviewer
/// or LLM weigh it. A fix runs only on explicit, per-finding confirmation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestedAction {
    /// e.g. "upgrade", "remove", "pin", "review".
    #[serde(rename = "type")]
    pub action_type: String,
    /// Target value for the action, e.g. an upgrade version. Free-form by design.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    Low,
    Medium,
    High,
}

/// A single, self-contained, reviewable finding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    /// Stable, deterministic id, e.g. "dep/pypi/requests/CVE-2024-XXXX".
    pub id: String,
    pub category: Category,
    pub severity: Severity,
    /// Human-readable one-line summary.
    pub title: String,
    /// Evidence backing the finding: locations, advisory ids, version deltas, snippets.
    #[serde(default)]
    pub locations: Vec<Location>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub package: Option<PackageRef>,
    /// Free-form, structured evidence (advisory id, fixed_in, matched signal, etc.).
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub evidence: serde_json::Map<String, serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggested_action: Option<SuggestedAction>,
    /// Always false until a confirmed, bounded fix is applied.
    #[serde(default)]
    pub auto_applied: bool,
    #[serde(default)]
    pub status: FindingStatus,
}

impl Finding {
    /// Construct a minimal candidate finding; builder-style setters fill the rest.
    pub fn new(
        id: impl Into<String>,
        category: Category,
        severity: Severity,
        title: impl Into<String>,
    ) -> Self {
        Finding {
            id: id.into(),
            category,
            severity,
            title: title.into(),
            locations: Vec::new(),
            package: None,
            evidence: serde_json::Map::new(),
            suggested_action: None,
            auto_applied: false,
            status: FindingStatus::Candidate,
        }
    }

    pub fn with_package(mut self, package: PackageRef) -> Self {
        self.package = Some(package);
        self
    }

    pub fn with_location(mut self, location: Location) -> Self {
        self.locations.push(location);
        self
    }

    pub fn with_action(mut self, action: SuggestedAction) -> Self {
        self.suggested_action = Some(action);
        self
    }

    pub fn with_evidence(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.evidence.insert(key.into(), value);
        self
    }
}
