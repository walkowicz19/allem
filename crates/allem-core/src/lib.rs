//! `allem-core` — the deterministic heart of Allem.
//!
//! Defines the stable [`Finding`](finding::Finding) contract, the [`Report`](report::Report)
//! aggregate, the [`LanguageAdapter`](adapter::LanguageAdapter) / [`EcosystemAdapter`](adapter::EcosystemAdapter)
//! extension traits, and configuration. No AI lives here — the analyzer is deterministic;
//! agents and humans consume its structured output.

pub mod adapter;
pub mod config;
pub mod error;
pub mod finding;
pub mod report;
pub mod triage;

pub use adapter::{EcosystemAdapter, LanguageAdapter};
pub use config::Config;
pub use error::{AllemError, Result};
pub use finding::{
    Category, Confidence, Finding, FindingStatus, Location, PackageRef, Severity, SuggestedAction,
};
pub use report::{Report, Summary};
pub use triage::TriageStore;

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn finding_serializes_as_candidate_by_default() {
        let f = Finding::new(
            "dep/pypi/requests/CVE-2024-0001",
            Category::DependencyVulnerable,
            Severity::High,
            "requests 2.19.0 is affected by CVE-2024-0001",
        );
        let json = serde_json::to_value(&f).expect("serialize");
        assert_eq!(json["status"], "candidate");
        assert_eq!(json["auto_applied"], false);
        assert_eq!(json["severity"], "high");
    }

    #[test]
    fn summary_counts_by_severity() {
        let findings = vec![
            Finding::new("a", Category::DeadCode, Severity::Low, "a"),
            Finding::new("b", Category::DependencyVulnerable, Severity::High, "b"),
            Finding::new("c", Category::DependencyVulnerable, Severity::High, "c"),
        ];
        let report = Report::new(".", findings);
        assert_eq!(report.summary.total, 3);
        assert_eq!(report.summary.high, 2);
        assert_eq!(report.worst_severity(), Some(Severity::High));
    }
}
