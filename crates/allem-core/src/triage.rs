//! Persistent triage state. Allem never bulk-fixes: a human or LLM reviews each candidate and
//! records a verdict, stored in `.allem/triage.json` keyed by finding id. On later runs the
//! stored verdict is re-applied to matching findings, so confirmations and false-positive
//! dismissals survive across sessions and feed CI gating.

use crate::error::{AllemError, Result};
use crate::finding::{Finding, FindingStatus};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Maps finding id → recorded status. Only triaged findings appear here; everything else
/// stays a `candidate`.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct TriageStore {
    #[serde(default)]
    entries: BTreeMap<String, FindingStatus>,
}

impl TriageStore {
    fn path(root: &Path) -> PathBuf {
        root.join(".allem").join("triage.json")
    }

    /// Load the store for `root`, or an empty store if none exists yet.
    pub fn load(root: &Path) -> Result<Self> {
        let path = Self::path(root);
        match std::fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text)
                .map_err(|e| AllemError::Config(format!("{}: {e}", path.display()))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(source) => Err(AllemError::Io { path, source }),
        }
    }

    /// Persist the store under `root` (creates `.allem/` if needed).
    pub fn save(&self, root: &Path) -> Result<()> {
        let dir = root.join(".allem");
        std::fs::create_dir_all(&dir).map_err(|source| AllemError::Io {
            path: dir.clone(),
            source,
        })?;
        let path = Self::path(root);
        let text = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, text).map_err(|source| AllemError::Io { path, source })
    }

    pub fn set(&mut self, id: impl Into<String>, status: FindingStatus) {
        self.entries.insert(id.into(), status);
    }

    pub fn get(&self, id: &str) -> Option<FindingStatus> {
        self.entries.get(id).copied()
    }

    pub fn entries(&self) -> &BTreeMap<String, FindingStatus> {
        &self.entries
    }

    /// Stamp each finding with its recorded status (leaves untriaged findings as candidates).
    pub fn apply(&self, findings: &mut [Finding]) {
        for f in findings.iter_mut() {
            if let Some(status) = self.get(&f.id) {
                f.status = status;
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::{Category, Severity};

    #[test]
    fn apply_stamps_status() {
        let mut store = TriageStore::default();
        store.set("x", FindingStatus::FalsePositive);
        let mut findings = vec![Finding::new("x", Category::Complexity, Severity::High, "x")];
        store.apply(&mut findings);
        assert_eq!(findings[0].status, FindingStatus::FalsePositive);
    }

    #[test]
    fn roundtrips_through_json() {
        let mut store = TriageStore::default();
        store.set("dep/a", FindingStatus::Confirmed);
        let json = serde_json::to_string(&store).unwrap();
        let back: TriageStore = serde_json::from_str(&json).unwrap();
        assert_eq!(back.get("dep/a"), Some(FindingStatus::Confirmed));
    }
}
