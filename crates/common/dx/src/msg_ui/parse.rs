//! Parse assistant content into typed stream parts.

use std::time::Duration;

/// Expand state for a disclosure card.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExpandMode {
	/// Header only.
	Collapsed,
	/// Short preview window (default for most tools).
	#[default]
	Preview,
	/// Full body (capped).
	Full,
}

/// Lifecycle of a tool / command card.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PartStatus {
	Running,
	#[default]
	Done,
	Error,
}

/// First-class transcript part (paint model).
#[derive(Debug, Clone)]
pub enum StreamPart {
	Text {
		body: String,
	},
	Thinking {
		/// Stable index among thinking blocks in this message.
		index: usize,
		body: String,
		duration: Option<Duration>,
		streaming: bool,
	},
	Tool {
		/// Stable id when present (`id="…"`), else synthetic `tool-{index}`.
		id: String,
		index: usize,
		name: String,
		title: String,
		status: PartStatus,
		preview: String,
		body: String,
		duration: Option<Duration>,
	},
	Subagent {
		index: usize,
		name: String,
		body: String,
		status: PartStatus,
	},
	Approval {
		/// Optional call id for resolving clicks.
		call_id: String,
		tool: String,
		body: String,
		/// once | always | deny | pending
		decision: String,
	},
	/// Ask-user question form in the transcript.
	Question {
		id: String,
		prompt: String,
		options: Vec<String>,
		/// Answer once resolved (empty while pending).
		answer: String,
	},
	Compaction {
		label: String,
		summary: String,
	},
	Error {
		body: String,
	},
	Retry {
		body: String,
	},
	ContextGroup {
		label: String,
	},
	/// First-class plan document in the stream.
	Plan {
		title: String,
		body: String,
		/// Checklist lines (optional, derived from `- [ ]` / numbered steps).
		steps: Vec<PlanStep>,
	},
	/// Interactive shell session card (PTY host).
	Pty {
		id: String,
		title: String,
		/// Snapshot lines (refreshed from PtyHost each paint).
		lines: Vec<String>,
		attached: bool,
		alive: bool,
	},
	Interrupted,
}

#[derive(Debug, Clone)]
pub struct PlanStep {
	pub text: String,
	pub done: bool,
}

// ── Limits (generous for production; still bounded for UI) ───────────────────

pub const TOOL_PREVIEW_LINES: usize = 8;
pub const TOOL_FULL_LINES: usize = 400;
pub const SUBAGENT_PREVIEW_LINES: usize = 12;
pub const SUBAGENT_FULL_LINES: usize = 120;
pub const LINE_CLIP_COLS: usize = 500;
pub const THINK_AUTO_COLLAPSE_LINES: usize = 8;

// ── Public parse ─────────────────────────────────────────────────────────────

/// Parse assistant `content` into ordered stream parts.
pub fn parse_stream_parts(content: &str, thinking_duration: Option<Duration>) -> Vec<StreamPart> {
	let mut parts: Vec<StreamPart> = Vec::new();
	let mut text_buf = String::new();
	let mut mode = Mode::Text;
	let mut cmd = CmdBuild::default();
	let mut think_body = String::new();
	let mut sub = SubBuild::default();
	let mut approval_body = String::new();
	let mut approval_id = String::new();
	let mut approval_tool = String::new();
	let mut question_body = String::new();
	let mut question_id = String::new();
	let mut tool_raw = String::new();
	let mut think_idx = 0usize;
	let mut tool_idx = 0usize;
	let mut sub_idx = 0usize;
	let mut first_think_duration_applied = false;

	fn flush_text(parts: &mut Vec<StreamPart>, buf: &mut String) {
		let trimmed = buf.trim_matches(|c| c == '\n' || c == '\r');
		if !trimmed.is_empty() {
			parts.push(StreamPart::Text { body: trimmed.to_string() });
		}
		buf.clear();
	}

	for line in content.lines() {
		let trimmed = line.trim();
		match mode {
			Mode::Text => {
				// Thinking open
				let think_open = trimmed == "<think>"
					|| trimmed.starts_with("<think>")
					|| trimmed == "<thinking>"
					|| trimmed.starts_with("<thinking>");
				if think_open {
					flush_text(&mut parts, &mut text_buf);
					think_body.clear();
					let rest = trimmed
						.strip_prefix("<thinking>")
						.or_else(|| trimmed.strip_prefix("<think>"))
						.unwrap_or("")
						.trim();
					if let Some(inner) =
						rest.strip_suffix("</thinking>").or_else(|| rest.strip_suffix("</think>"))
					{
						let inner = inner.trim();
						if !inner.is_empty() {
							think_body.push_str(inner);
						}
						let dur = if !first_think_duration_applied {
							first_think_duration_applied = true;
							thinking_duration
						} else {
							None
						};
						parts.push(StreamPart::Thinking {
							index: think_idx,
							body: std::mem::take(&mut think_body),
							duration: dur,
							streaming: false,
						});
						think_idx += 1;
						mode = Mode::Text;
					} else {
						if !rest.is_empty() {
							think_body.push_str(rest);
							think_body.push('\n');
						}
						mode = Mode::Thinking;
					}
				} else if let Some(rest) = trimmed.strip_prefix("```command") {
					flush_text(&mut parts, &mut text_buf);
					cmd = CmdBuild::from_header(rest);
					mode = Mode::Command;
				} else if matches!(trimmed, "```bash" | "```sh" | "```shell" | "```json")
					|| trimmed.starts_with("```bash ")
					|| trimmed.starts_with("```sh ")
					|| trimmed.starts_with("```shell ")
					|| trimmed.starts_with("```json ")
				{
					flush_text(&mut parts, &mut text_buf);
					let lang = trimmed.trim_start_matches('`').trim();
					let (name, title) =
						if lang.starts_with("bash") || lang.starts_with("sh") || lang.starts_with("shell") {
							("shell", "Terminal")
						} else {
							(lang, "Tool")
						};
					cmd = CmdBuild {
						id: format!("tool-{tool_idx}"),
						name: name.into(),
						title: title.into(),
						status: PartStatus::Done,
						preview: String::new(),
						body: String::new(),
						duration: None,
					};
					mode = Mode::Command;
				} else if trimmed.starts_with("▸ Context ·") || trimmed.starts_with("▸ Context") {
					flush_text(&mut parts, &mut text_buf);
					parts.push(StreamPart::ContextGroup {
						label: trimmed.trim_start_matches('▸').trim().to_string(),
					});
				} else if trimmed == "```plan" || trimmed.starts_with("```plan") {
					flush_text(&mut parts, &mut text_buf);
					// Collect until closing fence in Plan mode
					mode = Mode::Plan;
					// stash title in cmd_name temporarily via sub fields
					sub.name = extract_attr(trimmed.strip_prefix("```plan").unwrap_or(""), "title")
						.unwrap_or_else(|| "Plan".into());
					sub.body.clear();
				} else if trimmed.starts_with("## Plan") || trimmed == "# Plan" {
					flush_text(&mut parts, &mut text_buf);
					// Treat following text until blank double or next special as plan — simple: this line + body as plan
					let title = trimmed.trim_start_matches('#').trim().to_string();
					parts.push(StreamPart::Plan { title, body: String::new(), steps: Vec::new() });
				} else if trimmed.contains("Context compacted")
					&& (trimmed.starts_with('─')
						|| trimmed.starts_with("──")
						|| trimmed.contains("compacted"))
				{
					flush_text(&mut parts, &mut text_buf);
					parts.push(StreamPart::Compaction {
						label: "Context compacted".into(),
						summary: String::new(),
					});
				} else if trimmed.starts_with('✗') && trimmed.len() > 2 {
					flush_text(&mut parts, &mut text_buf);
					parts
						.push(StreamPart::Error { body: trimmed.trim_start_matches('✗').trim().to_string() });
				} else if trimmed.starts_with('↻') {
					flush_text(&mut parts, &mut text_buf);
					parts
						.push(StreamPart::Retry { body: trimmed.trim_start_matches('↻').trim().to_string() });
				} else if trimmed.contains("interrupted")
					&& (trimmed.contains("INTERRUPTED")
						|| trimmed.starts_with("*(")
						|| trimmed.contains("__INTERRUPT"))
				{
					flush_text(&mut parts, &mut text_buf);
					parts.push(StreamPart::Interrupted);
				} else if trimmed == "```approval" || trimmed.starts_with("```approval") {
					flush_text(&mut parts, &mut text_buf);
					approval_body.clear();
					let rest = trimmed.strip_prefix("```approval").unwrap_or("");
					approval_id = extract_attr(rest, "id").unwrap_or_default();
					approval_tool = extract_attr(rest, "tool").unwrap_or_default();
					mode = Mode::Approval;
				} else if trimmed == "```question" || trimmed.starts_with("```question") {
					flush_text(&mut parts, &mut text_buf);
					question_body.clear();
					let rest = trimmed.strip_prefix("```question").unwrap_or("");
					question_id = extract_attr(rest, "id").unwrap_or_else(|| format!("q-{tool_idx}"));
					mode = Mode::Question;
				} else if trimmed.starts_with("<tool_call") || trimmed == "<tool_call>" {
					flush_text(&mut parts, &mut text_buf);
					tool_raw.clear();
					if trimmed.contains("</tool_call>") {
						let inner =
							trimmed.trim_start_matches("<tool_call>").trim_end_matches("</tool_call>").trim();
						if !inner.is_empty() {
							tool_raw.push_str(inner);
						}
						parts.push(tool_from_json(&tool_raw, tool_idx, PartStatus::Done, false));
						tool_idx += 1;
						tool_raw.clear();
						mode = Mode::Text;
					} else {
						mode = Mode::ToolCall;
					}
				} else if trimmed.starts_with("<tool_result") || trimmed == "<tool_result>" {
					flush_text(&mut parts, &mut text_buf);
					cmd = CmdBuild {
						id: format!("tool-{tool_idx}"),
						name: "result".into(),
						title: "Result".into(),
						status: PartStatus::Done,
						preview: String::new(),
						body: String::new(),
						duration: None,
					};
					if trimmed.contains("</tool_result>") {
						let inner =
							trimmed.trim_start_matches("<tool_result>").trim_end_matches("</tool_result>").trim();
						if !inner.is_empty() {
							cmd.body.push_str(inner);
						}
						let finished = std::mem::take(&mut cmd);
						parts.push(finished.into_part(tool_idx));
						tool_idx += 1;
						mode = Mode::Text;
					} else {
						mode = Mode::ToolResult;
					}
				} else if let Some(rest) = trimmed.strip_prefix("<subagent") {
					flush_text(&mut parts, &mut text_buf);
					let name = extract_attr(rest, "name").unwrap_or_else(|| "subagent".into());
					sub = SubBuild { name, body: String::new(), status: PartStatus::Running };
					mode = Mode::Subagent;
				} else {
					if !text_buf.is_empty() {
						text_buf.push('\n');
					}
					text_buf.push_str(line);
				}
			}
			Mode::Thinking => {
				if trimmed == "</think>"
					|| trimmed == "</thinking>"
					|| trimmed.ends_with("</think>")
					|| trimmed.ends_with("</thinking>")
				{
					let before = trimmed
						.strip_suffix("</thinking>")
						.or_else(|| trimmed.strip_suffix("</think>"))
						.unwrap_or("")
						.trim();
					if !before.is_empty() && before != trimmed {
						if !think_body.is_empty() {
							think_body.push('\n');
						}
						think_body.push_str(before);
					}
					let dur = if !first_think_duration_applied {
						first_think_duration_applied = true;
						thinking_duration
					} else {
						None
					};
					parts.push(StreamPart::Thinking {
						index: think_idx,
						body: std::mem::take(&mut think_body).trim().to_string(),
						duration: dur,
						streaming: false,
					});
					think_idx += 1;
					mode = Mode::Text;
				} else {
					if !think_body.is_empty() {
						think_body.push('\n');
					}
					think_body.push_str(line);
				}
			}
			Mode::Command => {
				if trimmed == "```" {
					let finished = std::mem::take(&mut cmd);
					parts.push(finished.into_part(tool_idx));
					tool_idx += 1;
					mode = Mode::Text;
				} else {
					if !cmd.body.is_empty() {
						cmd.body.push('\n');
					}
					cmd.body.push_str(line);
				}
			}
			Mode::ToolCall => {
				if trimmed == "</tool_call>" || trimmed.ends_with("</tool_call>") {
					parts.push(tool_from_json(&tool_raw, tool_idx, PartStatus::Running, true));
					tool_idx += 1;
					tool_raw.clear();
					mode = Mode::Text;
				} else {
					if !tool_raw.is_empty() {
						tool_raw.push('\n');
					}
					tool_raw.push_str(line);
				}
			}
			Mode::ToolResult => {
				if trimmed == "</tool_result>" || trimmed.ends_with("</tool_result>") {
					let finished = std::mem::take(&mut cmd);
					parts.push(finished.into_part(tool_idx));
					tool_idx += 1;
					mode = Mode::Text;
				} else {
					if !cmd.body.is_empty() {
						cmd.body.push('\n');
					}
					cmd.body.push_str(line);
				}
			}
			Mode::Approval => {
				if trimmed == "```" {
					let body = std::mem::take(&mut approval_body).trim().to_string();
					let tool = if approval_tool.is_empty() {
						body
							.lines()
							.next()
							.unwrap_or("tool")
							.split(['·', ' '])
							.next()
							.unwrap_or("tool")
							.trim()
							.to_string()
					} else {
						std::mem::take(&mut approval_tool)
					};
					parts.push(StreamPart::Approval {
						call_id: std::mem::take(&mut approval_id),
						tool,
						body,
						decision: "pending".into(),
					});
					mode = Mode::Text;
				} else {
					if !approval_body.is_empty() {
						approval_body.push('\n');
					}
					approval_body.push_str(line);
				}
			}
			Mode::Question => {
				if trimmed == "```" {
					let body = std::mem::take(&mut question_body);
					let mut lines = body.lines();
					let prompt = lines.next().unwrap_or("Choose:").trim().to_string();
					let mut options = Vec::new();
					for l in lines {
						let t = l.trim();
						if t.is_empty() {
							continue;
						}
						// "  1. Option" or plain line
						if let Some(rest) = t.split_once(". ")
							&& rest.0.trim().chars().all(|c| c.is_ascii_digit())
						{
							options.push(rest.1.to_string());
							continue;
						}
						options.push(t.to_string());
					}
					if options.is_empty() {
						options = vec!["Yes".into(), "No".into()];
					}
					parts.push(StreamPart::Question {
						id: std::mem::take(&mut question_id),
						prompt,
						options,
						answer: String::new(),
					});
					mode = Mode::Text;
				} else {
					if !question_body.is_empty() {
						question_body.push('\n');
					}
					question_body.push_str(line);
				}
			}
			Mode::Subagent => {
				if trimmed == "</subagent>" {
					parts.push(StreamPart::Subagent {
						index: sub_idx,
						name: std::mem::take(&mut sub.name),
						body: std::mem::take(&mut sub.body).trim().to_string(),
						status: PartStatus::Done,
					});
					sub_idx += 1;
					mode = Mode::Text;
				} else {
					if !sub.body.is_empty() {
						sub.body.push('\n');
					}
					sub.body.push_str(line);
				}
			}
			Mode::Plan => {
				if trimmed == "```" {
					let body = std::mem::take(&mut sub.body);
					let title =
						if sub.name.is_empty() { "Plan".into() } else { std::mem::take(&mut sub.name) };
					let steps = parse_plan_steps(&body);
					parts.push(StreamPart::Plan { title, body: body.trim().to_string(), steps });
					mode = Mode::Text;
				} else {
					if !sub.body.is_empty() {
						sub.body.push('\n');
					}
					sub.body.push_str(line);
				}
			}
		}
	}

	// Unclosed modes (streaming)
	match mode {
		Mode::Thinking => {
			let dur = if !first_think_duration_applied { thinking_duration } else { None };
			parts.push(StreamPart::Thinking {
				index: think_idx,
				body: think_body.trim().to_string(),
				duration: dur,
				streaming: true,
			});
		}
		Mode::Command => {
			if cmd.status == PartStatus::Running || !cmd.body.is_empty() || !cmd.name.is_empty() {
				let finished = std::mem::take(&mut cmd);
				parts.push(finished.into_part(tool_idx));
			}
		}
		Mode::ToolCall => {
			if !tool_raw.is_empty() {
				parts.push(tool_from_json(&tool_raw, tool_idx, PartStatus::Running, true));
			}
		}
		Mode::ToolResult => {
			let finished = std::mem::take(&mut cmd);
			parts.push(finished.into_part(tool_idx));
		}
		Mode::Approval => {
			if !approval_body.trim().is_empty() {
				parts.push(StreamPart::Approval {
					call_id: approval_id,
					tool: if approval_tool.is_empty() { "tool".into() } else { approval_tool },
					body: approval_body.trim().to_string(),
					decision: "pending".into(),
				});
			}
		}
		Mode::Question => {
			if !question_body.trim().is_empty() {
				let mut lines = question_body.lines();
				let prompt = lines.next().unwrap_or("Choose:").trim().to_string();
				let options: Vec<String> =
					lines.map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect();
				parts.push(StreamPart::Question {
					id: question_id,
					prompt,
					options,
					answer: String::new(),
				});
			}
		}
		Mode::Subagent => {
			parts.push(StreamPart::Subagent {
				index: sub_idx,
				name: sub.name,
				body: sub.body.trim().to_string(),
				status: PartStatus::Running,
			});
		}
		Mode::Plan => {
			let body = sub.body;
			let steps = parse_plan_steps(&body);
			parts.push(StreamPart::Plan {
				title: if sub.name.is_empty() { "Plan".into() } else { sub.name },
				body: body.trim().to_string(),
				steps,
			});
		}
		Mode::Text => flush_text(&mut parts, &mut text_buf),
	}

	// Merge adjacent text
	coalesce_text(&mut parts);
	parts
}

fn coalesce_text(parts: &mut Vec<StreamPart>) {
	let mut i = 0;
	while i + 1 < parts.len() {
		let merge =
			matches!((&parts[i], &parts[i + 1]), (StreamPart::Text { .. }, StreamPart::Text { .. }));
		if merge {
			let next = parts.remove(i + 1);
			if let (StreamPart::Text { body: a }, StreamPart::Text { body: b }) = (&mut parts[i], next) {
				a.push_str("\n\n");
				a.push_str(&b);
			}
		} else {
			i += 1;
		}
	}
}

// ── Builders ─────────────────────────────────────────────────────────────────

#[derive(Default)]
enum Mode {
	#[default]
	Text,
	Thinking,
	Command,
	ToolCall,
	ToolResult,
	Subagent,
	Approval,
	Question,
	Plan,
}

#[derive(Default)]
struct CmdBuild {
	id: String,
	name: String,
	title: String,
	status: PartStatus,
	preview: String,
	body: String,
	duration: Option<Duration>,
}

impl CmdBuild {
	fn from_header(rest: &str) -> Self {
		let id = extract_attr(rest, "id").unwrap_or_default();
		let name = extract_attr(rest, "name").unwrap_or_else(|| "tool".into());
		let title = extract_attr(rest, "title").unwrap_or_else(|| {
			crate::tools::ToolKind::from_name(&name)
				.map(|k| k.display_title().to_string())
				.unwrap_or_else(|| "Tool".into())
		});
		let status_s = extract_attr(rest, "status").unwrap_or_default();
		let status = match status_s.to_ascii_lowercase().as_str() {
			"running" => PartStatus::Running,
			"error" => PartStatus::Error,
			_ => PartStatus::Done,
		};
		let duration = extract_attr(rest, "duration_ms")
			.and_then(|s| s.parse::<u64>().ok())
			.map(Duration::from_millis);
		// Human preview: strip known attrs from remainder
		let mut preview = rest.to_string();
		for key in ["id", "name", "title", "status", "duration_ms"] {
			if let Some(v) = extract_attr(rest, key) {
				preview = preview.replace(&format!("{key}=\"{v}\""), "");
			}
		}
		let preview = preview.split_whitespace().collect::<Vec<_>>().join(" ");
		Self { id, name, title, status, preview, body: String::new(), duration }
	}

	fn into_part(self, tool_idx: usize) -> StreamPart {
		let id = if self.id.is_empty() { format!("tool-{tool_idx}") } else { self.id };
		// Infer title from tool kind when generic
		let title = if self.title.is_empty() || self.title == "Tool" {
			crate::tools::ToolKind::from_name(&self.name)
				.map(|k| k.display_title().to_string())
				.unwrap_or(self.title)
		} else {
			self.title
		};
		StreamPart::Tool {
			id,
			index: tool_idx,
			name: self.name,
			title,
			status: self.status,
			preview: self.preview,
			body: self.body,
			duration: self.duration,
		}
	}
}

#[derive(Default)]
struct SubBuild {
	name: String,
	body: String,
	#[allow(dead_code)]
	status: PartStatus,
}

fn tool_from_json(raw: &str, index: usize, status: PartStatus, _streaming: bool) -> StreamPart {
	let s = raw.trim();
	let (name, preview, id) = if let Ok(v) = serde_json::from_str::<serde_json::Value>(s) {
		let name = v
			.get("name")
			.or_else(|| v.get("tool"))
			.and_then(|x| x.as_str())
			.unwrap_or("tool")
			.to_string();
		let id = v
			.get("id")
			.or_else(|| v.get("call_id"))
			.and_then(|x| x.as_str())
			.map(|s| s.to_string())
			.unwrap_or_default();
		let preview = v
			.get("arguments")
			.or_else(|| v.get("args"))
			.and_then(|a| {
				a.get("command")
					.or_else(|| a.get("cmd"))
					.or_else(|| a.get("query"))
					.or_else(|| a.get("path"))
					.or_else(|| a.get("url"))
					.or_else(|| a.get("pattern"))
					.and_then(|x| x.as_str())
					.map(|s| s.chars().take(120).collect::<String>())
			})
			.or_else(|| {
				v.get("arguments").and_then(|a| a.as_str()).map(|s| s.chars().take(120).collect())
			})
			.unwrap_or_default();
		(name, preview, id)
	} else {
		("tool".into(), String::new(), String::new())
	};
	let title = crate::tools::ToolKind::from_name(&name)
		.map(|k| k.display_title().to_string())
		.unwrap_or_else(|| format!("Tool · {name}"));
	let id = if id.is_empty() { format!("tool-{index}") } else { id };
	StreamPart::Tool {
		id,
		index,
		name,
		title,
		status,
		preview,
		body: if status == PartStatus::Running { String::new() } else { s.to_string() },
		duration: None,
	}
}

fn parse_plan_steps(body: &str) -> Vec<PlanStep> {
	let mut steps = Vec::new();
	for line in body.lines() {
		let t = line.trim();
		if let Some(rest) = t.strip_prefix("- [x] ").or_else(|| t.strip_prefix("- [X] ")) {
			steps.push(PlanStep { text: rest.to_string(), done: true });
		} else if let Some(rest) = t.strip_prefix("- [ ] ") {
			steps.push(PlanStep { text: rest.to_string(), done: false });
		} else if let Some(rest) = t.strip_prefix("- ") {
			steps.push(PlanStep { text: rest.to_string(), done: false });
		} else if t.len() > 2
			&& t.as_bytes()[0].is_ascii_digit()
			&& (t.contains(". ") || t.contains(") "))
		{
			let text = t
				.split_once(". ")
				.or_else(|| t.split_once(") "))
				.map(|(_, r)| r.to_string())
				.unwrap_or_else(|| t.to_string());
			steps.push(PlanStep { text, done: false });
		}
	}
	steps
}

pub fn extract_attr(s: &str, key: &str) -> Option<String> {
	let pattern = format!("{key}=\"");
	let start = s.find(&pattern)? + pattern.len();
	let rest = &s[start..];
	let end = rest.find('"')?;
	Some(rest[..end].to_string())
}

/// Resolve expand mode for a tool part.
pub fn resolve_tool_expand(
	index: usize,
	status: PartStatus,
	name: &str,
	command_expand: &std::collections::HashMap<usize, bool>,
	commands_expanded_default: bool,
) -> ExpandMode {
	if let Some(&open) = command_expand.get(&index) {
		return if open { ExpandMode::Full } else { ExpandMode::Collapsed };
	}
	if status == PartStatus::Running || status == PartStatus::Error {
		return ExpandMode::Preview;
	}
	if commands_expanded_default {
		return ExpandMode::Full;
	}
	let default_open =
		crate::tools::ToolKind::from_name(name).map(|k| k.default_open()).unwrap_or(false);
	if default_open { ExpandMode::Preview } else { ExpandMode::Collapsed }
}

pub fn resolve_thinking_expand(
	index: usize,
	streaming: bool,
	body_lines: usize,
	thinking_expanded: bool,
	thinking_expand: &std::collections::HashMap<usize, bool>,
) -> bool {
	if let Some(&v) = thinking_expand.get(&index) {
		return v;
	}
	if streaming {
		return true;
	}
	if thinking_expanded {
		// Global open, but auto-collapse very long finished thoughts when flag is "default"
		return body_lines <= THINK_AUTO_COLLAPSE_LINES || thinking_expanded;
	}
	// Default: short open, long closed — thinking_expanded false means user collapsed all
	false
}

pub fn resolve_subagent_expand(
	index: usize,
	status: PartStatus,
	subagent_expand: &std::collections::HashMap<usize, bool>,
	subagents_expanded: bool,
) -> ExpandMode {
	if let Some(&open) = subagent_expand.get(&index) {
		return if open { ExpandMode::Full } else { ExpandMode::Collapsed };
	}
	if status == PartStatus::Running {
		return ExpandMode::Preview;
	}
	if subagents_expanded { ExpandMode::Full } else { ExpandMode::Collapsed }
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn expand_mode_default_is_preview() {
		assert_eq!(ExpandMode::default(), ExpandMode::Preview);
	}

	#[test]
	fn thinking_block_detection() {
		let text = "<think>this is a thinking block</think>";
		let parts = crate::msg_ui::parse_stream_parts(text, HashMap::new(), false, false, false);
		let has_think = parts.iter().any(|p| matches!(p, StreamPart::Think(_)));
		assert!(has_think, "should detect thinking block: {parts:?}");
	}

	#[test]
	fn tool_call_detection() {
		let text = r#"<tool_call>{"name":"bash","arguments":{"command":"ls"}}</tool_call>"#;
		let parts = crate::msg_ui::parse_stream_parts(text, HashMap::new(), false, false, false);
		let has_tool = parts.iter().any(|p| matches!(p, StreamPart::Command { .. }));
		assert!(has_tool, "should detect tool call: {parts:?}");
	}

	#[test]
	fn empty_text_returns_single_text_part() {
		let text = "";
		let parts = crate::msg_ui::parse_stream_parts(text, HashMap::new(), false, false, false);
		// Empty text should still return something (at minimum a text part)
		assert!(!parts.is_empty(), "expected at least one part for empty text");
	}

	#[test]
	fn text_without_tags_returns_single_text_part() {
		let text = "Hello, this is a simple message without any tags or formatting.";
		let parts = crate::msg_ui::parse_stream_parts(text, HashMap::new(), false, false, false);
		let text_parts: Vec<_> = parts
			.iter()
			.filter_map(|p| if let StreamPart::Text(t) = p { Some(t.text.as_str()) } else { None })
			.collect();
		assert!(!text_parts.is_empty(), "should have at least one text part: {parts:?}");
	}

	#[test]
	fn code_block_detection() {
		let text = "```rust\nfn main() {}\n```";
		let parts = crate::msg_ui::parse_stream_parts(text, HashMap::new(), false, false, false);
		let has_code = parts.iter().any(|p| matches!(p, StreamPart::CodeBlock(_)));
		assert!(has_code, "should detect code block: {parts:?}");
	}
}
