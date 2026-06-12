//! Extension contracts. The orchestrator is language- and ecosystem-agnostic: it only
//! knows these traits. Adding a language or package ecosystem means registering one more
//! small adapter — never touching the engine (clean-code: ISP, KISS).

use crate::error::Result;
use crate::finding::Finding;
use std::path::Path;

/// A per-language code-intelligence adapter (tree-sitter backed in real impls).
pub trait LanguageAdapter: Send + Sync {
    /// Stable id, e.g. "python", "rust", "cobol".
    fn id(&self) -> &'static str;

    /// Whether this adapter handles the given file (by extension/shebang/heuristics).
    fn matches(&self, path: &Path) -> bool;

    /// Analyze a single source file, returning any findings (dead code, complexity, …).
    fn analyze_file(&self, path: &Path, source: &str) -> Result<Vec<Finding>>;
}

/// A per-ecosystem dependency adapter (pip, cargo, gem, maven, go mod, npm, …).
pub trait EcosystemAdapter: Send + Sync {
    /// Stable id, aligned with OSV ecosystem naming where possible.
    fn id(&self) -> &'static str;

    /// Whether a manifest/lockfile for this ecosystem exists under `root`.
    fn detect(&self, root: &Path) -> bool;

    /// Parse manifests and run the safety checks (outdated/vulnerable/dangerous/injection).
    fn analyze(&self, root: &Path) -> Result<Vec<Finding>>;
}
