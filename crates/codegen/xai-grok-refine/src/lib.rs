//! # xai-grok-refine — Continual Harness for DX
//!
//! Evidence-backed CRUD refinement of the agent's own harness layer
//! (prompt notes, memory, skills, subagent specs) with snapshot rollback.
//!
//! The base system prompt is immutable; everything here is the additive
//! harness layer. Every mutation snapshots the prior state into an append-only
//! refinement log, so a bad edit can be rolled back by refinement ID.
//!
//! ## Layout
//!
//! - [`state`] — `HarnessState`, the four collections + typed CRUD.
//! - [`log`] — append-only `RefinementLog` with snapshot rollback.
//! - [`prompt`] — doctrine + harness-section prompt builders.
//! - [`rhai`] — `harness.*` / `refine.*` registration on a Rhai engine
//!   (mirrors `xai-workflow` host-function style).
//!
//! ## Usage
//!
//! ```no_run
//! use xai_grok_refine::RefineSession;
//!
//! let mut session = RefineSession::new(None);
//! session.create_memory("flaky tests", "retry three times before failing", None);
//! let id = session.last_refinement_id().unwrap();
//! session.rollback(&id);
//! ```

pub mod log;
pub mod prompt;
pub mod rhai;
pub mod state;

use log::{RefinementAction, RefinementLog, RefinementResult};
use ::rhai::Dynamic;
use state::{HarnessEntry, HarnessKind, HarnessState};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Errors from the refine session.
#[derive(Debug, thiserror::Error)]
pub enum RefineError {
    #[error("persistence error: {0}")]
    Persistence(String),
    #[error("no entry for {kind}/{id}")]
    NotFound { kind: HarnessKind, id: String },
    #[error("unknown refinement id: {0}")]
    UnknownRefinement(String),
}

/// Owns the harness state + refinement log and applies every mutation as a
/// recorded, reversible refinement.
#[derive(Debug, Default)]
pub struct RefineSession {
    state: HarnessState,
    log: RefinementLog,
    seq: u64,
    state_path: Option<PathBuf>,
    log_path: Option<PathBuf>,
}

impl RefineSession {
    pub fn new(state_path: Option<PathBuf>) -> Self {
        let log_path = state_path.as_ref().map(|p| log_path_from(p));
        Self {
            state: HarnessState::new(),
            log: RefinementLog::new(),
            seq: 0,
            state_path,
            log_path,
        }
    }

    /// Loads state + log from disk if present; otherwise starts empty.
    pub fn load_or_default(session_dir: Option<&Path>) -> Self {
        let Some(dir) = session_dir else {
            return Self::new(None);
        };
        let state_path = dir.join("harness_state.json");
        let log_path = dir.join("refinement_log.json");

        let state = std::fs::read_to_string(&state_path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default();
        let log = RefinementLog::load(&log_path).unwrap_or_default();
        let seq = log.len() as u64;

        Self {
            state,
            log,
            seq,
            state_path: Some(state_path),
            log_path: Some(log_path),
        }
    }

    pub fn state(&self) -> &HarnessState {
        &self.state
    }

    pub fn log(&self) -> &RefinementLog {
        &self.log
    }

    pub fn last_refinement_id(&self) -> Option<String> {
        self.log.all().last().map(|r| r.id.clone())
    }

    fn persist(&self) -> Result<(), RefineError> {
        if let Some(p) = &self.state_path {
            write_json(p, &self.state)?;
        }
        if let Some(p) = &self.log_path {
            self.log.save(p).map_err(|e| RefineError::Persistence(e.to_string()))?;
        }
        Ok(())
    }

    fn next_id(&mut self) -> String {
        self.seq += 1;
        format!("refine-{seq:05}", seq = self.seq)
    }

    fn record(&mut self, kind: HarnessKind, action: RefinementAction, title: &str, content: &str, trigger: &str, baseline: HarnessState, outcome: &str) -> Result<String, RefineError> {
        let id = self.next_id();
        let result = RefinementResult {
            id: id.clone(),
            kind,
            action,
            title: title.to_string(),
            content: content.to_string(),
            trigger: trigger.to_string(),
            outcome: outcome.to_string(),
            baseline_state: baseline,
            applied_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        };
        self.log.record(result);
        self.persist()?;
        Ok(id)
    }

    // ---- CRUD (all recorded + reversible) -----------------------------

    pub fn create(
        &mut self,
        kind: HarnessKind,
        title: &str,
        content: &str,
        evidence: Option<String>,
        trigger: &str,
    ) -> Result<HarnessEntry, RefineError> {
        let baseline = self.state.clone();
        let entry = self
            .state
            .create(kind, title.to_string(), content.to_string(), evidence);
        self.record(kind, RefinementAction::Create, title, content, trigger, baseline, "created")?;
        Ok(entry)
    }

    pub fn create_memory(
        &mut self,
        title: &str,
        content: &str,
        evidence: Option<String>,
    ) -> Result<HarnessEntry, RefineError> {
        self.create(HarnessKind::Memory, title, content, evidence, "user")
    }

    pub fn update(
        &mut self,
        kind: HarnessKind,
        id: &str,
        content: &str,
        trigger: &str,
    ) -> Result<Option<HarnessEntry>, RefineError> {
        let baseline = self.state.clone();
        let Some(entry) = self.state.update(kind, id, content.to_string()) else {
            return Ok(None);
        };
        self.record(kind, RefinementAction::Update, &entry.title, content, trigger, baseline, "updated")?;
        Ok(Some(entry))
    }

    pub fn delete(
        &mut self,
        kind: HarnessKind,
        id: &str,
        trigger: &str,
    ) -> Result<Option<HarnessEntry>, RefineError> {
        let baseline = self.state.clone();
        let Some(entry) = self.state.delete(kind, id) else {
            return Ok(None);
        };
        self.record(kind, RefinementAction::Delete, &entry.title, "", trigger, baseline, "deleted")?;
        Ok(Some(entry))
    }

    pub fn record_from_map(
        &mut self,
        map: BTreeMap<String, Dynamic>,
        trigger: &str,
    ) -> Result<String, RefineError> {
        let get_str = |key: &str, default: &str| -> String {
            map.get(key)
                .and_then(|v| v.clone().try_cast::<String>())
                .unwrap_or_else(|| default.to_string())
        };
        let kind = rhai::kind_from_str(&get_str("kind", "memory")).unwrap_or(HarnessKind::Memory);
        let action = rhai::action_from_str(&get_str("action", "create"));
        let title = get_str("title", "");
        let content = get_str("content", "");
        let baseline = self.state.clone();
        let outcome = get_str("outcome", "");
        self.record(kind, action, &title, &content, trigger, baseline, &outcome)
    }

    pub fn rollback(&mut self, id: &str) -> Result<(), RefineError> {
        let baseline = self
            .log
            .rollback(id)
            .ok_or_else(|| RefineError::UnknownRefinement(id.to_string()))?;
        self.state = baseline;
        self.persist()
    }
}

fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), RefineError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| RefineError::Persistence(e.to_string()))?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(value).unwrap())
        .map_err(|e| RefineError::Persistence(e.to_string()))?;
    std::fs::rename(&tmp, path).map_err(|e| RefineError::Persistence(e.to_string()))
}

fn log_path_from(state_path: &Path) -> PathBuf {
    let dir = state_path.parent().unwrap_or_else(|| Path::new("."));
    dir.join("refinement_log.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> RefineSession {
        RefineSession::new(None)
    }

    #[test]
    fn every_create_is_recorded_and_rollbackable() {
        let mut s = session();
        s.create_memory("flaky", "retry 3x", None).unwrap();
        let id = s.last_refinement_id().unwrap();
        assert_eq!(s.state().list(HarnessKind::Memory).len(), 1);

        s.rollback(&id).unwrap();
        assert_eq!(s.state().list(HarnessKind::Memory).len(), 0);
    }

    #[test]
    fn rollback_unknown_id_errors() {
        let mut s = session();
        assert!(s.rollback("nope").is_err());
    }

    #[test]
    fn load_or_default_persists_across_sessions() {
        let dir = tempfile::tempdir().unwrap();
        {
            let mut s = RefineSession::load_or_default(Some(dir.path()));
            s.create_memory("lesson", "compile first", Some("traj-7".to_string())).unwrap();
        }
        {
            let s = RefineSession::load_or_default(Some(dir.path()));
            assert_eq!(s.state().list(HarnessKind::Memory).len(), 1);
            assert_eq!(s.state().list(HarnessKind::Memory)[0].content, "compile first");
            assert!(!s.log().is_empty());
        }
    }

    #[test]
    fn load_or_default_missing_dir_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let s = RefineSession::load_or_default(Some(&dir.path().join("nope")));
        assert!(s.state().is_empty());
    }

    #[test]
    fn update_and_delete_recorded() {
        let mut s = session();
        let e = s.create_memory("a", "1", None).unwrap();
        s.update(HarnessKind::Memory, &e.id, "2", "test").unwrap();
        assert_eq!(s.state().list(HarnessKind::Memory)[0].content, "2");
        let id = s.last_refinement_id().unwrap();
        s.rollback(&id).unwrap();
        assert_eq!(s.state().list(HarnessKind::Memory)[0].content, "1");

        s.delete(HarnessKind::Memory, &e.id, "test").unwrap();
        assert!(s.state().list(HarnessKind::Memory).is_empty());
    }
}
