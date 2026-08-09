//! Append-only refinement log with snapshot-based rollback.
//!
//! Every applied refinement stores the `baseline_state` it was applied to
//! plus the edit that was made. Rolling back by refinement ID restores that
//! baseline — the same contract Prime Agent exposes, backed by plain JSON
//! files so it survives across turns and sessions.

use crate::state::{HarnessKind, HarnessState};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RefinementAction {
    Create,
    Update,
    Delete,
}

impl RefinementAction {
    pub fn as_str(self) -> &'static str {
        match self {
            RefinementAction::Create => "create",
            RefinementAction::Update => "update",
            RefinementAction::Delete => "delete",
        }
    }
}

/// One applied refinement, fully replayable or reversible.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RefinementResult {
    pub id: String,
    pub kind: HarnessKind,
    pub action: RefinementAction,
    pub title: String,
    pub content: String,
    pub trigger: String,
    pub outcome: String,
    pub baseline_state: HarnessState,
    pub applied_at: String,
}

/// Append-only refinement history with snapshot rollback.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RefinementLog {
    entries: Vec<RefinementResult>,
    by_id: BTreeMap<String, usize>,
}

impl RefinementLog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&mut self, result: RefinementResult) {
        if !self.by_id.contains_key(&result.id) {
            self.by_id.insert(result.id.clone(), self.entries.len());
            self.entries.push(result);
        }
    }

    pub fn get(&self, id: &str) -> Option<&RefinementResult> {
        self.by_id.get(id).map(|idx| &self.entries[*idx])
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn all(&self) -> &[RefinementResult] {
        &self.entries
    }

    /// Last N refinements, newest first.
    pub fn recent(&self, n: usize) -> impl Iterator<Item = &RefinementResult> {
        self.entries.iter().rev().take(n)
    }

    /// Roll back to the state captured before refinement `id` was applied.
    ///
    /// Returns the restored baseline. Refinements applied after `id` are not
    /// reverted (mirroring Prime Agent's rollback-by-ID, which is a restore of
    /// the snapshot, not a history rewrite).
    pub fn rollback(&self, id: &str) -> Option<HarnessState> {
        self.get(id).map(|r| r.baseline_state.clone())
    }

    // ---- persistence -------------------------------------------------

    pub fn load(path: &Path) -> std::io::Result<Self> {
        if !path.exists() {
            return Ok(Self::new());
        }
        let raw = std::fs::read_to_string(path)?;
        let log: RefinementLog = serde_json::from_str(&raw).map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("refinement log parse error: {e}"),
            )
        })?;
        Ok(log)
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    /// Default log file location inside a session dir.
    pub fn default_path(session_dir: &Path) -> PathBuf {
        session_dir.join("refinement_log.json")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::HarnessKind;

    fn result(id: &str, baseline: HarnessState) -> RefinementResult {
        RefinementResult {
            id: id.to_string(),
            kind: HarnessKind::Memory,
            action: RefinementAction::Create,
            title: "t".to_string(),
            content: "c".to_string(),
            trigger: "test".to_string(),
            outcome: "ok".to_string(),
            baseline_state: baseline,
            applied_at: "now".to_string(),
        }
    }

    #[test]
    fn record_and_lookup() {
        let mut log = RefinementLog::new();
        let base = HarnessState::new();
        log.record(result("r1", base.clone()));
        log.record(result("r2", base));
        assert_eq!(log.len(), 2);
        assert!(log.get("r1").is_some());
        assert!(log.get("nope").is_none());
    }

    #[test]
    fn rollback_restores_baseline() {
        let mut log = RefinementLog::new();
        let mut base = HarnessState::new();
        base.create(HarnessKind::Prompt, "p", "original", None);
        log.record(result("r1", base.clone()));

        let restored = log.rollback("r1").unwrap();
        assert_eq!(restored, base);
        assert_eq!(restored.list(HarnessKind::Prompt)[0].content, "original");
    }

    #[test]
    fn save_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/refinement_log.json");
        let mut log = RefinementLog::new();
        log.record(result("r1", HarnessState::new()));
        log.save(&path).unwrap();

        let loaded = RefinementLog::load(&path).unwrap();
        assert_eq!(loaded.len(), 1);
        assert!(loaded.get("r1").is_some());
    }

    #[test]
    fn load_missing_returns_empty() {
        let dir = tempfile::tempdir().unwrap();
        let log = RefinementLog::load(&dir.path().join("nope.json")).unwrap();
        assert!(log.is_empty());
    }

    #[test]
    fn recent_newest_first() {
        let mut log = RefinementLog::new();
        log.record(result("r1", HarnessState::new()));
        log.record(result("r2", HarnessState::new()));
        let rec: Vec<_> = log.recent(2).map(|r| r.id.as_str()).collect();
        assert_eq!(rec, vec!["r2", "r1"]);
    }
}
