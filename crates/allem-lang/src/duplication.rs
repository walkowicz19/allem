//! Cross-file duplication detection. A deterministic, whole-corpus pass: for every function in
//! every file, build a normalized token sequence (leaf tokens, excluding comments and
//! whitespace) and hash it. Functions whose token sequences are identical — even if their
//! comments or formatting differ — are reported as a clone group.
//!
//! This catches type-1 copy-paste with low false positives (identical logic only). Trivial
//! functions are skipped via size thresholds. Findings are `low`-severity candidates for triage.

use crate::specs;
use crate::walk;
use crate::LangSpec;
use allem_core::{Category, Confidence, Finding, Location, Severity, SuggestedAction};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use tree_sitter::{Node, Parser};

/// Minimum spanned lines for a function to be considered (skips one-liners/getters).
const MIN_LINES: u32 = 3;
/// Minimum normalized token count (skips tiny bodies that collide trivially).
const MIN_TOKENS: usize = 20;
/// Recursion cap — stops deeply nested (minified/generated) trees from overflowing the stack.
const MAX_DEPTH: usize = 1000;

struct Clone {
    file: PathBuf,
    line: u32,
    lang: &'static str,
}

/// Analyze every recognized source file under `root` for duplicated functions.
pub fn analyze(root: &Path) -> Vec<Finding> {
    let sources = walk::source_files(root)
        .into_iter()
        .filter_map(|p| std::fs::read_to_string(&p).ok().map(|s| (p, s)));
    analyze_sources(sources)
}

/// Core logic over in-memory `(path, source)` pairs — filesystem-free, so it is unit-testable.
pub fn analyze_sources(sources: impl IntoIterator<Item = (PathBuf, String)>) -> Vec<Finding> {
    let specs = specs::all();
    // hash(token-sequence) → all functions sharing it.
    let mut groups: HashMap<u64, Vec<Clone>> = HashMap::new();

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
        collect_functions(spec, tree.root_node(), &source, &path, &mut groups, 0);
    }

    let mut findings: Vec<Finding> = groups
        .into_iter()
        .filter(|(_, clones)| clones.len() > 1)
        .map(|(hash, mut clones)| {
            clones.sort_by(|a, b| (a.file.clone(), a.line).cmp(&(b.file.clone(), b.line)));
            let lang = clones[0].lang;
            let mut finding = Finding::new(
                format!("duplication/{lang}/{hash:x}"),
                Category::Duplication,
                Severity::Low,
                format!(
                    "{} identical {lang} function blocks (possible copy-paste)",
                    clones.len()
                ),
            )
            .with_evidence("language", lang.into())
            .with_evidence("copies", clones.len().into())
            .with_action(SuggestedAction {
                action_type: "review".into(),
                to: None,
                confidence: Confidence::Low,
            });
            for c in clones {
                finding = finding.with_location(Location {
                    path: c.file,
                    line: Some(c.line),
                    column: None,
                });
            }
            finding
        })
        .collect();

    // Stable output order for deterministic reports.
    findings.sort_by(|a, b| a.id.cmp(&b.id));
    findings
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

fn collect_functions(
    spec: &LangSpec,
    node: Node,
    source: &str,
    path: &Path,
    groups: &mut HashMap<u64, Vec<Clone>>,
    depth: usize,
) {
    if depth > MAX_DEPTH {
        return;
    }
    if spec.function_kinds.contains(&node.kind()) {
        let lines = node.end_position().row as u32 - node.start_position().row as u32 + 1;
        if lines >= MIN_LINES {
            let mut tokens = Vec::new();
            collect_tokens(node, source, &mut tokens, 0);
            if tokens.len() >= MIN_TOKENS {
                let mut hasher = DefaultHasher::new();
                spec.id.hash(&mut hasher);
                tokens.hash(&mut hasher);
                groups.entry(hasher.finish()).or_default().push(Clone {
                    file: path.to_path_buf(),
                    line: node.start_position().row as u32 + 1,
                    lang: spec.id,
                });
            }
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_functions(spec, child, source, path, groups, depth + 1);
    }
}

/// Collect leaf-token text in order, skipping comment subtrees and whitespace — so reformatting
/// and reworded comments don't hide an otherwise identical body.
fn collect_tokens(node: Node, source: &str, out: &mut Vec<String>, depth: usize) {
    if depth > MAX_DEPTH || node.kind().contains("comment") {
        return;
    }
    if node.child_count() == 0 {
        if let Ok(text) = node.utf8_text(source.as_bytes()) {
            let text = text.trim();
            if !text.is_empty() {
                out.push(text.to_string());
            }
        }
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_tokens(child, source, out, depth + 1);
    }
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

    const BODY: &str = "    total = price * qty\n    if member:\n        total = total * 0.9\n    if qty > 10:\n        total = total * 0.95\n    return total\n";

    #[test]
    fn identical_functions_across_files_flagged_despite_comments() {
        let a = format!("def calc(price, qty, member):\n    # version A\n{BODY}");
        let b = format!("def calc(price, qty, member):\n    # totally different comment\n{BODY}");
        let findings = run(&[("a.py", &a), ("b.py", &b)]);
        assert_eq!(findings.len(), 1, "expected one clone group");
        assert_eq!(findings[0].category, Category::Duplication);
        assert_eq!(findings[0].locations.len(), 2, "two copies");
    }

    #[test]
    fn distinct_functions_not_flagged() {
        let a = "def alpha(x):\n    y = x + 1\n    if y > 0:\n        return y\n    return 0\n";
        let b =
            "def beta(x):\n    z = x * 2\n    for i in range(z):\n        z += i\n    return z\n";
        assert!(run(&[("a.py", a), ("b.py", b)]).is_empty());
    }

    #[test]
    fn trivial_functions_skipped() {
        let a = "def f():\n    return 1\n";
        let b = "def g():\n    return 1\n";
        assert!(run(&[("a.py", a), ("b.py", b)]).is_empty());
    }
}
