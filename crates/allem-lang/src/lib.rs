//! `allem-lang` — polyglot code intelligence.
//!
//! Most languages use the generic [`TreeSitterAdapter`], which runs the same deterministic
//! analyses (cyclomatic complexity, long functions) for any grammar from a small per-language
//! [`LangSpec`] (clean-code: DRY, ISP). COBOL uses a bespoke [`cobol::CobolAdapter`], proving
//! the adapter contract isn't tied to tree-sitter. Adding a language is one spec/adapter — never
//! an engine change.

mod analysis;
pub mod cobol;
pub mod deadcode;
pub mod duplication;
pub mod natural;
mod specs;
pub mod sql;
mod walk;

use allem_core::{Finding, LanguageAdapter, Result};
use std::path::Path;
use tree_sitter::Language;

pub use analysis::ComplexityThresholds;

/// Describes the tree-sitter node kinds a language uses, so the generic analyses can run
/// against any grammar.
#[derive(Clone)]
pub struct LangSpec {
    /// Stable language id, e.g. "python".
    pub id: &'static str,
    /// File extensions handled (lowercase, no dot), e.g. ["py"].
    pub extensions: &'static [&'static str],
    /// Node kinds that represent a function/method/procedure body.
    pub function_kinds: &'static [&'static str],
    /// Field name holding a function's identifier (for nicer findings), if any.
    pub name_field: Option<&'static str>,
    /// Node kinds that each add one decision point to cyclomatic complexity.
    pub decision_kinds: &'static [&'static str],
    /// Node kinds of named definitions tracked for cross-file dead-code (functions, classes…).
    /// The definition's name is read via `name_field`; empty disables dead-code for the language.
    pub definition_kinds: &'static [&'static str],
    /// Node kinds that represent a function/method call (for injection-sink detection).
    pub call_kinds: &'static [&'static str],
    /// Field on a call node holding the callee (function name/expression), if any.
    pub callee_field: Option<&'static str>,
    /// Dangerous callee patterns (e.g. "eval", "os.system", "Command::new"). A call whose
    /// callee equals one of these — or ends with `.`/`::` + the pattern — is flagged.
    pub sinks: &'static [&'static str],
}

/// A generic, grammar-driven [`LanguageAdapter`].
pub struct TreeSitterAdapter {
    spec: LangSpec,
    language: Language,
    thresholds: ComplexityThresholds,
}

impl TreeSitterAdapter {
    pub fn new(spec: LangSpec, language: Language) -> Self {
        TreeSitterAdapter {
            spec,
            language,
            thresholds: ComplexityThresholds::default(),
        }
    }
}

impl LanguageAdapter for TreeSitterAdapter {
    fn id(&self) -> &'static str {
        self.spec.id
    }

    fn matches(&self, path: &Path) -> bool {
        match path.extension().and_then(|e| e.to_str()) {
            Some(ext) => {
                let ext = ext.to_ascii_lowercase();
                self.spec.extensions.contains(&ext.as_str())
            }
            None => false,
        }
    }

    fn analyze_file(&self, path: &Path, source: &str) -> Result<Vec<Finding>> {
        analysis::analyze(&self.spec, &self.language, &self.thresholds, path, source)
    }
}

/// Build all language adapters Allem ships with (tree-sitter languages + COBOL).
pub fn adapters() -> Vec<Box<dyn LanguageAdapter>> {
    let mut adapters: Vec<Box<dyn LanguageAdapter>> = specs::all()
        .into_iter()
        .map(|(spec, lang)| {
            Box::new(TreeSitterAdapter::new(spec, lang)) as Box<dyn LanguageAdapter>
        })
        .collect();
    adapters.push(Box::new(cobol::CobolAdapter::default()));
    adapters.push(Box::new(sql::SqlAdapter::default()));
    adapters.push(Box::new(natural::NaturalAdapter::default()));
    adapters
}

/// Result of code-intelligence analysis over a directory tree.
pub struct LangReport {
    pub languages: Vec<&'static str>,
    pub findings: Vec<Finding>,
}

/// Walk `root`, analyze every recognized source file, and collect findings + detected
/// languages. Skips common vendor/build directories. I/O errors on a single file are
/// skipped rather than aborting the whole run.
pub fn analyze_tree(root: &Path) -> Result<LangReport> {
    let adapters = adapters();
    let mut findings = Vec::new();
    let mut languages: Vec<&'static str> = Vec::new();

    for path in walk::source_files(root) {
        let Some(adapter) = adapters.iter().find(|a| a.matches(&path)) else {
            continue;
        };
        if !languages.contains(&adapter.id()) {
            languages.push(adapter.id());
        }
        let Ok(source) = std::fs::read_to_string(&path) else {
            continue;
        };
        findings.extend(adapter.analyze_file(&path, &source)?);
    }

    // Cross-file dead code and duplication need the whole corpus, so they run as their own passes.
    findings.extend(deadcode::analyze(root));
    findings.extend(duplication::analyze(root));

    Ok(LangReport {
        languages,
        findings,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use allem_core::Category;

    fn python() -> Box<dyn LanguageAdapter> {
        adapters().into_iter().find(|a| a.id() == "python").unwrap()
    }

    #[test]
    fn ships_all_launch_languages() {
        let ids: Vec<&str> = adapters().iter().map(|a| a.id()).collect();
        for lang in [
            "python",
            "rust",
            "go",
            "ruby",
            "java",
            "javascript",
            "typescript",
            "c",
            "cpp",
            "csharp",
            "php",
            "bash",
            "cobol",
            "sql",
            "natural",
        ] {
            assert!(ids.contains(&lang), "missing adapter: {lang}");
        }
    }

    #[test]
    fn python_injection_sinks_are_flagged() {
        let src = "import os\ndef run(cmd):\n    os.system(cmd)\n    eval(cmd)\n";
        let findings = python().analyze_file(Path::new("f.py"), src).unwrap();
        let sinks = findings
            .iter()
            .filter(|f| f.category == Category::CodeInjection)
            .count();
        assert_eq!(sinks, 2, "expected os.system and eval sinks");
    }

    #[test]
    fn matches_by_extension() {
        let a = python();
        assert!(a.matches(Path::new("svc.py")));
        assert!(!a.matches(Path::new("svc.rs")));
    }

    #[test]
    fn syntax_error_is_flagged() {
        let src = "def broken(:\n    x = (1 +\n    return x\n";
        let findings = python().analyze_file(Path::new("broken.py"), src).unwrap();
        assert!(
            findings.iter().any(|f| f.category == Category::ParseError),
            "expected a parse_error finding"
        );
    }

    #[test]
    fn valid_file_has_no_parse_error() {
        let findings = python()
            .analyze_file(Path::new("ok.py"), "def f(x):\n    return x + 1\n")
            .unwrap();
        assert!(!findings.iter().any(|f| f.category == Category::ParseError));
    }

    #[test]
    fn complex_python_function_is_flagged() {
        let src = "def f(x):\n    if x: pass\n    elif x: pass\n    for i in x:\n        while i:\n            if i and x or x:\n                try: pass\n                except: pass\n    if x: pass\n    if x: pass\n";
        let findings = python().analyze_file(Path::new("f.py"), src).unwrap();
        assert!(findings.iter().any(|f| f.category == Category::Complexity));
    }

    #[test]
    fn trivial_function_is_clean() {
        let findings = python()
            .analyze_file(Path::new("f.py"), "def f():\n    return 1\n")
            .unwrap();
        assert!(findings.is_empty());
    }
}
