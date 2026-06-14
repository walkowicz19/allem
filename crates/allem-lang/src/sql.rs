//! Bespoke SQL adapter. SQL has no "functions" in the tree-sitter sense, so this scores stored
//! procedures / functions by counting control-flow keywords, and flags classic dynamic-SQL and
//! OS-command sinks (e.g. `xp_cmdshell`, `EXECUTE IMMEDIATE`). Deterministic, no tree-sitter
//! (there is no reliable Rust-bound SQL grammar), demonstrating the adapter contract again.

use allem_core::{
    Category, Confidence, Finding, LanguageAdapter, Location, Result, Severity, SuggestedAction,
};
use std::path::Path;

pub struct SqlAdapter {
    warn: u32,
    high: u32,
}

impl Default for SqlAdapter {
    fn default() -> Self {
        SqlAdapter { warn: 10, high: 20 }
    }
}

/// Decision-introducing keywords (whole-word, uppercased) inside a routine body.
const DECISION_KEYWORDS: &[&str] = &[
    "IF", "ELSIF", "ELSEIF", "CASE", "WHEN", "LOOP", "WHILE", "FOR", "UNTIL", "AND", "OR",
];

/// Dynamic-SQL / OS-command execution markers — the SQL injection surface.
const INJECTION_MARKERS: &[&str] = &[
    "XP_CMDSHELL",
    "EXECUTE IMMEDIATE",
    "SP_EXECUTESQL",
    "EXEC(",
    "EXECUTE(",
];

impl LanguageAdapter for SqlAdapter {
    fn id(&self) -> &'static str {
        "sql"
    }

    fn matches(&self, path: &Path) -> bool {
        matches!(
            path.extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_ascii_lowercase())
                .as_deref(),
            Some("sql")
        )
    }

    fn analyze_file(&self, path: &Path, source: &str) -> Result<Vec<Finding>> {
        let mut findings = self.routine_complexity(path, source);
        findings.extend(scan_injection(path, source));
        Ok(findings)
    }
}

struct Routine {
    name: String,
    line: u32,
    complexity: u32,
}

impl SqlAdapter {
    /// Score each CREATE PROCEDURE / FUNCTION by control-flow keyword count.
    fn routine_complexity(&self, path: &Path, source: &str) -> Vec<Finding> {
        let mut routines: Vec<Routine> = Vec::new();

        for (idx, raw) in source.lines().enumerate() {
            let upper = strip_comment(raw).to_ascii_uppercase();
            let line_no = idx as u32 + 1;

            if let Some(name) = routine_header(&upper) {
                routines.push(Routine {
                    name,
                    line: line_no,
                    complexity: 1,
                });
                continue;
            }
            if let Some(current) = routines.last_mut() {
                for word in upper.split(|c: char| !c.is_ascii_alphanumeric() && c != '_') {
                    if DECISION_KEYWORDS.contains(&word) {
                        current.complexity += 1;
                    }
                }
            }
        }

        routines
            .into_iter()
            .filter(|r| r.complexity >= self.warn)
            .map(|r| {
                let severity = if r.complexity >= self.high {
                    Severity::High
                } else {
                    Severity::Medium
                };
                Finding::new(
                    format!("complexity/sql/{}:{}", r.name, r.line),
                    Category::Complexity,
                    severity,
                    format!(
                        "routine `{}` has complexity {} (limit {})",
                        r.name, r.complexity, self.warn
                    ),
                )
                .with_location(Location {
                    path: path.to_path_buf(),
                    line: Some(r.line),
                    column: None,
                })
                .with_evidence("complexity", r.complexity.into())
                .with_evidence("language", "sql".into())
                .with_action(SuggestedAction {
                    action_type: "review".into(),
                    to: None,
                    confidence: Confidence::Medium,
                })
            })
            .collect()
    }
}

fn scan_injection(path: &Path, source: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    for (idx, raw) in source.lines().enumerate() {
        let upper = strip_comment(raw).to_ascii_uppercase();
        for marker in INJECTION_MARKERS {
            if upper.contains(marker) {
                let line_no = idx as u32 + 1;
                findings.push(
                    Finding::new(
                        format!("inject/sql/{marker}:{line_no}"),
                        Category::CodeInjection,
                        Severity::High,
                        format!("dynamic-SQL / command sink `{marker}` in sql"),
                    )
                    .with_location(Location {
                        path: path.to_path_buf(),
                        line: Some(line_no),
                        column: None,
                    })
                    .with_evidence("sink", marker.to_string().into())
                    .with_evidence("language", "sql".into())
                    .with_action(SuggestedAction {
                        action_type: "review".into(),
                        to: None,
                        confidence: Confidence::Medium,
                    }),
                );
            }
        }
    }
    findings
}

/// If an (uppercased) line declares a routine, return its name.
fn routine_header(upper: &str) -> Option<String> {
    let trimmed = upper.trim();
    if !trimmed.starts_with("CREATE") {
        return None;
    }
    let keyword = if trimmed.contains("PROCEDURE") {
        "PROCEDURE"
    } else if trimmed.contains("FUNCTION") {
        "FUNCTION"
    } else {
        return None;
    };
    let after = trimmed.split(keyword).nth(1)?.trim();
    // Name is the first token, sans argument list / schema-qualified.
    let name = after
        .split(|c: char| c == '(' || c.is_whitespace())
        .next()
        .unwrap_or("")
        .trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

/// Drop a trailing `-- comment` from a line.
fn strip_comment(line: &str) -> &str {
    match line.find("--") {
        Some(i) => &line[..i],
        None => line,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn matches_sql_extension() {
        assert!(SqlAdapter::default().matches(Path::new("schema.SQL")));
        assert!(!SqlAdapter::default().matches(Path::new("x.py")));
    }

    #[test]
    fn flags_complex_procedure() {
        let mut src = String::from("CREATE PROCEDURE dbo.Big AS BEGIN\n");
        for _ in 0..12 {
            src.push_str("  IF @x = 1 AND @y = 2 SET @z = 3;\n");
        }
        src.push_str("END;\n");
        let f = SqlAdapter::default()
            .analyze_file(Path::new("p.sql"), &src)
            .unwrap();
        assert!(f.iter().any(|x| x.category == Category::Complexity));
    }

    #[test]
    fn flags_injection_sink() {
        let src = "EXEC master..xp_cmdshell 'dir';\n";
        let f = SqlAdapter::default()
            .analyze_file(Path::new("p.sql"), src)
            .unwrap();
        assert!(f.iter().any(|x| x.category == Category::CodeInjection));
    }

    #[test]
    fn plain_query_is_clean() {
        let f = SqlAdapter::default()
            .analyze_file(
                Path::new("q.sql"),
                "SELECT id, name FROM users WHERE id = 1;\n",
            )
            .unwrap();
        assert!(f.is_empty());
    }
}
