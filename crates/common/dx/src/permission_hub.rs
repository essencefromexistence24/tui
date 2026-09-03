//! Interactive permission gate for the multi-step agent loop.
//!
//! The async loop parks until the TUI records a decision (y / a / n),
//! or until the timeout elapses (deny).

use std::{
	sync::{Arc, Mutex},
	time::{Duration, Instant},
};

use crate::tools::PermissionDecision;

#[derive(Debug, Clone)]
pub struct PendingPermission {
	pub tool: String,
	pub preview: String,
	pub requested_at: Instant,
}

#[derive(Debug, Default)]
struct HubInner {
	pending: Option<PendingPermission>,
	decision: Option<PermissionDecision>,
	/// Tools permanently allowed this session ("always").
	always: Vec<String>,
}

/// Shared between the agent task and the UI thread.
#[derive(Clone, Default)]
pub struct PermissionHub {
	inner: Arc<Mutex<HubInner>>,
}

impl PermissionHub {
	pub fn new() -> Self {
		Self::default()
	}

	pub fn clear(&self) {
		if let Ok(mut g) = self.inner.lock() {
			g.pending = None;
			g.decision = None;
		}
	}

	pub fn is_always_allowed(&self, tool: &str, preview: &str) -> bool {
		let Ok(g) = self.inner.lock() else {
			return false;
		};
		let key = format!("{tool}:{preview}");
		g.always.iter().any(|a| a == tool || a == &key)
	}

	/// Block (async) until the user answers or timeout. Emits via `on_request` once.
	pub async fn request(
		&self,
		tool: &str,
		preview: &str,
		timeout: Duration,
		on_request: impl FnOnce(),
	) -> PermissionDecision {
		if self.is_always_allowed(tool, preview) {
			return PermissionDecision::AllowAlways;
		}

		{
			let Ok(mut g) = self.inner.lock() else {
				return PermissionDecision::Deny;
			};
			g.decision = None;
			g.pending = Some(PendingPermission {
				tool: tool.to_string(),
				preview: preview.to_string(),
				requested_at: Instant::now(),
			});
		}
		on_request();

		let start = Instant::now();
		loop {
			if let Ok(mut g) = self.inner.lock()
				&& let Some(d) = g.decision.take()
			{
				g.pending = None;
				if matches!(d, PermissionDecision::AllowAlways) {
					let key = format!("{tool}:{preview}");
					if !g.always.iter().any(|a| a == &key || a == tool) {
						g.always.push(key);
					}
				}
				return d;
			}
			if start.elapsed() >= timeout {
				if let Ok(mut g) = self.inner.lock() {
					g.pending = None;
					g.decision = None;
				}
				return PermissionDecision::Deny;
			}
			tokio::time::sleep(Duration::from_millis(40)).await;
		}
	}

	pub fn pending(&self) -> Option<PendingPermission> {
		self.inner.lock().ok().and_then(|g| g.pending.clone())
	}

	pub fn reply(&self, decision: PermissionDecision) -> bool {
		let Ok(mut g) = self.inner.lock() else {
			return false;
		};
		if g.pending.is_none() {
			return false;
		}
		g.decision = Some(decision);
		true
	}
}

/// IPC markers on the agent stream channel.
pub const PERM_REQ_PREFIX: &str = "\n__PERM_REQ__\n";
pub const QUESTION_REQ_PREFIX: &str = "\n__QUESTION_REQ__\n";
pub const INTERRUPTED_MARKER: &str = "\n__INTERRUPTED__\n";
pub const COMPACTION_MARKER: &str = "\n__COMPACTION__\n";
pub const ERROR_CARD_PREFIX: &str = "\n__ERROR_CARD__\n";
pub const RETRY_HINT_PREFIX: &str = "\n__RETRY_HINT__\n";
