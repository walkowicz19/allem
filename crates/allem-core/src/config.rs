//! Zero-config by default. An optional `.allemrc.json` may override which checks run,
//! the gate threshold, and offline behavior (KISS — only knobs we actually need).

use crate::error::{AllemError, Result};
use crate::finding::Severity;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Minimum severity that causes a non-zero exit in `audit`/gate mode.
    pub gate_severity: Severity,
    /// If true, never hit the network (OSV/registries); use cached data only.
    pub offline: bool,
    /// Explicit language ids to restrict to; empty = autodetect all.
    pub languages: Vec<String>,
    /// Explicit ecosystem ids to restrict to; empty = autodetect all.
    pub ecosystems: Vec<String>,
    /// Path fragments to exclude from analysis (e.g. "fixtures", "vendor", "test/data").
    /// A file is skipped if any fragment appears in its slash-normalized path. Common
    /// build/VCS dirs (target, .git, node_modules, …) are always skipped.
    pub exclude: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            gate_severity: Severity::High,
            offline: false,
            languages: Vec::new(),
            ecosystems: Vec::new(),
            exclude: Vec::new(),
        }
    }
}

impl Config {
    /// Whether `path` (any form) is excluded by a configured fragment.
    pub fn is_excluded(&self, path: &Path) -> bool {
        if self.exclude.is_empty() {
            return false;
        }
        let normalized = path.to_string_lossy().replace('\\', "/");
        self.exclude.iter().any(|frag| {
            let frag = frag.replace('\\', "/");
            !frag.is_empty() && normalized.contains(&frag)
        })
    }
}

impl Config {
    /// Load `.allemrc.json` from `root` if present; otherwise return defaults.
    pub fn load(root: &Path) -> Result<Self> {
        let path = root.join(".allemrc.json");
        match std::fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text)
                .map_err(|e| AllemError::Config(format!("{}: {e}", path.display()))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
            Err(source) => Err(AllemError::Io { path, source }),
        }
    }
}
