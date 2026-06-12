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
}

impl Default for Config {
    fn default() -> Self {
        Config {
            gate_severity: Severity::High,
            offline: false,
            languages: Vec::new(),
            ecosystems: Vec::new(),
        }
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
