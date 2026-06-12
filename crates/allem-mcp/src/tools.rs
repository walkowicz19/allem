//! Tool definitions and execution. The MCP layer is a thin surface over the same engine the
//! CLI uses, so agents get the identical `Finding` contract (clean-code: DRY).
//!
//! Tools are triage-oriented: agents inspect findings, record a verdict, and apply at most one
//! bounded fix at a time — matching Allem's never-fix-everything-at-once design.

use allem_core::{Category, Config, Finding, FindingStatus, Report, Severity};
use serde_json::{json, Value};
use std::path::Path;

/// JSON-Schema descriptors advertised via `tools/list`.
pub fn definitions() -> Value {
    json!([
        {
            "name": "analyze_repo",
            "description": "Analyze a repository and return the full deterministic report \
                            (detected ecosystems + all dependency findings).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Repository root to analyze." }
                },
                "required": ["path"]
            }
        },
        {
            "name": "list_dependency_risks",
            "description": "List dependency-safety findings (outdated/vulnerable/dangerous/\
                            injection), optionally filtered to a minimum severity.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "min_severity": {
                        "type": "string",
                        "enum": ["info", "low", "medium", "high", "critical"],
                        "description": "Only return findings at or above this severity."
                    }
                },
                "required": ["path"]
            }
        },
        {
            "name": "explain_finding",
            "description": "Return the full evidence bundle for a single finding id, for triage \
                            (confirm vs. false positive).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "id": { "type": "string", "description": "The finding id to explain." }
                },
                "required": ["path", "id"]
            }
        },
        {
            "name": "confirm_finding",
            "description": "Record a triage verdict for a finding: confirm it as real, or \
                            dismiss it as a false positive. Persists across runs and feeds gating.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "id": { "type": "string" },
                    "verdict": {
                        "type": "string",
                        "enum": ["confirmed", "false_positive"],
                        "description": "Whether the finding is a real issue or a false positive."
                    }
                },
                "required": ["path", "id", "verdict"]
            }
        },
        {
            "name": "apply_fix",
            "description": "Apply a bounded fix for exactly ONE finding (e.g. a dependency \
                            upgrade) and mark it fixed. Refuses anything broader than a single \
                            finding.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "id": { "type": "string" }
                },
                "required": ["path", "id"]
            }
        }
    ])
}

/// Execute a tool by name. Returns the JSON payload to embed as text content, or an error
/// message string (surfaced to the agent as an `isError` tool result, not a transport error).
pub fn call(name: &str, args: &Value) -> std::result::Result<Value, String> {
    match name {
        "analyze_repo" => {
            let report = run(args)?;
            to_value(&report)
        }
        "list_dependency_risks" => {
            let report = run(args)?;
            let min = parse_min_severity(args)?;
            let risks: Vec<&Finding> = report
                .findings
                .iter()
                .filter(|f| is_dependency(f.category) && f.severity >= min)
                .collect();
            Ok(json!({ "count": risks.len(), "findings": risks }))
        }
        "explain_finding" => {
            let report = run(args)?;
            let id = arg_str(args, "id")?;
            match report.findings.iter().find(|f| f.id == id) {
                Some(f) => to_value(f),
                None => Err(format!("no finding with id '{id}'")),
            }
        }
        "confirm_finding" => {
            let root = arg_str(args, "path")?;
            let id = arg_str(args, "id")?;
            let status = match arg_str(args, "verdict")?.as_str() {
                "confirmed" => FindingStatus::Confirmed,
                "false_positive" => FindingStatus::FalsePositive,
                other => return Err(format!("invalid verdict '{other}'")),
            };
            allem_engine::set_verdict(Path::new(&root), &id, status).map_err(|e| e.to_string())?;
            Ok(json!({ "id": id, "status": to_value(&status)? }))
        }
        "apply_fix" => {
            let root = arg_str(args, "path")?;
            let id = arg_str(args, "id")?;
            let config = Config::load(Path::new(&root)).map_err(|e| e.to_string())?;
            let outcome = allem_engine::apply_fix(Path::new(&root), &config, &id)
                .map_err(|e| e.to_string())?;
            Ok(json!({ "applied": outcome.applied, "message": outcome.message }))
        }
        other => Err(format!("unknown tool '{other}'")),
    }
}

fn run(args: &Value) -> std::result::Result<Report, String> {
    let path = arg_str(args, "path")?;
    let root = Path::new(&path);
    let config = Config::load(root).map_err(|e| e.to_string())?;
    allem_engine::analyze_report(root, &config).map_err(|e| e.to_string())
}

fn arg_str(args: &Value, key: &str) -> std::result::Result<String, String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| format!("missing required string argument '{key}'"))
}

fn parse_min_severity(args: &Value) -> std::result::Result<Severity, String> {
    match args.get("min_severity") {
        None | Some(Value::Null) => Ok(Severity::Info),
        Some(v) => serde_json::from_value::<Severity>(v.clone())
            .map_err(|_| "invalid min_severity".to_string()),
    }
}

fn is_dependency(category: Category) -> bool {
    matches!(
        category,
        Category::DependencyHygiene
            | Category::DependencyOutdated
            | Category::DependencyVulnerable
            | Category::DependencyDangerous
            | Category::DependencyInjection
    )
}

fn to_value<T: serde::Serialize>(v: &T) -> std::result::Result<Value, String> {
    serde_json::to_value(v).map_err(|e| e.to_string())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn definitions_list_all_tools() {
        let defs = definitions();
        let names: Vec<&str> = defs
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert_eq!(
            names,
            [
                "analyze_repo",
                "list_dependency_risks",
                "explain_finding",
                "confirm_finding",
                "apply_fix",
            ]
        );
    }

    #[test]
    fn unknown_tool_errors() {
        assert!(call("nope", &json!({})).is_err());
    }

    #[test]
    fn missing_path_errors() {
        assert!(call("analyze_repo", &json!({})).is_err());
    }
}
