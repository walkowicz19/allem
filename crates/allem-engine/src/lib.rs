//! `allem-engine` — the orchestrator. Sits above the code-intelligence (`allem-lang`) and
//! dependency-intelligence (`allem-deps`) layers and assembles one [`Report`]. The CLI and the
//! MCP server both call into it, so every surface emits the identical contract (clean-code:
//! DRY; matches the plan's architecture diagram). It also drives the triage lifecycle:
//! recorded verdicts are re-applied to findings, and fixes are bounded to a single finding.

use allem_core::{Config, FindingStatus, Report, Result, TriageStore};
use std::path::Path;

pub use allem_deps::fix::FixOutcome;

/// Run the full analysis for `root`: polyglot code intelligence + dependency safety, merged,
/// with any recorded triage verdicts stamped onto matching findings.
pub fn analyze_report(root: &Path, config: &Config) -> Result<Report> {
    // Dependency intelligence (outdated/vulnerable/dangerous/injection).
    let ecosystems = allem_deps::detect_ecosystems(root);
    let mut findings = allem_deps::analyze(root, config)?;

    // Polyglot code intelligence (complexity, long functions across languages).
    let lang = allem_lang::analyze_tree(root)?;
    findings.extend(lang.findings);

    // Re-apply stored triage verdicts (confirmed / false_positive / fixed).
    TriageStore::load(root)?.apply(&mut findings);

    let mut report = Report::new(root.display().to_string(), findings);
    report.ecosystems = ecosystems.into_iter().map(String::from).collect();
    report.languages = lang.languages.into_iter().map(String::from).collect();
    Ok(report)
}

/// Record a triage verdict for one finding id (persists to `.allem/triage.json`).
pub fn set_verdict(root: &Path, id: &str, status: FindingStatus) -> Result<()> {
    let mut store = TriageStore::load(root)?;
    store.set(id, status);
    store.save(root)
}

/// Apply a bounded fix for a single finding id. Re-analyzes to locate the finding, dispatches
/// to the appropriate fixer, and — on success — records the finding as `fixed`. Never touches
/// any other finding.
pub fn apply_fix(root: &Path, config: &Config, id: &str) -> Result<FixOutcome> {
    let report = analyze_report(root, config)?;
    let Some(finding) = report.findings.iter().find(|f| f.id == id) else {
        return Ok(FixOutcome {
            applied: false,
            message: format!("no finding with id `{id}`"),
        });
    };

    // The only bounded automated fix today: upgrade a dependency to its suggested version.
    let outcome = match (&finding.package, &finding.suggested_action) {
        (Some(pkg), Some(action)) if action.action_type == "upgrade" => match &action.to {
            Some(to) => allem_deps::fix::apply_upgrade(root, &pkg.ecosystem, &pkg.name, to)?,
            None => FixOutcome {
                applied: false,
                message: "upgrade action has no target version".into(),
            },
        },
        _ => FixOutcome {
            applied: false,
            message: format!("no bounded automated fix for `{id}` — review and resolve manually"),
        },
    };

    if outcome.applied {
        set_verdict(root, id, FindingStatus::Fixed)?;
    }
    Ok(outcome)
}
