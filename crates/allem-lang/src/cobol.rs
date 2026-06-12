//! Bespoke COBOL adapter. COBOL is line-oriented and lacks a maintained tree-sitter grammar
//! with Rust bindings, so this implements [`LanguageAdapter`] directly — demonstrating that
//! Allem's adapter contract isn't tied to tree-sitter. It splits the PROCEDURE DIVISION into
//! paragraphs and scores each by counting decision verbs (deterministic, no AI).

use allem_core::{
    Category, Confidence, Finding, LanguageAdapter, Location, Result, Severity, SuggestedAction,
};
use std::path::Path;

pub struct CobolAdapter {
    /// Complexity (1 + decision verbs) at/above which a paragraph is reported.
    warn: u32,
    high: u32,
}

impl Default for CobolAdapter {
    fn default() -> Self {
        CobolAdapter { warn: 10, high: 20 }
    }
}

/// Decision-introducing COBOL verbs/keywords (uppercased comparison).
const DECISION_KEYWORDS: &[&str] = &[
    "IF", "ELSE", "EVALUATE", "WHEN", "UNTIL", "PERFORM", "AND", "OR",
];

impl LanguageAdapter for CobolAdapter {
    fn id(&self) -> &'static str {
        "cobol"
    }

    fn matches(&self, path: &Path) -> bool {
        matches!(
            path.extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_ascii_lowercase())
                .as_deref(),
            Some("cob" | "cbl" | "cpy" | "cobol")
        )
    }

    fn analyze_file(&self, path: &Path, source: &str) -> Result<Vec<Finding>> {
        Ok(self
            .paragraphs(source)
            .into_iter()
            .filter_map(|p| {
                let complexity = p.complexity;
                if complexity < self.warn {
                    return None;
                }
                let severity = if complexity >= self.high {
                    Severity::High
                } else {
                    Severity::Medium
                };
                Some(
                    Finding::new(
                        format!("complexity/cobol/{}:{}", p.name, p.line),
                        Category::Complexity,
                        severity,
                        format!(
                            "paragraph `{}` has complexity {complexity} (limit {})",
                            p.name, self.warn
                        ),
                    )
                    .with_location(Location {
                        path: path.to_path_buf(),
                        line: Some(p.line),
                        column: None,
                    })
                    .with_evidence("complexity", complexity.into())
                    .with_evidence("language", "cobol".into())
                    .with_action(SuggestedAction {
                        action_type: "review".into(),
                        to: None,
                        confidence: Confidence::Medium,
                    }),
                )
            })
            .collect())
    }
}

struct Paragraph {
    name: String,
    line: u32,
    complexity: u32,
}

impl CobolAdapter {
    /// Split source into PROCEDURE-DIVISION paragraphs and score each. A paragraph header is
    /// a label in Area A (cols 8-11): a bare name followed by a period, no inline statement.
    fn paragraphs(&self, source: &str) -> Vec<Paragraph> {
        let mut in_procedure = false;
        let mut paragraphs: Vec<Paragraph> = Vec::new();

        for (idx, raw) in source.lines().enumerate() {
            let line_no = idx as u32 + 1;
            let code = strip_cobol(raw);
            let upper = code.to_ascii_uppercase();
            let trimmed = upper.trim();

            if trimmed.contains("PROCEDURE DIVISION") {
                in_procedure = true;
                continue;
            }
            if !in_procedure || trimmed.is_empty() {
                continue;
            }

            if let Some(name) = paragraph_header(code) {
                paragraphs.push(Paragraph {
                    name,
                    line: line_no,
                    complexity: 1,
                });
                continue;
            }

            // Count decision verbs on this line into the current paragraph.
            if let Some(current) = paragraphs.last_mut() {
                for word in upper.split(|c: char| !c.is_ascii_alphabetic()) {
                    if DECISION_KEYWORDS.contains(&word) {
                        current.complexity += 1;
                    }
                }
            }
        }
        paragraphs
    }
}

/// Drop the COBOL sequence-number area (cols 1-6) and the indicator column (col 7),
/// keeping the code area. Lines shorter than that are returned trimmed.
fn strip_cobol(line: &str) -> &str {
    // Comment line: indicator column (index 6) is '*' or '/'.
    if line.len() > 6 {
        let indicator = line.as_bytes()[6];
        if indicator == b'*' || indicator == b'/' {
            return "";
        }
        &line[7..]
    } else {
        line.trim()
    }
}

/// If `code` is a paragraph header (a single name then a period), return the name.
fn paragraph_header(code: &str) -> Option<String> {
    let trimmed = code.trim();
    let stripped = trimmed.strip_suffix('.')?;
    // Headers are a single token; statements have spaces or are known divisions/sections.
    if stripped.is_empty() || stripped.contains(char::is_whitespace) {
        return None;
    }
    let upper = stripped.to_ascii_uppercase();
    if upper.ends_with("DIVISION") || upper.ends_with("SECTION") {
        return None;
    }
    // A label looks like an identifier (letters, digits, hyphens).
    if stripped
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        Some(stripped.to_string())
    } else {
        None
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn matches_cobol_extensions() {
        let a = CobolAdapter::default();
        assert!(a.matches(Path::new("PAYROLL.CBL")));
        assert!(a.matches(Path::new("x.cob")));
        assert!(!a.matches(Path::new("x.py")));
    }

    #[test]
    fn flags_complex_paragraph() {
        // A paragraph with many IF/PERFORM verbs exceeds the complexity limit.
        let mut src = String::from("       PROCEDURE DIVISION.\n");
        src.push_str("       MAIN-LOGIC.\n");
        for _ in 0..12 {
            src.push_str("           IF X = 1 PERFORM SUB-A END-IF.\n");
        }
        let findings = CobolAdapter::default()
            .analyze_file(Path::new("p.cbl"), &src)
            .unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].category, Category::Complexity);
    }

    #[test]
    fn simple_paragraph_is_clean() {
        let src = "       PROCEDURE DIVISION.\n       MAIN.\n           DISPLAY 'HI'.\n";
        let findings = CobolAdapter::default()
            .analyze_file(Path::new("p.cbl"), src)
            .unwrap();
        assert!(findings.is_empty());
    }
}
