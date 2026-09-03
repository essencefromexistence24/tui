//! Ask-user question dock (OpenCode-shaped, simplified).

use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct PendingQuestion {
	pub id: String,
	pub prompt: String,
	pub options: Vec<String>,
	/// Selected option index (or custom text in `custom`).
	pub selected: usize,
	pub custom: String,
	pub allow_custom: bool,
}

#[derive(Debug, Default)]
struct Inner {
	pending: Option<PendingQuestion>,
	/// Reply payload once answered: joined answers.
	reply: Option<String>,
	rejected: bool,
}

#[derive(Clone, Default)]
pub struct QuestionHub {
	inner: Arc<Mutex<Inner>>,
}

impl QuestionHub {
	pub fn new() -> Self {
		Self::default()
	}

	pub fn clear(&self) {
		if let Ok(mut g) = self.inner.lock() {
			*g = Inner::default();
		}
	}

	pub fn ask(&self, id: impl Into<String>, prompt: impl Into<String>, options: Vec<String>) {
		if let Ok(mut g) = self.inner.lock() {
			g.reply = None;
			g.rejected = false;
			g.pending = Some(PendingQuestion {
				id: id.into(),
				prompt: prompt.into(),
				options,
				selected: 0,
				custom: String::new(),
				allow_custom: true,
			});
		}
	}

	pub fn pending(&self) -> Option<PendingQuestion> {
		self.inner.lock().ok().and_then(|g| g.pending.clone())
	}

	pub fn move_selection(&self, delta: i32) {
		if let Ok(mut g) = self.inner.lock()
			&& let Some(ref mut q) = g.pending
		{
			let n = q.options.len().max(1) as i32;
			let mut i = q.selected as i32 + delta;
			if i < 0 {
				i = n - 1;
			}
			if i >= n {
				i = 0;
			}
			q.selected = i as usize;
		}
	}

	pub fn set_custom(&self, text: String) {
		if let Ok(mut g) = self.inner.lock()
			&& let Some(ref mut q) = g.pending
		{
			q.custom = text;
		}
	}

	/// Confirm current selection (or custom if non-empty).
	pub fn confirm(&self) -> Option<String> {
		let Ok(mut g) = self.inner.lock() else {
			return None;
		};
		let q = g.pending.take()?;
		let answer = if !q.custom.trim().is_empty() {
			q.custom.trim().to_string()
		} else {
			q.options.get(q.selected).cloned().unwrap_or_else(|| "ok".into())
		};
		g.reply = Some(answer.clone());
		Some(answer)
	}

	pub fn reject(&self) {
		if let Ok(mut g) = self.inner.lock() {
			g.pending = None;
			g.rejected = true;
			g.reply = Some(String::from("(user dismissed question)"));
		}
	}

	/// Block until answered (used by tool execute path via async poll).
	pub async fn wait_reply(&self, timeout: std::time::Duration) -> Option<String> {
		let start = std::time::Instant::now();
		loop {
			if let Ok(mut g) = self.inner.lock()
				&& let Some(r) = g.reply.take()
			{
				g.pending = None;
				return Some(r);
			}
			if start.elapsed() >= timeout {
				if let Ok(mut g) = self.inner.lock() {
					g.pending = None;
				}
				return None;
			}
			tokio::time::sleep(std::time::Duration::from_millis(40)).await;
		}
	}
}
