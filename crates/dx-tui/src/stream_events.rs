//! Structured agent→UI stream events (live tools, permissions, questions).
//!
//! Wire format (one chunk):
//! ```text
//! \n__STREAM_EVENT__\n{json}\n
//! ```
//! Kept alongside legacy fence markers so older parsers still work.

use serde::{Deserialize, Serialize};

pub const STREAM_EVENT_PREFIX: &str = "\n__STREAM_EVENT__\n";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamEvent {
	/// Append stdout/stderr chunk to a running tool card.
	ToolDelta { id: String, chunk: String },
	/// Mark a tool finished (optional; usually paired with a result fence).
	ToolEnd {
		id: String,
		ok: bool,
		#[serde(default)]
		duration_ms: u64,
	},
	/// Permission request for in-stream controls.
	Permission {
		tool: String,
		preview: String,
		#[serde(default)]
		call_id: String,
	},
	/// Permission resolved (update card).
	PermissionResolved {
		#[serde(default)]
		call_id: String,
		decision: String,
	},
	/// Ask-user question form.
	Question { id: String, prompt: String, options: Vec<String> },
	/// Subagent nested tool note (optional metadata).
	SubagentMeta {
		name: String,
		#[serde(default)]
		status: String,
	},
}

impl StreamEvent {
	pub fn encode(&self) -> String {
		match serde_json::to_string(self) {
			Ok(j) => format!("{STREAM_EVENT_PREFIX}{j}\n"),
			Err(_) => String::new(),
		}
	}

	pub fn decode_chunk(chunk: &str) -> Option<Self> {
		let rest = chunk.strip_prefix(STREAM_EVENT_PREFIX)?;
		let line = rest.lines().next()?.trim();
		serde_json::from_str(line).ok()
	}
}

/// Append `chunk` into the body of a running ```command id="…" status="running" fence.
/// Returns true if a matching running fence was updated.
pub fn append_tool_delta(content: &mut String, tool_id: &str, chunk: &str) -> bool {
	if tool_id.is_empty() || chunk.is_empty() {
		return false;
	}
	let id_tok = format!("id=\"{tool_id}\"");
	let mut search_from = 0usize;
	let mut target: Option<(usize, usize)> = None; // (body_start, fence_close_at_backticks)

	while let Some(rel) = content[search_from..].find("```command") {
		let start = search_from + rel;
		let after = start + "```command".len();
		let header_end = content[after..].find('\n').map(|i| after + i).unwrap_or(content.len());
		let header = &content[start..header_end];
		let is_running = header.contains("status=\"running\"") || header.contains("status=running");
		if is_running && header.contains(&id_tok) {
			let body_start = if header_end < content.len() { header_end + 1 } else { header_end };
			if let Some(close_rel) = content[body_start..].find("\n```") {
				let close_at = body_start + close_rel; // points at \n before ```
				target = Some((body_start, close_at));
			}
		}
		search_from = after;
	}

	let Some((_body_start, close_at)) = target else {
		return false;
	};

	// Insert before closing fence
	let mut next = String::with_capacity(content.len() + chunk.len());
	next.push_str(&content[..close_at]);
	// Ensure body ends with newline before new chunk if needed
	if !next.ends_with('\n') {
		next.push('\n');
	}
	// Drop leading newlines from chunk to avoid huge gaps; keep internal structure
	let c = chunk.trim_start_matches('\r');
	next.push_str(c);
	if !c.ends_with('\n') {
		next.push('\n');
	}
	next.push_str(&content[close_at..]);
	*content = next;
	true
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn roundtrip_tool_delta() {
		let ev = StreamEvent::ToolDelta { id: "c1".into(), chunk: "hello\n".into() };
		let enc = ev.encode();
		let dec = StreamEvent::decode_chunk(&enc).expect("decode");
		match dec {
			StreamEvent::ToolDelta { id, chunk } => {
				assert_eq!(id, "c1");
				assert_eq!(chunk, "hello\n");
			}
			_ => panic!("wrong variant"),
		}
	}

	#[test]
	fn append_into_running_fence() {
		let mut content = String::from(
			"\n```command id=\"abc\" name=\"shell\" title=\"Terminal\" status=\"running\"\n$ echo hi\n```\n",
		);
		assert!(append_tool_delta(&mut content, "abc", "hi\n"));
		assert!(content.contains("hi\n"));
		assert!(content.contains("status=\"running\""));
		// still one fence
		assert_eq!(content.matches("```command").count(), 1);
	}
}
