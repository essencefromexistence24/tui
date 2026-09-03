//! Live in-memory transcript: keep `Vec<StreamPart>` as paint source of truth

#![allow(dead_code)]
//! while still serializing to fences in `Message.content` for session persistence.

use std::time::Duration;

use super::ansi::strip_ansi;
use super::parse::{PartStatus, StreamPart, parse_stream_parts};

/// Rebuild parts from wire content (source of truth after any content mutation).
pub fn rebuild_parts(content: &str, thinking_duration: Option<Duration>) -> Vec<StreamPart> {
	parse_stream_parts(content, thinking_duration)
}

/// Append a text/answer delta into the last Text part (or create one).
pub fn append_text_part(parts: &mut Vec<StreamPart>, delta: &str) {
	if delta.is_empty() {
		return;
	}
	if let Some(StreamPart::Text { body }) = parts.last_mut() {
		body.push_str(delta);
		return;
	}
	parts.push(StreamPart::Text { body: delta.to_string() });
}

/// Append thinking delta into the last open Thinking part (streaming).
pub fn append_thinking_part(parts: &mut Vec<StreamPart>, delta: &str, duration: Option<Duration>) {
	if let Some(StreamPart::Thinking { body, streaming, duration: d, .. }) = parts.last_mut() {
		body.push_str(delta);
		*streaming = true;
		if duration.is_some() {
			*d = duration;
		}
		return;
	}
	let idx = parts.iter().filter(|p| matches!(p, StreamPart::Thinking { .. })).count();
	parts.push(StreamPart::Thinking {
		index: idx,
		body: delta.to_string(),
		duration,
		streaming: true,
	});
}

/// Close last thinking block.
pub fn close_thinking_part(parts: &mut [StreamPart], duration: Option<Duration>) {
	if let Some(StreamPart::Thinking { streaming, duration: d, .. }) = parts.last_mut() {
		*streaming = false;
		if duration.is_some() {
			*d = duration;
		}
	}
}

/// Upsert a running tool card by id.
pub fn upsert_running_tool(
	parts: &mut Vec<StreamPart>,
	id: &str,
	name: &str,
	title: &str,
	preview: &str,
) {
	if let Some(StreamPart::Tool { status, preview: p, title: t, name: n, .. }) =
		parts.iter_mut().rev().find(|p| match p {
			StreamPart::Tool { id: tid, .. } => tid == id,
			_ => false,
		}) {
		*status = PartStatus::Running;
		if !preview.is_empty() {
			*p = preview.to_string();
		}
		if !title.is_empty() {
			*t = title.to_string();
		}
		if !name.is_empty() {
			*n = name.to_string();
		}
		return;
	}
	let index = parts.iter().filter(|p| matches!(p, StreamPart::Tool { .. })).count();
	parts.push(StreamPart::Tool {
		id: id.to_string(),
		index,
		name: name.to_string(),
		title: if title.is_empty() { name.to_string() } else { title.to_string() },
		status: PartStatus::Running,
		preview: preview.to_string(),
		body: String::new(),
		duration: None,
	});
}

/// Live stdout/stderr into a running tool body.
pub fn append_tool_body(parts: &mut [StreamPart], id: &str, chunk: &str) -> bool {
	for p in parts.iter_mut().rev() {
		if let StreamPart::Tool { id: tid, body, status, .. } = p
			&& tid == id
		{
			body.push_str(chunk);
			*status = PartStatus::Running;
			return true;
		}
	}
	false
}

/// Finish a tool card from a result.
#[allow(clippy::too_many_arguments)]
pub fn finish_tool(
	parts: &mut Vec<StreamPart>,
	id: &str,
	name: &str,
	title: &str,
	ok: bool,
	body: &str,
	preview: &str,
	duration: Option<Duration>,
) {
	let status = if ok { PartStatus::Done } else { PartStatus::Error };
	let found = parts.iter_mut().rev().find(|p| match p {
		StreamPart::Tool { id: tid, name: nn, status: s, .. } => {
			tid == id || (nn == name && *s == PartStatus::Running)
		}
		_ => false,
	});
	if let Some(StreamPart::Tool {
		status: st,
		body: b,
		title: t,
		preview: p,
		name: n,
		duration: d,
		..
	}) = found
	{
		*st = status;
		if !body.is_empty() {
			*b = body.to_string();
		}
		if !title.is_empty() {
			*t = title.to_string();
		}
		if !preview.is_empty() {
			*p = preview.to_string();
		}
		if !name.is_empty() {
			*n = name.to_string();
		}
		*d = duration;
		return;
	}
	let index = parts.iter().filter(|p| matches!(p, StreamPart::Tool { .. })).count();
	parts.push(StreamPart::Tool {
		id: if id.is_empty() { format!("tool-{index}") } else { id.to_string() },
		index,
		name: name.to_string(),
		title: title.to_string(),
		status,
		preview: preview.to_string(),
		body: body.to_string(),
		duration,
	});
}

/// Open / append subagent body (nested tools stay as text until re-parse of body,
/// or we can push nested tool parts as siblings under a marker).
pub fn open_subagent(parts: &mut Vec<StreamPart>, name: &str) {
	let index = parts.iter().filter(|p| matches!(p, StreamPart::Subagent { .. })).count();
	parts.push(StreamPart::Subagent {
		index,
		name: name.to_string(),
		body: String::new(),
		status: PartStatus::Running,
	});
}

pub fn append_subagent_body(parts: &mut [StreamPart], chunk: &str) {
	if let Some(StreamPart::Subagent { body, .. }) = parts
		.iter_mut()
		.rev()
		.find(|p| matches!(p, StreamPart::Subagent { status: PartStatus::Running, .. }))
	{
		body.push_str(chunk);
	}
}

pub fn close_subagent(parts: &mut [StreamPart], ok: bool) {
	if let Some(StreamPart::Subagent { status, .. }) = parts
		.iter_mut()
		.rev()
		.find(|p| matches!(p, StreamPart::Subagent { status: PartStatus::Running, .. }))
	{
		*status = if ok { PartStatus::Done } else { PartStatus::Error };
	}
}

pub fn push_approval(parts: &mut Vec<StreamPart>, call_id: &str, tool: &str, body: &str) {
	parts.push(StreamPart::Approval {
		call_id: call_id.to_string(),
		tool: tool.to_string(),
		body: body.to_string(),
		decision: "pending".into(),
	});
}

pub fn resolve_approval(parts: &mut [StreamPart], call_id: &str, decision: &str) {
	for p in parts.iter_mut().rev() {
		if let StreamPart::Approval { call_id: id, decision: d, .. } = p
			&& (id == call_id || call_id.is_empty())
		{
			*d = decision.to_string();
			return;
		}
	}
}

pub fn push_question(parts: &mut Vec<StreamPart>, id: &str, prompt: &str, options: Vec<String>) {
	parts.push(StreamPart::Question {
		id: id.to_string(),
		prompt: prompt.to_string(),
		options,
		answer: String::new(),
	});
}

pub fn answer_question(parts: &mut [StreamPart], id: &str, answer: &str) {
	for p in parts.iter_mut().rev() {
		if let StreamPart::Question { id: qid, answer: a, .. } = p
			&& (qid == id || id.is_empty())
		{
			*a = answer.to_string();
			return;
		}
	}
}

pub fn push_plan(parts: &mut Vec<StreamPart>, body: &str) {
	let steps =
		super::parse::parse_stream_parts(&format!("```plan title=\"Plan\"\n{body}\n```\n"), None)
			.into_iter()
			.find_map(|p| match p {
				StreamPart::Plan { steps, .. } => Some(steps),
				_ => None,
			})
			.unwrap_or_default();
	parts.push(StreamPart::Plan { title: "Plan".into(), body: body.to_string(), steps });
}

pub fn push_pty(
	parts: &mut Vec<StreamPart>,
	id: &str,
	title: &str,
	lines: Vec<String>,
	attached: bool,
	alive: bool,
) {
	// Replace existing pty with same id
	if let Some(pos) =
		parts.iter().position(|p| matches!(p, StreamPart::Pty { id: pid, .. } if pid == id))
	{
		parts[pos] =
			StreamPart::Pty { id: id.to_string(), title: title.to_string(), lines, attached, alive };
		return;
	}
	parts.push(StreamPart::Pty {
		id: id.to_string(),
		title: title.to_string(),
		lines,
		attached,
		alive,
	});
}

pub fn push_interrupted(parts: &mut Vec<StreamPart>) {
	parts.push(StreamPart::Interrupted);
}

pub fn push_error(parts: &mut Vec<StreamPart>, body: &str) {
	parts.push(StreamPart::Error { body: body.to_string() });
}

/// Serialize parts back to wire content (session save / LLM-agnostic export).
pub fn parts_to_wire(parts: &[StreamPart]) -> String {
	let mut out = String::new();
	for p in parts {
		match p {
			StreamPart::Text { body } => {
				if !out.is_empty() && !out.ends_with('\n') {
					out.push('\n');
				}
				out.push_str(body);
				if !body.ends_with('\n') {
					out.push('\n');
				}
			}
			StreamPart::Thinking { body, .. } => {
				out.push_str("<think>\n");
				out.push_str(body);
				if !body.ends_with('\n') {
					out.push('\n');
				}
				out.push_str("</think>\n");
			}
			StreamPart::Tool { id, name, title, status, preview, body, duration, .. } => {
				let st = match status {
					PartStatus::Running => "running",
					PartStatus::Done => "done",
					PartStatus::Error => "error",
				};
				let dur =
					duration.map(|d| format!(" duration_ms=\"{}\"", d.as_millis())).unwrap_or_default();
				let prev = if preview.is_empty() { String::new() } else { format!(" {preview}") };
				out.push_str(&format!(
					"```command id=\"{id}\" name=\"{name}\" title=\"{title}\" status=\"{st}\"{dur}{prev}\n"
				));
				out.push_str(body);
				if !body.ends_with('\n') {
					out.push('\n');
				}
				out.push_str("```\n");
			}
			StreamPart::Subagent { name, body, .. } => {
				out.push_str(&format!("<subagent name=\"{name}\">\n"));
				out.push_str(body);
				if !body.ends_with('\n') {
					out.push('\n');
				}
				out.push_str("</subagent>\n");
			}
			StreamPart::Approval { call_id, tool, body, .. } => {
				out.push_str(&format!("```approval id=\"{call_id}\" tool=\"{tool}\"\n"));
				out.push_str(body);
				if !body.ends_with('\n') {
					out.push('\n');
				}
				out.push_str("```\n");
			}
			StreamPart::Question { id, prompt, options, answer } => {
				out.push_str(&format!("```question id=\"{id}\"\n{prompt}\n"));
				for (i, o) in options.iter().enumerate() {
					out.push_str(&format!("  {}. {o}\n", i + 1));
				}
				if !answer.is_empty() {
					out.push_str(&format!("answer: {answer}\n"));
				}
				out.push_str("```\n");
			}
			StreamPart::Compaction { label, summary } => {
				out.push_str(&format!("── {label} ──\n{summary}\n"));
			}
			StreamPart::Error { body } => {
				out.push_str(&format!("✗ {body}\n"));
			}
			StreamPart::Retry { body } => {
				out.push_str(&format!("↻ {body}\n"));
			}
			StreamPart::ContextGroup { label } => {
				out.push_str(&format!("▸ {label}\n"));
			}
			StreamPart::Plan { title, body, .. } => {
				out.push_str(&format!("```plan title=\"{title}\"\n{body}\n```\n"));
			}
			StreamPart::Pty { id, title, lines, .. } => {
				out.push_str(&format!("```pty id=\"{id}\" title=\"{title}\"\n"));
				for l in lines {
					out.push_str(l);
					out.push('\n');
				}
				out.push_str("```\n");
			}
			StreamPart::Interrupted => {
				out.push_str("*(interrupted)*\n");
			}
		}
	}
	out
}

/// Plain preview of tool body without ANSI for headers.
pub fn plain_preview(s: &str, max: usize) -> String {
	let t = strip_ansi(s);
	t.chars().take(max).collect()
}
