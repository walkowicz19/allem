//! Grammar-agnostic deterministic analyses over a parsed tree. Currently: per-function
//! cyclomatic complexity and long-function detection. No AI, no type info — syntactic only,
//! so results are reproducible (mirrors fallow's deterministic stance).

use crate::LangSpec;
use allem_core::{
    AllemError, Category, Confidence, Finding, Location, Result, Severity, SuggestedAction,
};
use std::path::Path;
use tree_sitter::{Language, Node, Parser};

/// Tunable limits above which a function is reported.
#[derive(Clone)]
pub struct ComplexityThresholds {
    /// Cyclomatic complexity that triggers a `medium` finding.
    pub warn: u32,
    /// Complexity that triggers a `high` finding.
    pub high: u32,
    /// Function length (lines) that triggers a `long function` finding.
    pub max_lines: u32,
}

impl Default for ComplexityThresholds {
    fn default() -> Self {
        ComplexityThresholds {
            warn: 10,
            high: 20,
            max_lines: 80,
        }
    }
}

pub fn analyze(
    spec: &LangSpec,
    language: &Language,
    thresholds: &ComplexityThresholds,
    path: &Path,
    source: &str,
) -> Result<Vec<Finding>> {
    let mut parser = Parser::new();
    parser
        .set_language(language)
        .map_err(|e| AllemError::Config(format!("tree-sitter language for {}: {e}", spec.id)))?;
    let Some(tree) = parser.parse(source.as_bytes(), None) else {
        return Ok(Vec::new()); // unparsable input → no findings, never panic
    };

    let mut findings = Vec::new();
    let root = tree.root_node();
    if root.has_error() {
        if let Some(f) = parse_error(spec, root, path) {
            findings.push(f);
        }
    }
    visit(spec, root, source, path, thresholds, &mut findings);
    Ok(findings)
}

/// Emit a single `parse_error` finding at the first ERROR/MISSING node in the tree, if any.
/// One per file keeps it actionable rather than flooding on a single broken construct.
fn parse_error(spec: &LangSpec, root: Node, path: &Path) -> Option<Finding> {
    let node = first_error(root)?;
    let line = node.start_position().row as u32 + 1;
    let kind = if node.is_missing() {
        "missing"
    } else {
        "unexpected"
    };
    Some(
        Finding::new(
            format!("parse-error/{}/{}:{}", spec.id, path.display(), line),
            Category::ParseError,
            Severity::Medium,
            format!(
                "syntax error in {} ({kind} syntax near line {line})",
                spec.id
            ),
        )
        .with_location(Location {
            path: path.to_path_buf(),
            line: Some(line),
            column: None,
        })
        .with_evidence("language", spec.id.into())
        .with_action(SuggestedAction {
            action_type: "review".into(),
            to: None,
            confidence: Confidence::High,
        }),
    )
}

/// First ERROR/MISSING node in DFS pre-order (≈ source order).
fn first_error(node: Node) -> Option<Node> {
    if node.is_error() || node.is_missing() {
        return Some(node);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(found) = first_error(child) {
            return Some(found);
        }
    }
    None
}

/// Depth-first walk: at each node, run complexity (on functions) and injection-sink (on calls)
/// checks, then recurse.
fn visit(
    spec: &LangSpec,
    node: Node,
    source: &str,
    path: &Path,
    thresholds: &ComplexityThresholds,
    out: &mut Vec<Finding>,
) {
    if spec.function_kinds.contains(&node.kind()) {
        if let Some(f) = measure_function(spec, node, source, path, thresholds) {
            out.extend(f);
        }
    }
    if spec.call_kinds.contains(&node.kind()) {
        if let Some(f) = check_sink(spec, node, source, path) {
            out.push(f);
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        visit(spec, child, source, path, thresholds, out);
    }
}

/// Flag a call whose callee matches a known dangerous sink for this language.
fn check_sink(spec: &LangSpec, node: Node, source: &str, path: &Path) -> Option<Finding> {
    if spec.sinks.is_empty() {
        return None;
    }
    let callee_node = match spec.callee_field {
        Some(field) => node.child_by_field_name(field)?,
        None => node.child(0)?,
    };
    let callee = callee_node.utf8_text(source.as_bytes()).ok()?;
    let sink = spec.sinks.iter().find(|s| matches_sink(callee, s))?;

    let line = node.start_position().row as u32 + 1;
    Some(
        Finding::new(
            format!("inject/{}/{}:{}", spec.id, sink, line),
            Category::CodeInjection,
            Severity::High,
            format!(
                "dangerous call `{callee}` (matches sink `{sink}`) in {}",
                spec.id
            ),
        )
        .with_location(Location {
            path: path.to_path_buf(),
            line: Some(line),
            column: None,
        })
        .with_evidence("sink", (*sink).into())
        .with_evidence("callee", callee.into())
        .with_evidence("language", spec.id.into())
        .with_action(SuggestedAction {
            action_type: "review".into(),
            to: None,
            confidence: Confidence::Medium,
        }),
    )
}

/// A callee matches a sink pattern if it is exactly the pattern, or ends with a member/path
/// access to it (`.pattern` or `::pattern`) — so `os.system` matches both bare and qualified.
fn matches_sink(callee: &str, pattern: &str) -> bool {
    callee == pattern
        || callee.ends_with(&format!(".{pattern}"))
        || callee.ends_with(&format!("::{pattern}"))
}

fn measure_function(
    spec: &LangSpec,
    node: Node,
    source: &str,
    path: &Path,
    thresholds: &ComplexityThresholds,
) -> Option<Vec<Finding>> {
    let name = spec
        .name_field
        .and_then(|field| node.child_by_field_name(field))
        .and_then(|n| n.utf8_text(source.as_bytes()).ok())
        .unwrap_or("<anonymous>")
        .to_string();

    let line = node.start_position().row as u32 + 1;
    let lines = (node.end_position().row - node.start_position().row) as u32 + 1;
    let complexity = 1 + count_decisions(spec, node);

    let mut findings = Vec::new();
    let location = || Location {
        path: path.to_path_buf(),
        line: Some(line),
        column: None,
    };

    if complexity >= thresholds.warn {
        let severity = if complexity >= thresholds.high {
            Severity::High
        } else {
            Severity::Medium
        };
        findings.push(
            Finding::new(
                format!("complexity/{}/{}:{}", spec.id, name, line),
                Category::Complexity,
                severity,
                format!(
                    "`{name}` has cyclomatic complexity {complexity} (limit {})",
                    thresholds.warn
                ),
            )
            .with_location(location())
            .with_evidence("complexity", complexity.into())
            .with_evidence("language", spec.id.into())
            .with_action(SuggestedAction {
                action_type: "review".into(),
                to: None,
                confidence: Confidence::Medium,
            }),
        );
    }

    if lines > thresholds.max_lines {
        findings.push(
            Finding::new(
                format!("long-function/{}/{}:{}", spec.id, name, line),
                Category::Complexity,
                Severity::Low,
                format!(
                    "`{name}` is {lines} lines long (limit {})",
                    thresholds.max_lines
                ),
            )
            .with_location(location())
            .with_evidence("lines", lines.into())
            .with_action(SuggestedAction {
                action_type: "review".into(),
                to: None,
                confidence: Confidence::Low,
            }),
        );
    }

    if findings.is_empty() {
        None
    } else {
        Some(findings)
    }
}

/// Count decision-point nodes anywhere inside `node` (its own kind excluded).
fn count_decisions(spec: &LangSpec, node: Node) -> u32 {
    let mut count = 0;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if spec.decision_kinds.contains(&child.kind()) {
            count += 1;
        }
        count += count_decisions(spec, child);
    }
    count
}
