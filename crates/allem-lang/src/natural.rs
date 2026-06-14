//! Bespoke NATURAL adapter (Software AG's mainframe 4GL — a legacy companion to COBOL). NATURAL
//! is line/keyword-oriented with no tree-sitter grammar, so this scores each `DEFINE SUBROUTINE`
//! block (or the whole program if it has none) by counting control-flow keywords. Deterministic.

use allem_core::{
    Category, Confidence, Finding, LanguageAdapter, Location, Result, Severity, SuggestedAction,
};
use std::path::Path;

pub struct NaturalAdapter {
    warn: u32,
    high: u32,
}

impl Default for NaturalAdapter {
    fn default() -> Self {
        NaturalAdapter { warn: 10, high: 20 }
    }
}

/// Decision-introducing NATURAL keywords (whole-word, uppercased).
const DECISION_KEYWORDS: &[&str] = &[
    "IF", "DECIDE", "WHEN", "FOR", "REPEAT", "WHILE", "UNTIL", "AND", "OR", "ELSE",
];

impl LanguageAdapter for NaturalAdapter {
    fn id(&self) -> &'static str {
        "natural"
    }

    fn matches(&self, path: &Path) -> bool {
        matches!(
            path.extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_ascii_lowercase())
                .as_deref(),
            Some("nsp" | "nsn" | "nss" | "nat" | "natural")
        )
    }

    fn analyze_file(&self, path: &Path, source: &str) -> Result<Vec<Finding>> {
        Ok(self
            .units(path, source)
            .into_iter()
            .filter(|u| u.complexity >= self.warn)
            .map(|u| self.finding(path, u))
            .collect())
    }
}

struct Unit {
    name: String,
    line: u32,
    complexity: u32,
}

impl NaturalAdapter {
    fn units(&self, path: &Path, source: &str) -> Vec<Unit> {
        let mut units: Vec<Unit> = Vec::new();

        for (idx, raw) in source.lines().enumerate() {
            let upper = strip_comment(raw).to_ascii_uppercase();
            let line_no = idx as u32 + 1;

            if let Some(name) = subroutine_header(&upper) {
                units.push(Unit {
                    name,
                    line: line_no,
                    complexity: 1,
                });
                continue;
            }
            if let Some(current) = units.last_mut() {
                count_into(&upper, &mut current.complexity);
            }
        }

        // Fallback: a program with no explicit subroutines is scored as one unit.
        if units.is_empty() {
            let mut complexity = 1;
            for raw in source.lines() {
                count_into(&strip_comment(raw).to_ascii_uppercase(), &mut complexity);
            }
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("program")
                .to_string();
            units.push(Unit {
                name,
                line: 1,
                complexity,
            });
        }
        units
    }

    fn finding(&self, path: &Path, u: Unit) -> Finding {
        let severity = if u.complexity >= self.high {
            Severity::High
        } else {
            Severity::Medium
        };
        Finding::new(
            format!("complexity/natural/{}:{}", u.name, u.line),
            Category::Complexity,
            severity,
            format!(
                "`{}` has complexity {} (limit {})",
                u.name, u.complexity, self.warn
            ),
        )
        .with_location(Location {
            path: path.to_path_buf(),
            line: Some(u.line),
            column: None,
        })
        .with_evidence("complexity", u.complexity.into())
        .with_evidence("language", "natural".into())
        .with_action(SuggestedAction {
            action_type: "review".into(),
            to: None,
            confidence: Confidence::Medium,
        })
    }
}

fn count_into(upper: &str, complexity: &mut u32) {
    for word in upper.split(|c: char| !c.is_ascii_alphanumeric()) {
        if DECISION_KEYWORDS.contains(&word) {
            *complexity += 1;
        }
    }
}

/// If an (uppercased) line opens a subroutine, return its name.
fn subroutine_header(upper: &str) -> Option<String> {
    let trimmed = upper.trim();
    let after = trimmed.strip_prefix("DEFINE SUBROUTINE")?.trim();
    let name = after.split_whitespace().next().unwrap_or("").trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

/// Drop a NATURAL `*` comment (line starting with `*`) or trailing `/* ... */`-style note.
fn strip_comment(line: &str) -> &str {
    let t = line.trim_start();
    if t.starts_with('*') {
        return "";
    }
    line
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn matches_natural_extensions() {
        let a = NaturalAdapter::default();
        assert!(a.matches(Path::new("ORDER.NSP")));
        assert!(a.matches(Path::new("sub.nsn")));
        assert!(!a.matches(Path::new("x.py")));
    }

    #[test]
    fn flags_complex_subroutine() {
        let mut src = String::from("DEFINE SUBROUTINE CALC\n");
        for _ in 0..12 {
            src.push_str("  IF #X = 1 AND #Y = 2\n    ADD 1 TO #Z\n  END-IF\n");
        }
        src.push_str("END-SUBROUTINE\n");
        let f = NaturalAdapter::default()
            .analyze_file(Path::new("c.nss"), &src)
            .unwrap();
        assert!(f.iter().any(|x| x.category == Category::Complexity));
        assert!(f[0].id.contains("CALC"));
    }

    #[test]
    fn simple_program_is_clean() {
        let f = NaturalAdapter::default()
            .analyze_file(Path::new("p.nsp"), "WRITE 'HELLO'\nEND\n")
            .unwrap();
        assert!(f.is_empty());
    }
}
