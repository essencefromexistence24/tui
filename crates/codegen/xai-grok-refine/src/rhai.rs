//! Rhai engine registration for the continual harness (`harness.*`) and
//! refinement (`refine.*`) surfaces, mirroring how `xai-workflow` registers
//! host functions on its engine.
//!
//! The model-facing surface matches Prime Agent's naming so the doctrine
//! prompt stays recognizable, but every call routes through the native Rust
//! [`RefineSession`](crate::RefineSession), which snapshots state before each
//! mutation so any edit is reversible.

use crate::log::RefinementAction;
use crate::state::HarnessKind;
use crate::RefineSession;
use ::rhai::Engine;
use ::rhai::Dynamic;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

/// Registers `harness.*` CRUD + `refine.*` on the given engine.
///
/// `session` must be an `Arc<Mutex<RefineSession>>` so script calls mutate
/// the same session the host owns.
pub fn register_refine_fns(engine: &mut Engine, session: &Arc<Mutex<RefineSession>>) {
    let ctx = session.clone();
    engine.register_fn("harness_create", move |kind: &str, title: &str, content: &str| {
        let kind = match HarnessKind::parse(kind) {
            Some(k) => k,
            None => return format!("Error: unknown harness kind: {kind}"),
        };
        let mut s = ctx.lock().unwrap();
        match s.create(kind, title, content, None, "script") {
            Ok(entry) => format!("created {} {}/{}", kind.as_str(), entry.id, entry.title),
            Err(e) => format!("Error: {e}"),
        }
    });

    let ctx = session.clone();
    engine.register_fn("harness_update", move |kind: &str, id: &str, content: &str| {
        let kind = match HarnessKind::parse(kind) {
            Some(k) => k,
            None => return format!("Error: unknown harness kind: {kind}"),
        };
        let mut s = ctx.lock().unwrap();
        match s.update(kind, id, content, "script") {
            Ok(Some(entry)) => format!("updated {} {}/{}", kind.as_str(), entry.id, entry.title),
            Ok(None) => format!("Error: no {}/{id} entry", kind.as_str()),
            Err(e) => format!("Error: {e}"),
        }
    });

    let ctx = session.clone();
    engine.register_fn("harness_delete", move |kind: &str, id: &str| {
        let kind = match HarnessKind::parse(kind) {
            Some(k) => k,
            None => return format!("Error: unknown harness kind: {kind}"),
        };
        let mut s = ctx.lock().unwrap();
        match s.delete(kind, id, "script") {
            Ok(Some(entry)) => format!("deleted {} {}/{}", kind.as_str(), entry.id, entry.title),
            Ok(None) => format!("Error: no {}/{id} entry", kind.as_str()),
            Err(e) => format!("Error: {e}"),
        }
    });

    let ctx = session.clone();
    engine.register_fn("harness_list", move |kind: &str| {
        let kind = match HarnessKind::parse(kind) {
            Some(k) => k,
            None => return format!("Error: unknown harness kind: {kind}"),
        };
        let s = ctx.lock().unwrap();
        let entries = s.state().list(kind);
        if entries.is_empty() {
            format!("no {} entries", kind.as_str())
        } else {
            entries
                .iter()
                .map(|e| format!("{}: {}", e.id, e.title))
                .collect::<Vec<_>>()
                .join("\n")
        }
    });

    let ctx = session.clone();
    engine.register_fn("harness_get", move |kind: &str, id: &str| {
        let kind = match HarnessKind::parse(kind) {
            Some(k) => k,
            None => return format!("Error: unknown harness kind: {kind}"),
        };
        let s = ctx.lock().unwrap();
        match s.state().get(kind, id) {
            Some(e) => format!("# {}\n{}", e.title, e.content),
            None => format!("Error: no {}/{id} entry", kind.as_str()),
        }
    });

    let ctx = session.clone();
    engine.register_fn("harness_overview", move || {
        let s = ctx.lock().unwrap();
        s.state().overview(crate::prompt::HARNESS_OVERVIEW_MAX_CHARS)
    });

    let ctx = session.clone();
    engine.register_fn("harness_record_refinement", move |map: BTreeMap<String, Dynamic>| {
        let mut s = ctx.lock().unwrap();
        match s.record_from_map(map, "script") {
            Ok(id) => format!("recorded refinement {id}"),
            Err(e) => format!("Error: {e}"),
        }
    });

    let ctx = session.clone();
    engine.register_fn("refine_rollback", move |id: &str| {
        let mut s = ctx.lock().unwrap();
        match s.rollback(id) {
            Ok(_) => format!("rolled back refinement {id}"),
            Err(e) => format!("Error: {e}"),
        }
    });

    let ctx = session.clone();
    engine.register_fn("refine_status", move || {
        let s = ctx.lock().unwrap();
        let log = s.log();
        if log.is_empty() {
            "no refinements yet".to_string()
        } else {
            log.recent(10)
                .map(|r| {
                    format!(
                        "{} {} {} ({})",
                        r.id,
                        r.kind.as_str(),
                        r.action.as_str(),
                        r.trigger
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        }
    });
}

/// Builds the model-facing surface names from a `kind` string.
pub(crate) fn kind_from_str(s: &str) -> Option<HarnessKind> {
    HarnessKind::parse(s)
}

/// Converts a script-provided action string to the typed action.
pub(crate) fn action_from_str(s: &str) -> RefinementAction {
    match s {
        "update" => RefinementAction::Update,
        "delete" => RefinementAction::Delete,
        _ => RefinementAction::Create,
    }
}
