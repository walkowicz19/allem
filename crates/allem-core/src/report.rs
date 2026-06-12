//! Aggregated analysis output: the list of findings plus a transparent, explainable
//! summary. No opaque ML scoring — counts and a documented severity rollup only.

use crate::finding::{Finding, Severity};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    /// Root path that was analyzed.
    pub root: String,
    /// Languages detected during the run.
    pub languages: Vec<String>,
    /// Dependency ecosystems detected during the run.
    pub ecosystems: Vec<String>,
    pub findings: Vec<Finding>,
    pub summary: Summary,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Summary {
    pub total: usize,
    pub critical: usize,
    pub high: usize,
    pub medium: usize,
    pub low: usize,
    pub info: usize,
}

impl Report {
    pub fn new(root: impl Into<String>, findings: Vec<Finding>) -> Self {
        let summary = Summary::from_findings(&findings);
        Report {
            root: root.into(),
            languages: Vec::new(),
            ecosystems: Vec::new(),
            findings,
            summary,
        }
    }

    /// Highest severity present, if any — useful for CI gating.
    pub fn worst_severity(&self) -> Option<Severity> {
        self.findings.iter().map(|f| f.severity).max()
    }

    /// Highest severity among *actionable* findings — those not dismissed as false positives
    /// or already fixed. This is what CI gating should use so triaged noise doesn't fail builds.
    pub fn worst_actionable_severity(&self) -> Option<Severity> {
        self.findings
            .iter()
            .filter(|f| {
                !matches!(
                    f.status,
                    crate::FindingStatus::FalsePositive | crate::FindingStatus::Fixed
                )
            })
            .map(|f| f.severity)
            .max()
    }
}

impl Summary {
    pub fn from_findings(findings: &[Finding]) -> Self {
        let mut s = Summary {
            total: findings.len(),
            ..Default::default()
        };
        for f in findings {
            match f.severity {
                Severity::Critical => s.critical += 1,
                Severity::High => s.high += 1,
                Severity::Medium => s.medium += 1,
                Severity::Low => s.low += 1,
                Severity::Info => s.info += 1,
            }
        }
        s
    }
}
