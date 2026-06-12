//! Cross-file dead-code detection. A deterministic, syntactic, whole-corpus pass: collect every
//! named definition (functions/classes/methods) and count every identifier occurrence across the
//! *entire* codebase. A definition whose name occurs exactly once — only at its own definition
//! site — is reported as an unused candidate.
//!
//! This is intentionally conservative (a name shared or referenced anywhere suppresses the
//! finding, favoring few false positives) and heuristic (no import resolution or type info), so
//! results are emitted as `candidate` findings for a human/LLM to confirm — exactly the case the
//! triage-first workflow is built for. Severity is `low`, so dead code never fails a default gate.

use crate::specs;
use crate::walk;
use crate::LangSpec;
use allem_core::{Category, Confidence, Finding, Location, Severity, SuggestedAction};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tree_sitter::{Node, Parser};

/// Conventional entry points / framework hooks that are "used" implicitly — never flagged.
const IGNORE: &[&str] = &[
    "main", "__init__", "__main__", "__new__", "new", "default", "setUp", "tearDown", "toString",
    "hashCode", "equals",
];

struct Def {
    name: String,
    lang: &'static str,
    file: PathBuf,
    line: u32,
}

/// Analyze every recognized source file under `root` for cross-file dead code.
pub fn analyze(root: &Path) -> Vec<Finding> {
    let sources = walk::source_files(root)
        .into_iter()
        .filter_map(|p| std::fs::read_to_string(&p).ok().map(|s| (p, s)));
    analyze_sources(sources)
}

/// Core logic over in-memory `(path, source)` pairs — filesystem-free, so it is unit-testable.
pub fn analyze_sources(sources: impl IntoIterator<Item = (PathBuf, String)>) -> Vec<Finding> {
    let specs = specs::all();
    let mut counts: HashMap<String, u32> = HashMap::new();
    let mut defs: Vec<Def> = Vec::new();

    for (path, source) in sources {
        let Some((spec, language)) = specs.iter().find(|(s, _)| matches_ext(s, &path)) else {
            continue;
        };
        let mut parser = Parser::new();
        if parser.set_language(language).is_err() {
            continue;
        }
        let Some(tree) = parser.parse(source.as_bytes(), None) else {
            continue;
        };
        collect(
            spec,
            tree.root_node(),
            &source,
            &path,
            &mut counts,
            &mut defs,
        );
    }

    defs.into_iter()
        .filter(|d| !IGNORE.contains(&d.name.as_str()))
        .filter(|d| counts.get(&d.name).copied().unwrap_or(0) <= 1)
        .map(|d| {
            Finding::new(
                format!("deadcode/{}/{}:{}", d.lang, d.name, d.line),
                Category::DeadCode,
                Severity::Low,
                format!(
                    "`{}` ({}) appears unused — defined but never referenced across the codebase",
                    d.name, d.lang
                ),
            )
            .with_location(Location {
                path: d.file,
                line: Some(d.line),
                column: None,
            })
            .with_evidence("symbol", d.name.into())
            .with_evidence("language", d.lang.into())
            .with_action(SuggestedAction {
                action_type: "remove".into(),
                to: None,
                confidence: Confidence::Low,
            })
        })
        .collect()
}

fn matches_ext(spec: &LangSpec, path: &Path) -> bool {
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) => {
            let ext = ext.to_ascii_lowercase();
            spec.extensions.contains(&ext.as_str())
        }
        None => false,
    }
}

/// Single recursive walk that both counts identifier occurrences and records definitions.
fn collect(
    spec: &LangSpec,
    node: Node,
    source: &str,
    path: &Path,
    counts: &mut HashMap<String, u32>,
    defs: &mut Vec<Def>,
) {
    if is_identifier_kind(node.kind()) {
        if let Ok(text) = node.utf8_text(source.as_bytes()) {
            *counts.entry(text.to_string()).or_insert(0) += 1;
        }
    }

    if spec.definition_kinds.contains(&node.kind()) {
        if let Some(name) = spec
            .name_field
            .and_then(|f| node.child_by_field_name(f))
            .and_then(|n| n.utf8_text(source.as_bytes()).ok())
        {
            defs.push(Def {
                name: name.to_string(),
                lang: spec.id,
                file: path.to_path_buf(),
                line: node.start_position().row as u32 + 1,
            });
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect(spec, child, source, path, counts, defs);
    }
}

/// Leaf identifier-like node kinds across grammars (`identifier`, `field_identifier`, …).
fn is_identifier_kind(kind: &str) -> bool {
    kind == "identifier"
        || kind.ends_with("_identifier")
        || kind == "constant"
        || kind == "property_identifier"
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn run(files: &[(&str, &str)]) -> Vec<Finding> {
        analyze_sources(
            files
                .iter()
                .map(|(p, s)| (PathBuf::from(p), s.to_string()))
                .collect::<Vec<_>>(),
        )
    }

    #[test]
    fn flags_unreferenced_function_keeps_used_one() {
        // `orphan` is never called; `ping` is called at module scope.
        let findings = run(&[(
            "a.py",
            "def orphan():\n    return 1\n\ndef ping():\n    return 2\n\nping()\n",
        )]);
        let names: Vec<&str> = findings
            .iter()
            .map(|f| f.evidence["symbol"].as_str().unwrap())
            .collect();
        assert!(
            names.contains(&"orphan"),
            "orphan should be dead: {names:?}"
        );
        assert!(!names.contains(&"ping"), "ping is used: {names:?}");
    }

    #[test]
    fn cross_file_usage_suppresses_finding() {
        // `helper` is defined in a.py and used in b.py → not dead.
        let findings = run(&[
            ("a.py", "def helper():\n    return 1\n"),
            ("b.py", "from a import helper\nhelper()\n"),
        ]);
        let names: Vec<&str> = findings
            .iter()
            .map(|f| f.evidence["symbol"].as_str().unwrap())
            .collect();
        assert!(
            !names.contains(&"helper"),
            "helper used cross-file: {names:?}"
        );
    }

    #[test]
    fn main_is_never_flagged() {
        let findings = run(&[("a.py", "def main():\n    return 0\n")]);
        assert!(findings.is_empty());
    }
}
