#![allow(dead_code)]

use ratatui::{
	buffer::Buffer,
	layout::Rect,
	style::{Color, Modifier, Style},
	text::{Line, Span, Text},
	widgets::{Block, Borders, Paragraph, Widget},
};
use tiktoken_rs::cl100k_base;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::{
	effects::{ShimmerEffect, TypingIndicator},
	theme::ChatTheme,
};

fn count_tokens(text: &str) -> usize {
	// Prefer cheap estimate. Full BPE encode is only for finalize/load paths that
	// call this once — never from the hot streaming loop (see append_content).
	match cl100k_base() {
		Ok(bpe) => bpe.encode_with_special_tokens(text).len(),
		Err(_) => text.chars().count().div_ceil(4),
	}
}

/// True when a stream delta may open/close structure and needs a full reparse.
fn delta_requires_parts_rebuild(delta: &str) -> bool {
	delta.contains('<')
		|| delta.contains('`')
		|| delta.contains("__STREAM")
		|| delta.contains("__UPDATE")
		|| delta.contains("__VOICE")
		|| delta.contains("TITLE:")
		|| delta.contains("Title:")
}

/// Count lines for structured assistant blocks with expand flags.
pub fn count_parsed_blocks(
	content: &str,
	thinking_expanded: bool,
	commands_expanded_default: bool,
	subagents_expanded_default: bool,
	width: usize,
) -> usize {
	let blocks = parse_message_blocks(content);
	if blocks.is_empty() {
		return 1;
	}
	blocks
		.iter()
		.map(|b| {
			block_line_count(
				b,
				thinking_expanded,
				commands_expanded_default,
				subagents_expanded_default,
				width,
			)
		})
		.sum::<usize>()
		.max(1)
}

fn block_line_count(
	block: &MessageBlock<'_>,
	thinking_expanded: bool,
	commands_expanded: bool,
	subagents_expanded: bool,
	w: usize,
) -> usize {
	match block {
		MessageBlock::Text(lines) => lines
			.iter()
			.map(|l| crate::components::hard_wrap_line_count(l.chars().count().max(1), w))
			.sum::<usize>()
			.max(1),
		MessageBlock::Thinking { lines, .. } => {
			if thinking_expanded {
				let body: usize =
					lines.iter().map(|l| hard_wrap_line_count(l.chars().count().max(1), w)).sum();
				1 + body
			} else {
				1
			}
		}
		MessageBlock::Command { preview, lines, expanded_hint, name, .. } => {
			let status = command_status_from_preview(preview, lines.first().copied());
			let is_running = status == "running";
			let default_open = expanded_hint.unwrap_or_else(|| {
				crate::tools::ToolKind::from_name(name).map(|k| k.default_open()).unwrap_or(false)
			}) || is_running
				|| status == "error"
				|| commands_expanded;
			let total = lines.len();
			let is_shell = is_terminal_tool(name)
				|| matches!(crate::tools::ToolKind::from_name(name), Some(crate::tools::ToolKind::Shell));
			if is_running {
				let show = total.min(TOOL_MAX_LINES);
				let body_h: usize = lines
					.iter()
					.rev()
					.take(show)
					.map(|l| hard_wrap_line_count(l.chars().count().max(1), w))
					.sum();
				1 + body_h + 1
			} else if default_open {
				// Preview window (latest N lines for shell)
				let show = total.min(TOOL_PREVIEW_LINES);
				let body_h: usize = if is_shell {
					lines
						.iter()
						.rev()
						.take(show)
						.map(|l| hard_wrap_line_count(l.chars().count().max(1), w))
						.sum()
				} else {
					lines.iter().take(show).map(|l| hard_wrap_line_count(l.chars().count().max(1), w)).sum()
				};
				1 + body_h + 1 // header + body + "Click to expand"
			} else if total > 0 {
				2 // header + expand footer
			} else {
				1
			}
		}
		MessageBlock::Subagent { lines, .. } => {
			if subagents_expanded {
				let show = lines.len().min(SUBAGENT_MAX_LINES);
				let body_h: usize =
					lines.iter().take(show).map(|l| hard_wrap_line_count(l.chars().count().max(1), w)).sum();
				1 + body_h + if lines.len() > SUBAGENT_MAX_LINES { 1 } else { 0 }
			} else {
				1
			}
		}
		MessageBlock::Approval { lines } => {
			1 + lines.iter().map(|l| hard_wrap_line_count(l.chars().count().max(1), w)).sum::<usize>()
		}
		MessageBlock::Compaction { .. } => 1,
		MessageBlock::ErrorCard { lines } => {
			1 + lines
				.iter()
				.take(8)
				.map(|l| hard_wrap_line_count(l.chars().count().max(1), w))
				.sum::<usize>()
		}
		MessageBlock::RetryHint { lines } => {
			1 + lines
				.iter()
				.take(3)
				.map(|l| hard_wrap_line_count(l.chars().count().max(1), w))
				.sum::<usize>()
		}
		MessageBlock::ContextGroup { .. } => 1,
	}
}

#[allow(clippy::too_many_arguments)]
pub fn hit_test_block(
	content: &str,
	relative_y: usize,
	width: usize,
	thinking_expanded: bool,
	commands_expanded_default: bool,
	subagents_expanded_default: bool,
	command_expand: &std::collections::HashMap<usize, bool>,
	subagent_expand: &std::collections::HashMap<usize, bool>,
) -> Option<InteractiveBlock> {
	let empty = std::collections::HashMap::new();
	hit_test_block_ex(
		content,
		relative_y,
		width,
		thinking_expanded,
		commands_expanded_default,
		subagents_expanded_default,
		command_expand,
		subagent_expand,
		&empty,
	)
}

#[allow(clippy::too_many_arguments)]
pub fn hit_test_block_ex(
	content: &str,
	relative_y: usize,
	width: usize,
	thinking_expanded: bool,
	commands_expanded_default: bool,
	subagents_expanded_default: bool,
	command_expand: &std::collections::HashMap<usize, bool>,
	subagent_expand: &std::collections::HashMap<usize, bool>,
	thinking_expand: &std::collections::HashMap<usize, bool>,
) -> Option<InteractiveBlock> {
	let theme = crate::theme::ChatTheme::dark_fallback();
	let ctx = crate::msg_ui::RenderCtx {
		theme: &theme,
		thinking_expanded,
		commands_expanded: commands_expanded_default,
		subagents_expanded: subagents_expanded_default,
		thinking_duration: None,
		content_width: Some(width),
		command_expand,
		subagent_expand,
		thinking_expand,
		streaming: false,
	};
	let tagged = crate::msg_ui::render_parts_tagged(content, &ctx);
	let mut current_y = 0usize;
	for (line, tag) in tagged {
		let wrapped = clip_lines_to_width(vec![line], width);
		let h = wrapped.len().max(1);
		if relative_y >= current_y && relative_y < current_y + h {
			return tag;
		}
		current_y += h;
	}
	None
}

/// Structured segments inside an assistant message.
#[derive(Debug, Clone)]
pub enum MessageBlock<'a> {
	Text(Vec<&'a str>),
	Thinking {
		lines: Vec<&'a str>,
	},
	Command {
		name: String,
		preview: String,
		lines: Vec<&'a str>,
		/// When Some, overrides default expand for this block index via Message flags.
		expanded_hint: Option<bool>,
		/// Optional display title (Read / Terminal / …).
		title: Option<String>,
	},
	Subagent {
		name: String,
		lines: Vec<&'a str>,
	},
	Approval {
		lines: Vec<&'a str>,
	},
	/// Visual “context compacted” divider.
	Compaction {
		label: String,
	},
	/// Structured error card.
	ErrorCard {
		lines: Vec<&'a str>,
	},
	/// Retry affordance line.
	RetryHint {
		lines: Vec<&'a str>,
	},
	/// Collapsed context group summary (“Read 3 · Grep 2”).
	ContextGroup {
		label: String,
	},
}

/// Parse assistant content into thinking / command / subagent / text blocks.
/// Supports:
/// - `<think>…</think>`
/// - ` ```command name="…"` fences
/// - `<tool_call>…</tool_call>` (JSON tool invoke — collapsed UI)
/// - `<tool_result>…</tool_result>`
/// - `<subagent>…</subagent>`
pub fn parse_message_blocks(content: &str) -> Vec<MessageBlock<'_>> {
	let mut blocks: Vec<MessageBlock<'_>> = Vec::new();
	let mut text_buf: Vec<&str> = Vec::new();
	let mut mode = ParseMode::Text;
	let mut cmd_name = String::new();
	let mut cmd_preview = String::new();
	let mut cmd_lines: Vec<&str> = Vec::new();
	let mut think_lines: Vec<&str> = Vec::new();
	let mut sub_name = String::new();
	let mut sub_lines: Vec<&str> = Vec::new();
	let mut approval_lines: Vec<&str> = Vec::new();
	let mut tool_raw: Vec<&str> = Vec::new();

	fn flush_text<'a>(blocks: &mut Vec<MessageBlock<'a>>, buf: &mut Vec<&'a str>) {
		if !buf.is_empty() {
			while buf.first().is_some_and(|l| l.trim().is_empty()) {
				buf.remove(0);
			}
			if !buf.is_empty() {
				blocks.push(MessageBlock::Text(std::mem::take(buf)));
			}
		}
	}

	fn push_tool_block<'a>(blocks: &mut Vec<MessageBlock<'a>>, raw: Vec<&'a str>, streaming: bool) {
		let joined = raw.join("\n");
		let (name, preview) = parse_tool_call_json(&joined);
		// Invocation cards stay collapsed (JSON body is noisy); results use default_open.
		// Streaming in-progress calls stay open so the user sees progress.
		blocks.push(MessageBlock::Command {
			name,
			preview,
			lines: raw,
			expanded_hint: Some(streaming),
			title: None,
		});
	}

	enum ParseMode {
		Text,
		Thinking,
		Command,
		ToolCall,
		ToolResult,
		Subagent,
		Approval,
	}

	for line in content.lines() {
		let trimmed = line.trim();

		match mode {
			ParseMode::Text => {
				// Thinking blocks: <think>…</think> or <thinking>…</thinking>
				// (also single-line forms)
				let think_open = trimmed == "<think>"
					|| trimmed.starts_with("<think>")
					|| trimmed == "<thinking>"
					|| trimmed.starts_with("<thinking>");
				if think_open {
					flush_text(&mut blocks, &mut text_buf);
					think_lines.clear();
					// Prefer longer tag first
					let rest = trimmed
						.strip_prefix("<thinking>")
						.or_else(|| trimmed.strip_prefix("<think>"))
						.unwrap_or("");
					let rest = rest.trim();
					// Single-line: <think>body</think>
					if let Some(inner) =
						rest.strip_suffix("</thinking>").or_else(|| rest.strip_suffix("</think>"))
					{
						let inner = inner.trim();
						if !inner.is_empty() {
							think_lines.push(inner);
						}
						blocks.push(MessageBlock::Thinking { lines: std::mem::take(&mut think_lines) });
						mode = ParseMode::Text;
					} else {
						if !rest.is_empty() {
							think_lines.push(rest);
						}
						mode = ParseMode::Thinking;
					}
				} else if let Some(rest) = trimmed.strip_prefix("```command") {
					flush_text(&mut blocks, &mut text_buf);
					cmd_name = extract_attr(rest, "name").unwrap_or_else(|| "command".into());
					let status = extract_attr(rest, "status").unwrap_or_default();
					let title_attr = extract_attr(rest, "title");
					// Keep human preview (command text) after stripping attrs; prefix status token for UI.
					let mut rest_clean = rest.to_string();
					for key in ["name", "status", "title"] {
						if let Some(v) = extract_attr(rest, key) {
							rest_clean = rest_clean.replace(&format!("{key}=\"{v}\""), "");
						}
					}
					let human = rest_clean.trim().to_string();
					cmd_preview = if status.is_empty() {
						human
					} else if human.is_empty() {
						format!("status={status}")
					} else {
						format!("status={status} {human}")
					};
					// Stash title in preview side-channel via magic prefix when present
					if let Some(t) = title_attr {
						cmd_preview = format!("title={t} {cmd_preview}");
					}
					cmd_lines.clear();
					mode = ParseMode::Command;
				} else if matches!(trimmed, "```bash" | "```sh" | "```shell" | "```json")
					|| trimmed.starts_with("```bash ")
					|| trimmed.starts_with("```sh ")
					|| trimmed.starts_with("```shell ")
					|| trimmed.starts_with("```json ")
				{
					flush_text(&mut blocks, &mut text_buf);
					let lang = trimmed.trim_start_matches('`').trim();
					let (name, title) = match lang {
						l if l == "bash" || l == "sh" || l == "shell" => ("shell", "Terminal"),
						_ => (lang, "Tool"),
					};
					cmd_name = name.to_string();
					cmd_preview = format!("status=done title={title} {lang}");
					cmd_lines.clear();
					mode = ParseMode::Command;
				} else if trimmed.starts_with("▸ Context ·") || trimmed.starts_with("▸ Context") {
					flush_text(&mut blocks, &mut text_buf);
					blocks.push(MessageBlock::ContextGroup {
						label: trimmed.trim_start_matches('▸').trim().to_string(),
					});
				} else if trimmed.contains("Context compacted")
					&& (trimmed.starts_with('─') || trimmed.starts_with("──"))
				{
					flush_text(&mut blocks, &mut text_buf);
					blocks.push(MessageBlock::Compaction { label: "Context compacted".into() });
				} else if trimmed.starts_with('✗') && trimmed.len() > 2 {
					flush_text(&mut blocks, &mut text_buf);
					blocks.push(MessageBlock::ErrorCard {
						lines: vec![trimmed.trim_start_matches('✗').trim()],
					});
				} else if trimmed.starts_with('↻') {
					flush_text(&mut blocks, &mut text_buf);
					blocks.push(MessageBlock::RetryHint {
						lines: vec![trimmed.trim_start_matches('↻').trim()],
					});
				} else if trimmed == "```approval" {
					flush_text(&mut blocks, &mut text_buf);
					approval_lines.clear();
					mode = ParseMode::Approval;
				} else if trimmed.starts_with("<tool_call") || trimmed == "<tool_call>" {
					flush_text(&mut blocks, &mut text_buf);
					tool_raw.clear();
					// Inline single-line tool_call
					if trimmed.contains("</tool_call>") {
						let inner =
							trimmed.trim_start_matches("<tool_call>").trim_end_matches("</tool_call>").trim();
						if !inner.is_empty() {
							tool_raw.push(inner);
						}
						push_tool_block(&mut blocks, std::mem::take(&mut tool_raw), false);
						mode = ParseMode::Text;
					} else {
						mode = ParseMode::ToolCall;
					}
				} else if trimmed.starts_with("<tool_result") || trimmed == "<tool_result>" {
					flush_text(&mut blocks, &mut text_buf);
					cmd_name = "result".into();
					cmd_preview = String::new();
					cmd_lines.clear();
					if trimmed.contains("</tool_result>") {
						let inner =
							trimmed.trim_start_matches("<tool_result>").trim_end_matches("</tool_result>").trim();
						if !inner.is_empty() {
							cmd_lines.push(inner);
						}
						blocks.push(MessageBlock::Command {
							name: std::mem::take(&mut cmd_name),
							preview: std::mem::take(&mut cmd_preview),
							lines: std::mem::take(&mut cmd_lines),
							expanded_hint: None,
							title: None,
						});
						mode = ParseMode::Text;
					} else {
						mode = ParseMode::ToolResult;
					}
				} else if let Some(rest) = trimmed.strip_prefix("<subagent") {
					flush_text(&mut blocks, &mut text_buf);
					sub_name = extract_attr(rest, "name").unwrap_or_else(|| "subagent".into());
					sub_lines.clear();
					mode = ParseMode::Subagent;
				} else {
					text_buf.push(line);
				}
			}
			ParseMode::Thinking => {
				if trimmed == "</think>"
					|| trimmed == "</thinking>"
					|| trimmed.ends_with("</think>")
					|| trimmed.ends_with("</thinking>")
				{
					// Capture any trailing text before the close tag on the same line
					let before = trimmed
						.strip_suffix("</thinking>")
						.or_else(|| trimmed.strip_suffix("</think>"))
						.unwrap_or("")
						.trim();
					if !before.is_empty() && before != trimmed {
						think_lines.push(before);
					}
					blocks.push(MessageBlock::Thinking { lines: std::mem::take(&mut think_lines) });
					mode = ParseMode::Text;
				} else {
					think_lines.push(line);
				}
			}
			ParseMode::Command => {
				if trimmed == "```" {
					let name = std::mem::take(&mut cmd_name);
					let preview = std::mem::take(&mut cmd_preview);
					let title = title_from_preview(&preview);
					let hint = default_open_hint(&name, &preview);
					blocks.push(MessageBlock::Command {
						name,
						preview,
						lines: std::mem::take(&mut cmd_lines),
						expanded_hint: hint,
						title,
					});
					mode = ParseMode::Text;
				} else {
					cmd_lines.push(line);
				}
			}
			ParseMode::ToolCall => {
				if trimmed == "</tool_call>" || trimmed.ends_with("</tool_call>") {
					push_tool_block(&mut blocks, std::mem::take(&mut tool_raw), false);
					mode = ParseMode::Text;
				} else {
					tool_raw.push(line);
				}
			}
			ParseMode::ToolResult => {
				if trimmed == "</tool_result>" || trimmed.ends_with("</tool_result>") {
					blocks.push(MessageBlock::Command {
						name: "result".into(),
						preview: String::new(),
						lines: std::mem::take(&mut cmd_lines),
						expanded_hint: None,
						title: Some("Result".into()),
					});
					mode = ParseMode::Text;
				} else {
					cmd_lines.push(line);
				}
			}
			ParseMode::Approval => {
				if trimmed == "```" {
					blocks.push(MessageBlock::Approval { lines: std::mem::take(&mut approval_lines) });
					mode = ParseMode::Text;
				} else {
					approval_lines.push(line);
				}
			}
			ParseMode::Subagent => {
				if trimmed == "</subagent>" {
					blocks.push(MessageBlock::Subagent {
						name: std::mem::take(&mut sub_name),
						lines: std::mem::take(&mut sub_lines),
					});
					mode = ParseMode::Text;
				} else {
					sub_lines.push(line);
				}
			}
		}
	}

	match mode {
		ParseMode::Thinking => {
			blocks.push(MessageBlock::Thinking { lines: think_lines });
		}
		ParseMode::Command => {
			let title = title_from_preview(&cmd_preview);
			let hint = default_open_hint(&cmd_name, &cmd_preview).or(Some(true));
			blocks.push(MessageBlock::Command {
				name: cmd_name,
				preview: cmd_preview,
				lines: cmd_lines,
				expanded_hint: hint,
				title,
			});
		}
		ParseMode::ToolCall => {
			push_tool_block(&mut blocks, tool_raw, true);
		}
		ParseMode::ToolResult => {
			blocks.push(MessageBlock::Command {
				name: "result".into(),
				preview: String::new(),
				lines: cmd_lines,
				expanded_hint: Some(true),
				title: Some("Result".into()),
			});
		}
		ParseMode::Subagent => {
			blocks.push(MessageBlock::Subagent { name: sub_name, lines: sub_lines });
		}
		ParseMode::Approval => {
			blocks.push(MessageBlock::Approval { lines: approval_lines });
		}
		ParseMode::Text => flush_text(&mut blocks, &mut text_buf),
	}

	blocks
}

/// Extract tool name + short preview from JSON inside `<tool_call>`.
fn parse_tool_call_json(raw: &str) -> (String, String) {
	let s = raw.trim();
	// Prefer serde when valid JSON
	if let Ok(v) = serde_json::from_str::<serde_json::Value>(s) {
		let name = v
			.get("name")
			.or_else(|| v.get("tool"))
			.and_then(|x| x.as_str())
			.unwrap_or("tool")
			.to_string();
		let preview = v
			.get("arguments")
			.or_else(|| v.get("args"))
			.and_then(|a| {
				a.get("command")
					.or_else(|| a.get("cmd"))
					.or_else(|| a.get("query"))
					.or_else(|| a.get("path"))
					.and_then(|x| x.as_str())
					.map(|s| s.chars().take(60).collect::<String>())
			})
			.or_else(|| v.get("arguments").and_then(|a| a.as_str()).map(|s| s.chars().take(60).collect()))
			.unwrap_or_default();
		return (name, preview);
	}
	// Fallback: regex-ish name field
	let name = s
		.find("\"name\"")
		.and_then(|i| {
			let rest = &s[i + 6..];
			let start = rest.find('"')? + 1;
			let end = rest[start..].find('"')? + start;
			Some(rest[start..end].to_string())
		})
		.unwrap_or_else(|| "tool".into());
	let preview = s
		.find("\"command\"")
		.and_then(|i| {
			let rest = &s[i + 9..];
			let start = rest.find('"')? + 1;
			let end = rest[start..].find('"')? + start;
			Some(rest[start..end].chars().take(60).collect())
		})
		.unwrap_or_default();
	(name, preview)
}

fn command_status_from_preview(preview: &str, first_line: Option<&str>) -> &'static str {
	let p = preview.to_ascii_lowercase();
	if p.contains("status=running") {
		return "running";
	}
	if p.contains("status=error") {
		return "error";
	}
	if p.contains("status=done") {
		return "done";
	}
	if let Some(l) = first_line {
		let t = l.trim();
		if t == "… running" || t.starts_with("… running") {
			return "running";
		}
	}
	"done"
}

fn title_from_preview(preview: &str) -> Option<String> {
	// preview may start with `title=Read status=done …`
	if let Some(rest) = preview.strip_prefix("title=") {
		let title = rest.split_whitespace().next()?.to_string();
		if !title.is_empty() {
			return Some(title);
		}
	}
	// `title=Read` anywhere
	if let Some(i) = preview.find("title=") {
		let rest = &preview[i + 6..];
		let title = rest.split_whitespace().next()?.to_string();
		if !title.is_empty() {
			return Some(title);
		}
	}
	None
}

fn default_open_hint(name: &str, preview: &str) -> Option<bool> {
	let status = command_status_from_preview(preview, None);
	if status == "running" || status == "error" {
		return Some(true);
	}
	use crate::tools::ToolKind;
	if let Some(k) = ToolKind::from_name(name) {
		return Some(k.default_open());
	}
	None
}

/// Default-open for the Nth command block in content (for per-block toggle).
fn default_open_for_command_index(content: &str, index: usize) -> bool {
	let mut i = 0usize;
	for block in parse_message_blocks(content) {
		if let MessageBlock::Command { name, preview, lines, expanded_hint, .. } = block {
			if i == index {
				if let Some(h) = expanded_hint {
					return h;
				}
				let status = command_status_from_preview(&preview, lines.first().copied());
				if status == "running" || status == "error" {
					return true;
				}
				return crate::tools::ToolKind::from_name(&name).map(|k| k.default_open()).unwrap_or(false);
			}
			i += 1;
		}
	}
	false
}

/// Default visible body lines for shell / tools (then "Click to expand").
const TOOL_PREVIEW_LINES: usize = 6;
/// Hard cap when fully expanded.
const TOOL_MAX_LINES: usize = 200;
/// Subagent body cap.
const SUBAGENT_MAX_LINES: usize = 48;
/// Thought blocks longer than this auto-collapse when the turn finishes.
const THINK_AUTO_COLLAPSE_LINES: usize = 6;

fn strip_status_from_preview(preview: &str) -> String {
	let mut s = preview.to_string();
	for token in ["status=running", "status=error", "status=done"] {
		s = s.replace(token, "");
	}
	// Strip title=Word token
	if let Some(i) = s.find("title=") {
		let after = &s[i + 6..];
		let word_end = after.find(char::is_whitespace).unwrap_or(after.len());
		let end = i + 6 + word_end;
		s = format!("{}{}", &s[..i], &s[end..]);
	}
	s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn extract_attr(s: &str, key: &str) -> Option<String> {
	let pattern = format!("{key}=\"");
	let start = s.find(&pattern)? + pattern.len();
	let rest = &s[start..];
	let end = rest.find('"')?;
	Some(rest[..end].to_string())
}

#[cfg(test)]
mod tool_parse_tests {
	use super::*;

	#[test]
	fn parses_xml_tool_call() {
		let raw = r#"Here:
<tool_call>
{"name": "bash", "arguments": {"command": "git status"}}
</tool_call>
done."#;
		let blocks = parse_message_blocks(raw);
		assert!(
			blocks.iter().any(|b| matches!(
				b,
				MessageBlock::Command { name, preview, .. }
					if name == "bash" && preview.contains("git status")
			)),
			"blocks={blocks:?}"
		);
	}

	#[test]
	fn streaming_render_does_not_flash_raw_stars() {
		let lines = render_message_blocks(
			"Working on **partial",
			&ChatTheme::dark_fallback(),
			false,
			false,
			false,
			None,
			None,
		);
		let joined: String = lines
			.iter()
			.flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
			.collect::<Vec<_>>()
			.join("");
		assert!(!joined.contains("**"), "raw stars leaked: {joined}");
		assert!(joined.contains("partial") || joined.contains("Working"), "got {joined}");
	}

	#[test]
	fn heading_and_bold_preview_hide_markers() {
		let lines = markdown_preview_lines("## Hello **world**", Style::default(), None);
		let joined: String = lines
			.iter()
			.flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
			.collect::<Vec<_>>()
			.join("");
		assert!(!joined.contains('#'), "raw hash leaked: {joined}");
		assert!(!joined.contains("**"), "raw stars leaked: {joined}");
		assert!(joined.contains("Hello"), "got {joined}");
		assert!(joined.contains("world"), "got {joined}");
	}

	#[test]
	fn thinking_accordion_label_and_no_raw_md() {
		let lines = render_message_blocks(
			"<think>\n**idea** about topic\n</think>\n# Hello",
			&ChatTheme::dark_fallback(),
			false,
			false,
			false,
			Some(std::time::Duration::from_millis(676)),
			None,
		);
		let joined: String = lines
			.iter()
			.flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
			.collect::<Vec<_>>()
			.join("\n");
		assert!(joined.contains("Thought · 676ms") || joined.contains("Thought"), "got {joined}");
		assert!(joined.contains("▸") || joined.contains("▾"), "chevron missing: {joined}");
		assert!(!joined.contains("**"), "raw stars leaked: {joined}");
		assert!(!joined.contains('#'), "raw hash leaked: {joined}");
		assert!(joined.contains("Hello"), "got {joined}");
	}

	#[test]
	fn thinking_single_line_and_thinking_tag() {
		let lines = render_message_blocks(
			"<thinking>quick plan</thinking>\nAnswer here",
			&ChatTheme::dark_fallback(),
			false,
			false,
			false,
			Some(std::time::Duration::from_millis(42)),
			None,
		);
		let joined: String = lines
			.iter()
			.flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
			.collect::<Vec<_>>()
			.join("\n");
		assert!(joined.contains("Thought · 42ms") || joined.contains("Thought"), "got {joined}");
		assert!(joined.contains("Answer here"), "got {joined}");
	}

	#[test]
	fn tools_default_collapsed() {
		let lines = render_message_blocks(
			"<tool_call>\n{\"name\":\"bash\",\"arguments\":{\"command\":\"git status\"}}\n</tool_call>\nDone.",
			&ChatTheme::dark_fallback(),
			false,
			false,
			false,
			None,
			None,
		);
		let joined: String = lines
			.iter()
			.flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
			.collect::<Vec<_>>()
			.join("\n");
		assert!(joined.contains("Terminal"), "got {joined}");
		assert!(joined.contains("▸") || joined.contains("▾"), "got {joined}");
		// body should not include full json when collapsed
		assert!(!joined.contains("\"arguments\""), "tool body leaked while collapsed: {joined}");
	}

	#[test]
	fn clean_copy_strips_protocol() {
		let msg = Message::assistant(
			"<think>secret plan</think>\n```command name=\"shell\" title=\"Terminal\" status=\"done\"\n$ ls\n```\nHello world".into(),
		);
		let copy = msg.copy_text();
		assert!(!copy.contains("```command"), "got {copy}");
		assert!(!copy.contains("<think>"), "got {copy}");
		assert!(copy.contains("Hello world"), "got {copy}");
	}

	#[test]
	fn footer_always_for_assistant() {
		let mut msg = Message::assistant("hi".into());
		msg.footer_profile = Some("Write".into());
		msg.footer_model = Some("gpt-test".into());
		msg.footer_duration = Some(std::time::Duration::from_secs(2));
		let foot = msg.footer_line().expect("footer");
		assert!(foot.contains("Write"), "got {foot}");
		assert!(foot.contains("gpt-test"), "got {foot}");
		assert!(foot.contains("2.0s") || foot.contains("2s"), "got {foot}");
	}
}

/// Empty rows after the last message (keep small — avoid huge empty gap).
pub const MESSAGE_LIST_BOTTOM_PAD: usize = 2;

/// Visual + hit width of the message-list / sidebar scrollbar track (columns).
pub const SCROLLBAR_TRACK_WIDTH: u16 = 1;
/// Extra gap between message body and the scrollbar / right edge (not the sidebar).
pub const MESSAGE_LIST_RIGHT_PAD: u16 = 2;
/// Horizontal inset for selection highlight so it doesn't hug the text flush.
pub const MESSAGE_SELECTION_PAD: u16 = 1;
/// Gap between chat input text (and spinner/loader) and the input box right border.
pub const INPUT_BOX_RIGHT_PAD: u16 = 1;
/// Columns reserved for the space-hold / load spinner inside the input (glyph + gap).
pub const INPUT_SPINNER_RESERVE: u16 = 1;
/// Columns reserved for Ctrl+S voice frequency bars on the right of the input box.
/// Wide professional meter (~half of a typical input on narrow terminals, capped).
pub const INPUT_VOICE_WAVE_RESERVE: u16 = 36;

/// Right-sidebar solid background (`#151515`).
pub const SIDEBAR_BG: ratatui::style::Color = ratatui::style::Color::Rgb(0x15, 0x15, 0x15);
/// Scrollbar track fill (`#242424`) — matches dx code-editor default track.
pub const SCROLLBAR_TRACK_BG: ratatui::style::Color = ratatui::style::Color::Rgb(0x24, 0x24, 0x24);
/// Scrollbar track on hover (lighter rail).
pub const SCROLLBAR_TRACK_HOVER_BG: ratatui::style::Color =
	ratatui::style::Color::Rgb(0x40, 0x40, 0x40);
/// Scrollbar thumb default (`#555555`).
pub const SCROLLBAR_THUMB_BG: ratatui::style::Color = ratatui::style::Color::Rgb(0x55, 0x55, 0x55);
/// Scrollbar thumb on hover (near-white like the code editor).
pub const SCROLLBAR_THUMB_HOVER_BG: ratatui::style::Color =
	ratatui::style::Color::Rgb(0xcc, 0xcc, 0xcc);
/// Soft muted text for footers / secondary labels (readable, not near-black).
pub const SOFT_MUTED_FG: ratatui::style::Color = ratatui::style::Color::Rgb(0x9a, 0x9a, 0x9a);

// ── Semantic palette used across all block renderers ────────────────────────
/// Thinking block foreground: soft lavender-grey.
pub const THINK_FG: ratatui::style::Color = ratatui::style::Color::Rgb(0x9f, 0xa8, 0xd8);
/// Shell/command output gutter colour.
pub const SHELL_GUTTER: ratatui::style::Color = ratatui::style::Color::Rgb(0x44, 0x8a, 0xff);
/// Subagent block foreground: muted teal.
pub const SUBAGENT_FG: ratatui::style::Color = ratatui::style::Color::Rgb(0x56, 0xc7, 0xb4);
/// Diff addition line background tint (dark green).
pub const DIFF_ADD_BG: ratatui::style::Color = ratatui::style::Color::Rgb(0x0d, 0x2a, 0x0d);
/// Diff deletion line background tint (dark red).
pub const DIFF_DEL_BG: ratatui::style::Color = ratatui::style::Color::Rgb(0x2a, 0x0d, 0x0d);
/// Diff hunk header colour.
pub const DIFF_HUNK: ratatui::style::Color = ratatui::style::Color::Rgb(0x60, 0xcc, 0xee);
/// Diff add fg.
pub const DIFF_ADD_FG: ratatui::style::Color = ratatui::style::Color::Rgb(0x4a, 0xe5, 0x8a);
/// Diff del fg.
pub const DIFF_DEL_FG: ratatui::style::Color = ratatui::style::Color::Rgb(0xff, 0x5c, 0x5c);

/// Draw a vertical scrollbar on the right edge of `area`.
///
/// Matches the dx code-editor scrollbar: solid track + solid thumb backgrounds
/// (no box-drawing gaps). Track/thumb lighten when `hovered`.
///
/// Legacy `track_style` / `thumb_style` are ignored for colors — kept so call
/// sites compile — actual colors come from the editor-aligned constants and `hovered`.
pub fn render_scrollbar_track(
	area: Rect,
	buf: &mut Buffer,
	content_len: usize,
	position: usize,
	_track_style: Style,
	_thumb_style: Style,
	bar_width: u16,
) {
	render_scrollbar_track_hover(area, buf, content_len, position, false, bar_width);
}

/// Same as [`render_scrollbar_track`] with explicit hover styling.
pub fn render_scrollbar_track_hover(
	area: Rect,
	buf: &mut Buffer,
	content_len: usize,
	position: usize,
	hovered: bool,
	bar_width: u16,
) {
	if area.height == 0 || area.width == 0 || bar_width == 0 {
		return;
	}
	let viewport = area.height as usize;
	let max_scroll = content_len.saturating_sub(viewport);
	if max_scroll == 0 {
		return;
	}

	let bar_width = bar_width.min(area.width);
	let track_h = area.height as usize;
	let pos = position.min(max_scroll);

	let thumb_h = ((viewport * track_h) / content_len.max(1)).max(1).min(track_h);
	let travel = track_h.saturating_sub(thumb_h);
	let thumb_top = if travel == 0 { 0 } else { (pos * travel) / max_scroll };

	let x0 = area.x + area.width.saturating_sub(bar_width);
	let track_bg = if hovered { SCROLLBAR_TRACK_HOVER_BG } else { SCROLLBAR_TRACK_BG };
	let thumb_bg = if hovered { SCROLLBAR_THUMB_HOVER_BG } else { SCROLLBAR_THUMB_BG };

	// Solid background fills (editor-style) — avoid glyph gaps in some terminals.
	for row in 0..track_h {
		let y = area.y + row as u16;
		let on_thumb = row >= thumb_top && row < thumb_top + thumb_h;
		let bg = if on_thumb { thumb_bg } else { track_bg };
		for col in 0..bar_width {
			let cell = &mut buf[(x0 + col, y)];
			cell.reset();
			cell.set_char(' ');
			cell.set_bg(bg);
			cell.set_fg(bg);
		}
	}
}

/// Total rendered height of one message (matches MessageList `y` advances).
/// Compact: minimal chrome so the list does not explode vertically.
pub fn message_rendered_height(msg: &Message) -> usize {
	message_rendered_height_for_width(msg, 80)
}

/// Width-aware height (hard-wrap long lines so scroll matches paint).
pub fn message_rendered_height_for_width(msg: &Message, width: usize) -> usize {
	// Must match paint width exactly or lines wrap extra and stack on the next message.
	let list_w = width.max(12);
	// Assistant paints with MESSAGE_SELECTION_PAD inset on the left.
	let assistant_w = list_w.saturating_sub(MESSAGE_SELECTION_PAD as usize).max(8);
	let content_lines = if msg.content.is_empty() {
		1
	} else if msg.role == MessageRole::Assistant {
		// Prefer in-memory parts when present (production paint path).
		let lines = render_assistant_lines(msg, &ChatTheme::dark_fallback(), Some(assistant_w), false);
		let clipped = clip_lines_to_width(lines.into_iter().map(|(l, _)| l).collect(), assistant_w);
		clipped.len().max(1)
	} else {
		// User bubble: text_w matches paint (content_w − borders − hpad*2 − edge)
		let text_w = list_w.saturating_sub(2 + 2 + 1).max(8);
		let mut body = user_media_chip_lines(&msg.content, &ChatTheme::dark_fallback());
		body.extend(render_user_message_lines(&msg.content, &ChatTheme::dark_fallback(), text_w));
		body = clip_lines_to_width(body, text_w);
		body.retain(|l| l.spans.iter().any(|s| !s.content.trim().is_empty()));
		body.len().max(1)
	};

	// Single footer row: `Write · model · time · tokens · regenerate · branch`
	let footer =
		if msg.role == MessageRole::Assistant && msg.footer_line().is_some() { 1 } else { 0 };
	// Turn marker is inserted only when previous visible message is User — callers
	// that know list context should use `message_rendered_height_with_context`.
	match msg.role {
		// top border + body + bottom border  (NO header / "You" row, NO extra gap)
		MessageRole::User => content_lines.saturating_add(2),
		// body + footer + small gap (turn mark added via context-aware helper)
		MessageRole::Assistant => content_lines + footer + 1,
	}
}

/// Whether the previous non-hidden message is a user turn (turn-mark "· · ·" is painted).
pub fn previous_visible_is_user(messages: &[Message], idx: usize) -> bool {
	messages[..idx.min(messages.len())]
		.iter()
		.rev()
		.find(|m| !m.hidden)
		.is_some_and(|m| m.role == MessageRole::User)
}

/// Height matching paint, including the after-user turn mark when applicable.
pub fn message_rendered_height_with_context(
	messages: &[Message],
	idx: usize,
	width: usize,
) -> usize {
	let Some(msg) = messages.get(idx) else {
		return 0;
	};
	if msg.hidden {
		return 0;
	}
	message_rendered_height_for_width(msg, width)
}

/// Render user prompt body as tight lines only (never inserts a blank header row).
pub fn render_user_message_lines(
	content: &str,
	theme: &ChatTheme,
	width: usize,
) -> Vec<Line<'static>> {
	if content.trim().is_empty() {
		return Vec::new();
	}
	let base = Style::default().fg(theme.fg);
	// Strip attachment-marker lines shown as chips instead
	let body: String = content
		.lines()
		.filter(|l| {
			let t = l.trim();
			!(t.starts_with("[image]")
				|| t.starts_with("[file]")
				|| t.starts_with("📎")
				|| t.starts_with("image:")
				|| t.starts_with("file:"))
		})
		.collect::<Vec<_>>()
		.join("\n");
	let body = body.trim().to_string();
	if body.is_empty() {
		return Vec::new();
	}

	// Prefer per-source-line rendering for normal prompts (tight, no MD paragraph blanks).
	// Only use block markdown when fences/lists truly need structure.
	let needs_blocks = body.contains("```")
		|| body.lines().any(|l| {
			let t = l.trim_start();
			t.starts_with("# ")
				|| t.starts_with("## ")
				|| t.starts_with("- ")
				|| t.starts_with("* ")
				|| t.starts_with("1. ")
		});

	let mut lines: Vec<Line<'static>> = if needs_blocks {
		let md = markdown_preview_lines(&body, base, Some(width));
		if md.is_empty() {
			body
				.lines()
				.filter(|l| !l.trim().is_empty())
				.map(|l| render_inline_markdown(l, base))
				.collect()
		} else {
			md
		}
	} else {
		body
			.lines()
			.filter(|l| !l.trim().is_empty())
			.flat_map(|l| {
				// Hard-wrap long plain lines to width so height matches paint
				let wrapped = hard_wrap_text(l, width);
				wrapped
					.into_iter()
					.filter(|w| !w.trim().is_empty())
					.map(|w| render_inline_markdown(&w, base))
					.collect::<Vec<_>>()
			})
			.collect()
	};

	// Aggressively drop blank rows (empty spans or whitespace-only)
	lines.retain(|l| l.spans.iter().any(|s| !s.content.trim().is_empty()));
	lines
}

/// Compact chips for images / files / code blocks in a user message.
fn user_media_chip_lines(content: &str, theme: &ChatTheme) -> Vec<Line<'static>> {
	let mut chips = Vec::new();
	let chip = |icon: &str, label: String, color: Color| {
		Line::from(vec![
			Span::styled(format!("{icon} "), Style::default().fg(color).add_modifier(Modifier::BOLD)),
			Span::styled(label, Style::default().fg(theme.muted_fg).add_modifier(Modifier::ITALIC)),
		])
	};

	// Markdown images ![alt](path)
	for line in content.lines() {
		if let Some(rest) = line.trim().strip_prefix("![")
			&& let Some(end) = rest.find("](")
		{
			let alt = &rest[..end];
			let path = rest[end + 2..].trim_end_matches(')');
			let label = if alt.is_empty() {
				path.chars().take(40).collect()
			} else {
				format!(
					"{} · {}",
					alt.chars().take(20).collect::<String>(),
					path.chars().take(24).collect::<String>()
				)
			};
			chips.push(chip("◇", format!("Image · {label}"), theme.primary));
		}
		let t = line.trim();
		if let Some(p) = t.strip_prefix("[image]").or_else(|| t.strip_prefix("image:")) {
			chips.push(chip(
				"◇",
				format!("Image · {}", p.trim().chars().take(40).collect::<String>()),
				theme.primary,
			));
		}
		if let Some(p) = t.strip_prefix("[file]").or_else(|| t.strip_prefix("file:")) {
			chips.push(chip(
				"◆",
				format!("File · {}", p.trim().chars().take(40).collect::<String>()),
				theme.success(),
			));
		}
	}
	// Code fences
	let mut in_fence = false;
	let mut fence_lang = String::new();
	let mut fence_lines = 0usize;
	for line in content.lines() {
		let t = line.trim();
		if t.starts_with("```") {
			if in_fence {
				let lang = if fence_lang.is_empty() { "code".to_string() } else { fence_lang.clone() };
				chips.push(chip("◇", format!("Code · {lang} · {fence_lines} lines"), theme.warning()));
				in_fence = false;
				fence_lang.clear();
				fence_lines = 0;
			} else {
				in_fence = true;
				fence_lang = t.trim_start_matches('`').trim().to_string();
				fence_lines = 0;
			}
		} else if in_fence {
			fence_lines += 1;
		}
	}
	chips.truncate(4);
	chips
}

fn hard_wrap_line_count(display_cols: usize, width: usize) -> usize {
	if width == 0 {
		return 1;
	}
	display_cols.div_ceil(width).max(1)
}

/// Display columns for a span string (handles wide emoji / CJK).
fn display_width(s: &str) -> usize {
	s.width()
}

fn line_display_width(line: &Line<'_>) -> usize {
	line.spans.iter().map(|s| display_width(s.content.as_ref())).sum()
}

pub fn messages_total_height_for_width(messages: &[Message], width: usize) -> usize {
	let body: usize = messages
		.iter()
		.enumerate()
		.filter(|(_, m)| !m.hidden)
		.map(|(i, _)| message_rendered_height_with_context(messages, i, width))
		.sum();
	let any = messages.iter().any(|m| !m.hidden);
	if !any { 0 } else { body + MESSAGE_LIST_BOTTOM_PAD }
}

/// Format a thought/tool duration for accordion headers (`676ms`, `1.2s`).
pub fn format_short_duration(d: std::time::Duration) -> String {
	let ms = d.as_millis();
	if ms < 1000 {
		format!("{ms}ms")
	} else if ms < 60_000 {
		format!("{:.1}s", d.as_secs_f32())
	} else {
		format!("{}m {:02}s", d.as_secs() / 60, d.as_secs() % 60)
	}
}

fn is_diff_output(lines: &[&str]) -> bool {
	lines.iter().any(|l| {
		let t = l.trim();
		t.starts_with("--- ") || t.starts_with("+++ ") || t.starts_with("@@ -")
	})
}

/// Render a single diff line with beautiful per-line gutter + background tint.
fn render_diff_line(line: &str, line_no: usize, _muted: Style) -> Line<'static> {
	let trimmed = line.trim_start();
	let gutter_style = Style::default().fg(ratatui::style::Color::Rgb(0x55, 0x55, 0x55));
	let ln = format!("{line_no:>4} ");

	if let Some(rest) = trimmed.strip_prefix("+++") {
		Line::from(vec![
			Span::styled(ln, gutter_style),
			Span::styled("+ ", Style::default().fg(DIFF_ADD_FG).add_modifier(Modifier::BOLD)),
			Span::styled(
				rest.trim_start().to_string(),
				Style::default().fg(DIFF_ADD_FG).add_modifier(Modifier::BOLD),
			),
		])
	} else if let Some(rest) = trimmed.strip_prefix("---") {
		Line::from(vec![
			Span::styled(ln, gutter_style),
			Span::styled("- ", Style::default().fg(DIFF_DEL_FG).add_modifier(Modifier::BOLD)),
			Span::styled(
				rest.trim_start().to_string(),
				Style::default().fg(DIFF_DEL_FG).add_modifier(Modifier::BOLD),
			),
		])
	} else if trimmed.starts_with("@@") {
		Line::from(vec![
			Span::styled("     ", gutter_style),
			Span::styled(
				trimmed.to_string(),
				Style::default().fg(DIFF_HUNK).add_modifier(Modifier::BOLD),
			),
		])
	} else if let Some(rest) = trimmed.strip_prefix('+') {
		Line::from(vec![
			Span::styled(ln, gutter_style),
			Span::styled("+ ", Style::default().fg(DIFF_ADD_FG).add_modifier(Modifier::BOLD)),
			Span::styled(rest.to_string(), Style::default().fg(DIFF_ADD_FG)),
		])
	} else if let Some(rest) = trimmed.strip_prefix('-') {
		Line::from(vec![
			Span::styled(ln, gutter_style),
			Span::styled("- ", Style::default().fg(DIFF_DEL_FG).add_modifier(Modifier::BOLD)),
			Span::styled(rest.to_string(), Style::default().fg(DIFF_DEL_FG)),
		])
	} else {
		Line::from(vec![
			Span::styled(ln, gutter_style),
			Span::styled("  ", gutter_style),
			Span::styled(
				trimmed.to_string(),
				Style::default().fg(ratatui::style::Color::Rgb(0xbb, 0xbb, 0xbb)),
			),
		])
	}
}

fn is_terminal_tool(name: &str) -> bool {
	matches!(
		name.to_ascii_lowercase().as_str(),
		"bash"
			| "shell"
			| "zsh"
			| "sh"
			| "cmd"
			| "powershell"
			| "terminal"
			| "run_terminal_command"
			| "execute"
			| "exec"
	)
}

/// Soft fill for a hovered user bubble (card surface, else a gentle lift off bg).
fn user_bubble_hover_bg(theme: &ChatTheme) -> Color {
	// Prefer theme card surface when it differs from bg; otherwise lift dark/light neutrals.
	if theme.card != theme.bg {
		return theme.card;
	}
	match theme.bg {
		Color::Rgb(r, g, b) => {
			let lift = |c: u8| -> u8 { c.saturating_add(18) };
			// Light themes: darken slightly instead of lightening into white.
			let luminance = u16::from(r) + u16::from(g) + u16::from(b);
			if luminance > 500 {
				let drop = |c: u8| -> u8 { c.saturating_sub(14) };
				Color::Rgb(drop(r), drop(g), drop(b))
			} else {
				Color::Rgb(lift(r), lift(g), lift(b))
			}
		}
		_ => theme.muted,
	}
}

/// Production markdown preview: never paints raw `**` / `` ` `` / `#` / fences.
/// Incomplete streaming markers open a style until end-of-line/input.
fn markdown_preview_lines(
	text: &str,
	base: Style,
	content_width: Option<usize>,
) -> Vec<Line<'static>> {
	if text.trim().is_empty() {
		return Vec::new();
	}
	// Prefer full block markdown so fenced ```markdown / ```rust render as code cards.
	// Only fall back to streaming-safe inline when the input has no block fences and
	// the rendered output still leaks raw emphasis markers (incomplete stream).
	let has_fence = text.contains("```") || text.contains("~~~");
	let lines = crate::markdown_render::render_markdown_blocks(text, base, content_width);
	if has_fence {
		return lines;
	}
	let joined: String =
		lines.iter().flat_map(|l| l.spans.iter().map(|s| s.content.as_ref())).collect();
	if joined.contains("**") || joined.contains("__") || joined.contains("~~") {
		return text.lines().map(|l| render_inline_markdown(l, base)).collect();
	}
	lines
}

/// Inline markdown → styled spans. Markers never appear in output.
fn render_inline_markdown(input: &str, base: Style) -> Line<'static> {
	let chars: Vec<char> = input.chars().collect();
	let mut spans: Vec<Span<'static>> = Vec::new();
	let mut buf = String::new();
	let mut i = 0usize;
	let mut bold = false;
	let mut italic = false;
	let mut strike = false;

	let style_now = |bold: bool, italic: bool, strike: bool, base: Style| -> Style {
		let mut s = base;
		if bold {
			s = s.add_modifier(Modifier::BOLD);
		}
		if italic {
			s = s.add_modifier(Modifier::ITALIC);
		}
		if strike {
			s = s.add_modifier(Modifier::CROSSED_OUT);
		}
		s
	};

	let flush = |buf: &mut String,
	             spans: &mut Vec<Span<'static>>,
	             bold: bool,
	             italic: bool,
	             strike: bool,
	             base: Style| {
		if !buf.is_empty() {
			spans.push(Span::styled(std::mem::take(buf), style_now(bold, italic, strike, base)));
		}
	};

	while i < chars.len() {
		// Inline code `...` (unclosed → rest is code, no backtick shown)
		if chars[i] == '`' {
			flush(&mut buf, &mut spans, bold, italic, strike, base);
			i += 1;
			let start = i;
			while i < chars.len() && chars[i] != '`' {
				i += 1;
			}
			let code: String = chars[start..i].iter().collect();
			if !code.is_empty() {
				spans.push(Span::styled(
					code,
					base.add_modifier(Modifier::DIM).add_modifier(Modifier::ITALIC),
				));
			}
			if i < chars.len() && chars[i] == '`' {
				i += 1; // consume closing
			}
			continue;
		}

		// ** bold ** or __ bold __
		if i + 1 < chars.len()
			&& ((chars[i] == '*' && chars[i + 1] == '*') || (chars[i] == '_' && chars[i + 1] == '_'))
		{
			flush(&mut buf, &mut spans, bold, italic, strike, base);
			bold = !bold;
			i += 2;
			continue;
		}

		// ~~ strike ~~
		if i + 1 < chars.len() && chars[i] == '~' && chars[i + 1] == '~' {
			flush(&mut buf, &mut spans, bold, italic, strike, base);
			strike = !strike;
			i += 2;
			continue;
		}

		// * italic * or _ italic _ (single)
		if chars[i] == '*' || chars[i] == '_' {
			let delim = chars[i];
			// Don't treat list-like "* " mid-parse here (block level handles lists)
			flush(&mut buf, &mut spans, bold, italic, strike, base);
			italic = !italic;
			let _ = delim;
			i += 1;
			continue;
		}

		// [label](url) → label only; incomplete [label]( or [label → label text
		if chars[i] == '[' {
			if let Some((label, consumed)) = parse_md_link(&chars[i..]) {
				flush(&mut buf, &mut spans, bold, italic, strike, base);
				spans.push(Span::styled(
					label,
					style_now(bold, italic, strike, base).add_modifier(Modifier::UNDERLINED),
				));
				i += consumed;
				continue;
			}
			// incomplete bare "[" — skip the bracket glyph
			i += 1;
			continue;
		}

		// Skip leftover closing markdown punctuation that would look raw
		if matches!(chars[i], '*' | '_' | '~' | '`') {
			i += 1;
			continue;
		}

		buf.push(chars[i]);
		i += 1;
	}

	flush(&mut buf, &mut spans, bold, italic, strike, base);
	if spans.is_empty() {
		spans.push(Span::styled(String::new(), base));
	}
	Line::from(spans)
}

/// Parse `[label](url)` or incomplete `[label](` / `[label]` from char slice.
/// Returns (label, chars_consumed).
fn parse_md_link(chars: &[char]) -> Option<(String, usize)> {
	if chars.first() != Some(&'[') {
		return None;
	}
	let mut i = 1usize;
	while i < chars.len() && chars[i] != ']' {
		i += 1;
	}
	if i >= chars.len() {
		// unclosed [label — take rest as label, consume all
		let label: String = chars[1..].iter().collect();
		return Some((label, chars.len()));
	}
	let label: String = chars[1..i].iter().collect();
	i += 1; // ]
	if i < chars.len() && chars[i] == '(' {
		i += 1;
		while i < chars.len() && chars[i] != ')' {
			i += 1;
		}
		if i < chars.len() && chars[i] == ')' {
			i += 1;
		}
		// incomplete url still shows label only
		return Some((label, i));
	}
	// [label] without url
	Some((label, i))
}

/// Render structured assistant blocks: markdown preview + collapsed accordions.
pub fn render_message_blocks<'a>(
	content: &'a str,
	theme: &'a ChatTheme,
	thinking_expanded: bool,
	commands_expanded: bool,
	subagents_expanded: bool,
	thinking_duration: Option<std::time::Duration>,
	content_width: Option<usize>,
) -> Vec<Line<'static>> {
	render_message_blocks_tagged(
		content,
		theme,
		thinking_expanded,
		commands_expanded,
		subagents_expanded,
		thinking_duration,
		content_width,
	)
	.into_iter()
	.map(|(l, _)| l)
	.collect()
}

/// Tagged render (legacy static tags) — prefer `render_message_blocks_tagged_ex`.
pub fn render_message_blocks_tagged<'a>(
	content: &'a str,
	theme: &'a ChatTheme,
	thinking_expanded: bool,
	commands_expanded: bool,
	subagents_expanded: bool,
	thinking_duration: Option<std::time::Duration>,
	content_width: Option<usize>,
) -> Vec<(Line<'static>, Option<InteractiveBlock>)> {
	let empty = std::collections::HashMap::new();
	render_message_blocks_tagged_ex(
		content,
		theme,
		thinking_expanded,
		commands_expanded,
		subagents_expanded,
		thinking_duration,
		content_width,
		&empty,
		&empty,
	)
}

/// Full tagged render with per-block expand maps.
/// Production path: structured `msg_ui` parser + theme-aware cards.
#[allow(clippy::too_many_arguments)]
pub fn render_message_blocks_tagged_ex(
	content: &str,
	theme: &ChatTheme,
	thinking_expanded: bool,
	commands_expanded: bool,
	subagents_expanded: bool,
	thinking_duration: Option<std::time::Duration>,
	content_width: Option<usize>,
	command_expand: &std::collections::HashMap<usize, bool>,
	subagent_expand: &std::collections::HashMap<usize, bool>,
) -> Vec<(Line<'static>, Option<InteractiveBlock>)> {
	let empty_think = std::collections::HashMap::new();
	render_message_blocks_tagged_ex2(
		content,
		theme,
		thinking_expanded,
		commands_expanded,
		subagents_expanded,
		thinking_duration,
		content_width,
		command_expand,
		subagent_expand,
		&empty_think,
		false,
	)
}

/// Extended paint entry (per-thinking expand + streaming flag).
#[allow(clippy::too_many_arguments)]
pub fn render_message_blocks_tagged_ex2(
	content: &str,
	theme: &ChatTheme,
	thinking_expanded: bool,
	commands_expanded: bool,
	subagents_expanded: bool,
	thinking_duration: Option<std::time::Duration>,
	content_width: Option<usize>,
	command_expand: &std::collections::HashMap<usize, bool>,
	subagent_expand: &std::collections::HashMap<usize, bool>,
	thinking_expand: &std::collections::HashMap<usize, bool>,
	streaming: bool,
) -> Vec<(Line<'static>, Option<InteractiveBlock>)> {
	let ctx = crate::msg_ui::RenderCtx {
		theme,
		thinking_expanded,
		commands_expanded,
		subagents_expanded,
		thinking_duration,
		content_width,
		command_expand,
		subagent_expand,
		thinking_expand,
		streaming,
	};
	crate::msg_ui::render_parts_tagged(content, &ctx)
}

/// Production paint: use `Message.parts` when available, else parse `content`.
pub fn render_assistant_lines(
	msg: &Message,
	theme: &ChatTheme,
	content_width: Option<usize>,
	streaming: bool,
) -> Vec<(Line<'static>, Option<InteractiveBlock>)> {
	let ctx = crate::msg_ui::RenderCtx {
		theme,
		thinking_expanded: msg.thinking_expanded,
		commands_expanded: msg.commands_expanded,
		subagents_expanded: msg.subagents_expanded,
		thinking_duration: msg.thinking_duration,
		content_width,
		command_expand: &msg.command_expand,
		subagent_expand: &msg.subagent_expand,
		thinking_expand: &msg.thinking_expand,
		streaming,
	};
	if !msg.parts.is_empty() {
		crate::msg_ui::render_parts_list(&msg.parts, &ctx)
	} else {
		crate::msg_ui::render_parts_tagged(&msg.content, &ctx)
	}
}

/// Push professionally styled tool body lines (diff / read / shell / todo / web).
fn push_tool_body_lines(
	out: &mut Vec<(Line<'static>, Option<InteractiveBlock>)>,
	name: &str,
	kind: Option<crate::tools::ToolKind>,
	body: &[&str],
	status: &str,
	muted: Style,
	theme: &ChatTheme,
) {
	let is_diff = is_diff_output(body)
		|| matches!(
			kind,
			Some(
				crate::tools::ToolKind::Write
					| crate::tools::ToolKind::Edit
					| crate::tools::ToolKind::ApplyPatch
			)
		);
	let is_read = matches!(kind, Some(crate::tools::ToolKind::Read))
		|| name.eq_ignore_ascii_case("read")
		|| name.eq_ignore_ascii_case("read_file");
	let is_todo = matches!(kind, Some(crate::tools::ToolKind::TodoWrite))
		|| name.eq_ignore_ascii_case("todowrite")
		|| name.eq_ignore_ascii_case("todo");
	let is_web =
		matches!(kind, Some(crate::tools::ToolKind::WebSearch | crate::tools::ToolKind::WebFetch))
			|| name.to_ascii_lowercase().contains("web");
	let is_shell = is_terminal_tool(name) || matches!(kind, Some(crate::tools::ToolKind::Shell));

	if is_diff {
		let sep_style = Style::default().fg(theme.muted_fg);
		out.push((
			Line::from(Span::styled(
				"  ─────────────────────────────────────────────".to_string(),
				sep_style,
			)),
			None,
		));
		for (i, cmd_line) in body.iter().enumerate() {
			let clipped: String = cmd_line.chars().take(300).collect();
			out.push((render_diff_line(&clipped, i + 1, muted), None));
		}
		out.push((
			Line::from(Span::styled(
				"  ─────────────────────────────────────────────".to_string(),
				sep_style,
			)),
			None,
		));
		return;
	}

	if is_read {
		// Numbered source with light syntax colouring
		for cmd_line in body {
			out.push((render_read_line(cmd_line, muted), None));
		}
		return;
	}

	if is_todo {
		for cmd_line in body {
			out.push((render_todo_line(cmd_line, theme), None));
		}
		return;
	}

	if is_web {
		for cmd_line in body {
			out.push((render_web_line(cmd_line, muted), None));
		}
		return;
	}

	// Shell / generic tool output
	let gutter_style = Style::default().fg(if is_shell { SHELL_GUTTER } else { theme.muted_fg });
	for cmd_line in body {
		let clipped: String = cmd_line.chars().take(240).collect();
		let line_style = if status == "error" {
			Style::default().fg(theme.danger())
		} else if clipped.starts_with('$') {
			Style::default().fg(theme.success()).add_modifier(Modifier::BOLD)
		} else {
			Style::default().fg(theme.fg)
		};
		let mut spans = vec![Span::styled("  │ ", gutter_style)];
		if is_shell {
			spans.push(Span::styled(clipped, line_style));
		} else {
			spans.extend(render_inline_markdown(&clipped, line_style).spans);
		}
		out.push((Line::from(spans), None));
	}
}

/// Render a Read tool line: `  12│ code…` with line number + syntax tint.
fn render_read_line(line: &str, muted: Style) -> Line<'static> {
	// Prefer `    12|code` or `    12│code` from exec_read
	let (num, code) = if let Some((n, rest)) = line.split_once('|') {
		let n = n.trim();
		if n.chars().all(|c| c.is_ascii_digit()) {
			(n.to_string(), rest)
		} else {
			(String::new(), line)
		}
	} else if let Some((n, rest)) = line.split_once('│') {
		let n = n.trim();
		if n.chars().all(|c| c.is_ascii_digit()) {
			(n.to_string(), rest)
		} else {
			(String::new(), line)
		}
	} else {
		(String::new(), line)
	};

	let mut spans = Vec::new();
	if !num.is_empty() {
		spans.push(Span::styled(
			format!("{num:>5}│ "),
			Style::default().fg(ratatui::style::Color::Rgb(0x55, 0x55, 0x66)),
		));
	} else {
		spans.push(Span::styled("      │ ", muted));
	}
	// Lightweight syntax: keywords / strings
	spans.extend(highlight_code_spans(code));
	Line::from(spans)
}

fn highlight_code_spans(code: &str) -> Vec<Span<'static>> {
	let keywords = [
		"pub",
		"fn",
		"let",
		"mut",
		"if",
		"else",
		"match",
		"return",
		"struct",
		"impl",
		"use",
		"mod",
		"trait",
		"for",
		"while",
		"loop",
		"const",
		"static",
		"type",
		"enum",
		"function",
		"var",
		"await",
		"async",
		"class",
		"interface",
		"export",
		"import",
		"from",
		"default",
		"true",
		"false",
		"null",
		"None",
		"Some",
		"Ok",
		"Err",
	];
	let mut spans = Vec::new();
	let mut word = String::new();
	let mut other = String::new();
	let flush_other = |other: &mut String, spans: &mut Vec<Span<'static>>| {
		if !other.is_empty() {
			spans.push(Span::styled(
				std::mem::take(other),
				Style::default().fg(ratatui::style::Color::Rgb(0xcc, 0xcc, 0xcc)),
			));
		}
	};
	for c in code.chars() {
		if c.is_alphanumeric() || c == '_' {
			if !other.is_empty() {
				flush_other(&mut other, &mut spans);
			}
			word.push(c);
		} else {
			if !word.is_empty() {
				let color = if keywords.contains(&word.as_str()) {
					ratatui::style::Color::Rgb(0xff, 0x7b, 0x72)
				} else if word.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
					ratatui::style::Color::Rgb(0x79, 0xc0, 0xff)
				} else {
					ratatui::style::Color::Rgb(0xcc, 0xcc, 0xcc)
				};
				spans.push(Span::styled(std::mem::take(&mut word), Style::default().fg(color)));
			}
			other.push(c);
		}
	}
	if !word.is_empty() {
		let color = if keywords.contains(&word.as_str()) {
			ratatui::style::Color::Rgb(0xff, 0x7b, 0x72)
		} else {
			ratatui::style::Color::Rgb(0xcc, 0xcc, 0xcc)
		};
		spans.push(Span::styled(word, Style::default().fg(color)));
	}
	flush_other(&mut other, &mut spans);
	if spans.is_empty() {
		spans.push(Span::raw(String::new()));
	}
	spans
}

fn render_todo_line(line: &str, theme: &ChatTheme) -> Line<'static> {
	let t = line.trim();
	// Glyphs from TaskStatus: ○ / ◐ / ✓ / ✕
	let (icon, color, text) = if let Some(rest) = t.strip_prefix("✓ ") {
		("☑", theme.success(), rest)
	} else if let Some(rest) = t.strip_prefix("◐ ") {
		("◐", theme.primary, rest)
	} else if let Some(rest) = t.strip_prefix("✕ ") {
		("☒", theme.danger(), rest)
	} else if let Some(rest) = t.strip_prefix("○ ") {
		("☐", theme.border, rest)
	} else {
		("•", theme.fg, t)
	};
	Line::from(vec![
		Span::styled(format!("  {icon} "), Style::default().fg(color).add_modifier(Modifier::BOLD)),
		Span::styled(text.to_string(), Style::default().fg(theme.fg)),
	])
}

fn render_web_line(line: &str, muted: Style) -> Line<'static> {
	let t = line.trim();
	// Markdown link-ish result rows
	if t.contains("](") && t.contains('[') {
		let mut spans = vec![Span::styled("  ↗ ", Style::default().fg(SHELL_GUTTER))];
		spans.extend(
			render_inline_markdown(t, Style::default().fg(ratatui::style::Color::Rgb(0x9e, 0xcb, 0xff)))
				.spans,
		);
		return Line::from(spans);
	}
	if t.starts_with("http://") || t.starts_with("https://") {
		return Line::from(vec![
			Span::styled("  ↗ ", Style::default().fg(SHELL_GUTTER)),
			Span::styled(
				t.to_string(),
				Style::default()
					.fg(ratatui::style::Color::Rgb(0x6c, 0xb6, 0xff))
					.add_modifier(Modifier::UNDERLINED),
			),
		]);
	}
	// Numbered "1. Title" rows from search
	if t.chars().next().is_some_and(|c| c.is_ascii_digit()) && t.contains(". ") {
		return Line::from(vec![
			Span::styled("  • ", Style::default().fg(SHELL_GUTTER)),
			Span::styled(
				t.to_string(),
				Style::default()
					.fg(ratatui::style::Color::Rgb(0xdd, 0xdd, 0xee))
					.add_modifier(Modifier::BOLD),
			),
		]);
	}
	Line::from(vec![
		Span::styled("  │ ", muted),
		Span::styled(t.to_string(), Style::default().fg(ratatui::style::Color::Rgb(0xbb, 0xbb, 0xbb))),
	])
}

#[derive(Debug, Clone)]
pub struct Message {
	/// Stable id for branching / regenerate graph.
	pub id: String,
	/// Parent message id (conversation tree edge).
	pub parent_id: Option<String>,
	/// Branch this message belongs to (`main` or fork id).
	pub branch_id: String,
	/// Hidden when viewing another branch.
	pub hidden: bool,
	pub role: MessageRole,
	/// Wire format (session persistence / export). Always kept in sync with `parts`.
	pub content: String,
	/// In-memory paint model — primary source of truth for the message list.
	pub parts: Vec<crate::msg_ui::StreamPart>,
	pub timestamp: chrono::DateTime<chrono::Local>,
	pub token_count: usize,
	/// Expand/collapse thinking accordion (global default).
	pub thinking_expanded: bool,
	/// Expand/collapse command / tool blocks (global default).
	pub commands_expanded: bool,
	/// Expand/collapse subagent blocks (global default).
	pub subagents_expanded: bool,
	/// Per-tool-block expand overrides (index → open).
	pub command_expand: std::collections::HashMap<usize, bool>,
	/// Per-subagent-block expand overrides (index → open).
	pub subagent_expand: std::collections::HashMap<usize, bool>,
	/// Per-thinking-block expand overrides (index → open).
	pub thinking_expand: std::collections::HashMap<usize, bool>,
	/// Footer (assistant): profile label e.g. Write.
	pub footer_profile: Option<String>,
	/// Footer (assistant): model display name.
	pub footer_model: Option<String>,
	/// Footer (assistant): wall time for the turn.
	pub footer_duration: Option<std::time::Duration>,
	/// Footer (assistant): reasoning effort label e.g. Medium.
	pub footer_reasoning: Option<String>,
	/// Wall time spent in `<think>` (shown as `Thought · 676ms`).
	pub thinking_duration: Option<std::time::Duration>,
	/// Optional input tokens for this turn (footer metrics).
	pub tokens_in: Option<u32>,
	/// Optional output tokens for this turn (footer metrics).
	pub tokens_out: Option<u32>,
	/// Number of tool calls completed in this turn.
	pub tool_count: u32,
	/// Turn was aborted mid-stream.
	pub interrupted: bool,
}

/// Interactive hit targets inside an assistant message body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InteractiveBlock {
	Thinking,
	Command {
		index: usize,
	},
	Subagent {
		index: usize,
	},
	Approval,
	/// In-stream permission action: 0=once, 1=always, 2=deny.
	PermissionAction {
		action: u8,
	},
	/// In-stream question option index.
	QuestionOption {
		index: usize,
	},
	/// Confirm current question selection.
	QuestionConfirm,
	/// Diff review: 0=accept, 1=reject, 2=open.
	DiffReview {
		index: usize,
		action: u8,
	},
	/// Open path from read/tool card.
	OpenPath {
		index: usize,
	},
	Plan,
	PlanStep {
		index: usize,
	},
	/// Interactive PTY attach (session id via FNV hash).
	PtyAttach {
		session_id_hash: u64,
	},
	PtyKill {
		session_id_hash: u64,
	},
	ContextGroup,
	/// Regenerate this assistant turn.
	Regenerate,
	/// Fork a branch from this message.
	BranchFromHere,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageRole {
	User,
	Assistant,
}

impl Message {
	pub fn new_id() -> String {
		use std::time::{SystemTime, UNIX_EPOCH};
		let t = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
		format!("m{t:x}")
	}

	pub fn user(content: String) -> Self {
		let token_count = count_tokens(&content);
		Self {
			id: Self::new_id(),
			parent_id: None,
			branch_id: "main".into(),
			hidden: false,
			role: MessageRole::User,
			content,
			parts: Vec::new(),
			timestamp: chrono::Local::now(),
			token_count,
			thinking_expanded: false,
			commands_expanded: false,
			subagents_expanded: false,
			command_expand: std::collections::HashMap::new(),
			subagent_expand: std::collections::HashMap::new(),
			thinking_expand: std::collections::HashMap::new(),
			footer_profile: None,
			footer_model: None,
			footer_duration: None,
			footer_reasoning: None,
			thinking_duration: None,
			tokens_in: None,
			tokens_out: None,
			tool_count: 0,
			interrupted: false,
		}
	}

	pub fn assistant(content: String) -> Self {
		let token_count = count_tokens(&content);
		let parts =
			if content.is_empty() { Vec::new() } else { crate::msg_ui::rebuild_parts(&content, None) };
		Self {
			id: Self::new_id(),
			parent_id: None,
			branch_id: "main".into(),
			hidden: false,
			role: MessageRole::Assistant,
			content,
			parts,
			timestamp: chrono::Local::now(),
			token_count,
			// Thoughts expand while streaming; auto-collapse if long when the turn ends.
			thinking_expanded: true,
			// Tools use per-block preview by default.
			commands_expanded: false,
			subagents_expanded: false,
			command_expand: std::collections::HashMap::new(),
			subagent_expand: std::collections::HashMap::new(),
			thinking_expand: std::collections::HashMap::new(),
			footer_profile: None,
			footer_model: None,
			footer_duration: None,
			footer_reasoning: None,
			thinking_duration: None,
			tokens_in: None,
			tokens_out: None,
			tool_count: 0,
			interrupted: false,
		}
	}

	/// Expand / collapse every tool + subagent in this message.
	pub fn set_details_expanded(&mut self, open: bool) {
		self.commands_expanded = open;
		self.subagents_expanded = open;
		self.command_expand.clear();
		self.subagent_expand.clear();
	}

	/// Rebuild `parts` from wire `content` (after load / bulk mutation).
	pub fn sync_parts_from_content(&mut self) {
		if self.role != MessageRole::Assistant {
			self.parts.clear();
			return;
		}
		self.parts = crate::msg_ui::rebuild_parts(&self.content, self.thinking_duration);
	}

	/// Rebuild wire `content` from in-memory `parts` (after live mutations).
	pub fn sync_content_from_parts(&mut self) {
		if self.role != MessageRole::Assistant || self.parts.is_empty() {
			return;
		}
		self.content = crate::msg_ui::parts_to_wire(&self.parts);
		self.token_count = count_tokens(&self.content);
	}

	/// Line count inside the first thinking block (for auto-collapse).
	pub fn thinking_line_count(&self) -> usize {
		for block in parse_message_blocks(&self.content) {
			if let MessageBlock::Thinking { lines } = block {
				return lines.len();
			}
		}
		0
	}

	/// Remove raw XML tool invocations once real tool results are present.
	pub fn strip_xml_tool_tags(&mut self) {
		let tags = [
			"shell",
			"bash",
			"run_terminal_command",
			"terminal",
			"read",
			"read_file",
			"write",
			"write_file",
			"edit",
			"search_replace",
			"grep",
			"glob",
			"list",
			"list_dir",
			"websearch",
			"web_search",
			"webfetch",
			"web_fetch",
		];
		let mut out = self.content.clone();
		// Work on indices from a lowercase scan, edit the original string carefully.
		// Simpler multipass per tag with regex-like find.
		for tag in tags {
			// Self-closing: <tag .../>
			loop {
				let lo = out.to_ascii_lowercase();
				let open = format!("<{tag}");
				let Some(start) = lo.find(&open) else { break };
				let rest = &out[start..];
				let Some(gt) = rest.find('>') else { break };
				let end = start + gt + 1;
				// Only strip if it looks like a self-closing or empty tool tag
				let slice = &rest[..=gt];
				if slice.contains("command=")
					|| slice.contains("path=")
					|| slice.contains("query=")
					|| slice.contains("url=")
					|| slice.contains("pattern=")
					|| slice.ends_with("/>")
					|| slice.contains("/>")
				{
					out.replace_range(start..end, "");
					continue;
				}
				// Paired tag <tag ...>...</tag>
				let close = format!("</{tag}>");
				if let Some(c_rel) = lo[end..].find(&close) {
					let c_end = end + c_rel + close.len();
					out.replace_range(start..c_end, "");
					continue;
				}
				break;
			}
		}
		// Clean leftover blank runs
		while out.contains("\n\n\n") {
			out = out.replace("\n\n\n", "\n\n");
		}
		if out != self.content {
			self.content = out;
			self.token_count = count_tokens(&self.content);
			self.sync_parts_from_content();
		}
	}

	pub fn footer_line(&self) -> Option<String> {
		if self.role != MessageRole::Assistant {
			return None;
		}
		let profile = self.footer_profile.as_deref()?;
		let model = self.footer_model.as_deref()?;
		let time = self.footer_duration.map(|d| {
			if d.as_secs() < 60 {
				format!("{:.1}s", d.as_secs_f32())
			} else {
				format!("{}m {:02}s", d.as_secs() / 60, d.as_secs() % 60)
			}
		})?;
		let reasoning = self.footer_reasoning.as_deref().map(|r| format!(" · {r}")).unwrap_or_default();
		let tokens = match (self.tokens_in, self.tokens_out) {
			(Some(i), Some(o)) => format!(" · ↑{i} ↓{o}"),
			(Some(i), None) => format!(" · ↑{i}"),
			(None, Some(o)) => format!(" · ↓{o}"),
			(None, None) if self.token_count > 0 => format!(" · ~{} tok", self.token_count),
			_ => String::new(),
		};
		let mut s = format!("{profile} · {model}{reasoning} · {time}{tokens}");
		if self.interrupted {
			s.push_str(" · interrupted");
		}
		Some(s)
	}

	/// Plain text suitable for clipboard — clean answer/tools, no protocol soup.
	pub fn copy_text(&self) -> String {
		if self.role == MessageRole::Assistant {
			crate::msg_ui::clean_copy_text(&self.content)
		} else {
			self.content.clone()
		}
	}

	/// Raw protocol content (debug / session export).
	pub fn copy_raw(&self) -> String {
		self.content.clone()
	}

	pub fn append_content(&mut self, content: &str) {
		if content.is_empty() {
			return;
		}
		self.content.push_str(content);
		// Cheap live estimate — exact counting runs when the turn finishes.
		// Full tiktoken encode on every SSE token freezes input during streaming.
		self.token_count = self.content.len().div_ceil(4);
		if self.role == MessageRole::Assistant {
			self.append_stream_delta(content);
		}
	}

	/// Fast path for plain text/thinking deltas; full reparse only on structural markers.
	fn append_stream_delta(&mut self, delta: &str) {
		if self.parts.is_empty() || delta_requires_parts_rebuild(delta) {
			self.sync_parts_from_content();
			return;
		}
		let in_thinking = matches!(
			self.parts.last(),
			Some(crate::msg_ui::StreamPart::Thinking { streaming: true, .. })
		);
		if in_thinking {
			crate::msg_ui::append_thinking_part(&mut self.parts, delta, self.thinking_duration);
		} else {
			crate::msg_ui::append_text_part(&mut self.parts, delta);
		}
	}

	/// Replace a matching running tool fence with the completed result (or append).
	pub fn upgrade_tool_result(&mut self, result_fence: &str) {
		crate::tools::upgrade_running_tool_block(&mut self.content, result_fence);
		self.token_count = self.content.len().div_ceil(4);
		// Count completed tool fences (best-effort for footer metrics).
		self.tool_count = self
			.content
			.matches("status=\"done\"")
			.count()
			.saturating_add(self.content.matches("status=\"error\"").count()) as u32;
		if self.role == MessageRole::Assistant {
			self.sync_parts_from_content();
		}
	}

	/// Live shell/tool stdout into the matching running card (content + parts).
	pub fn apply_tool_delta(&mut self, id: &str, chunk: &str) {
		let _ = crate::stream_events::append_tool_delta(&mut self.content, id, chunk);
		if !crate::msg_ui::append_tool_body(&mut self.parts, id, chunk) {
			self.sync_parts_from_content();
		}
		self.token_count = self.content.len().div_ceil(4);
	}

	/// Toggle expand state for a specific tool/command block by index.
	/// Cycle: preview/default → full expand → collapsed → full expand …
	pub fn toggle_command_at(&mut self, index: usize) {
		match self.command_expand.get(&index).copied() {
			None => {
				// Was in default preview (or collapsed for non-default tools) → full expand
				self.command_expand.insert(index, true);
			}
			Some(true) => {
				// Fully expanded → collapse
				self.command_expand.insert(index, false);
			}
			Some(false) => {
				// Collapsed → full expand
				self.command_expand.insert(index, true);
			}
		}
	}

	pub fn command_is_expanded(&self, index: usize, default_open: bool) -> bool {
		self.command_expand.get(&index).copied().unwrap_or(default_open || self.commands_expanded)
	}

	pub fn toggle_subagent_at(&mut self, index: usize) {
		let current = self.subagent_is_expanded(index, false);
		self.subagent_expand.insert(index, !current);
	}

	pub fn subagent_is_expanded(&self, index: usize, default_open: bool) -> bool {
		self.subagent_expand.get(&index).copied().unwrap_or(default_open || self.subagents_expanded)
	}

	pub fn has_thinking(&self) -> bool {
		if self.content.contains("<think>")
			|| self.content.contains("<thinking>")
			|| self.thinking_duration.is_some()
		{
			return true;
		}
		self.parts.iter().any(|p| matches!(p, crate::msg_ui::StreamPart::Thinking { .. }))
	}

	pub fn has_commands(&self) -> bool {
		self.content.contains("```command")
			|| self.content.contains("```bash")
			|| self.content.contains("```sh")
			|| self.content.contains("```shell")
			|| self.content.contains("```json")
			|| self.content.contains("<tool_call")
			|| self.content.contains("<tool_result")
	}

	pub fn has_subagents(&self) -> bool {
		self.content.contains("<subagent")
	}

	pub fn toggle_thinking(&mut self) {
		self.thinking_expanded = !self.thinking_expanded;
		// Clear per-block overrides so global toggle is authoritative.
		self.thinking_expand.clear();
	}

	pub fn toggle_thinking_at(&mut self, index: usize) {
		let current = self.thinking_expand.get(&index).copied().unwrap_or(self.thinking_expanded);
		self.thinking_expand.insert(index, !current);
	}

	pub fn toggle_commands(&mut self) {
		self.commands_expanded = !self.commands_expanded;
		self.command_expand.clear();
	}

	pub fn toggle_subagents(&mut self) {
		self.subagents_expanded = !self.subagents_expanded;
		self.subagent_expand.clear();
	}

	/// True when the message has an in-flight tool (running fence) or open think.
	pub fn is_actively_working(&self) -> bool {
		self.content.contains("status=\"running\"") || self.content.contains("status=running") || {
			let opens =
				self.content.matches("<think>").count() + self.content.matches("<thinking>").count();
			let closes =
				self.content.matches("</think>").count() + self.content.matches("</thinking>").count();
			opens > closes
		}
	}
}

pub struct MessageList<'a> {
	messages: &'a [Message],
	theme: &'a ChatTheme,
	scroll_offset: usize,
	shimmer: Option<&'a ShimmerEffect>,
	typing_indicator: Option<&'a TypingIndicator>,
	show_timestamps: bool,
	/// Whether the chat scrollbar track/thumb is hovered (lighter colors).
	scrollbar_hovered: bool,
	/// Inclusive message index range currently selected for copy.
	selection: Option<(usize, usize)>,
	text_selection_start: Option<(usize, usize)>,
	text_selection_end: Option<(usize, usize)>,
	/// Label for user bubbles (default "You").
	user_label: &'a str,
	/// When true, the last assistant message is still streaming — soften MD markers.
	streaming: bool,
	/// Message index under the pointer (user bubble hover fill).
	hovered_message_index: Option<usize>,
}

impl<'a> MessageList<'a> {
	pub fn with_effects(
		messages: &'a [Message],
		theme: &'a ChatTheme,
		scroll_offset: usize,
		shimmer: &'a ShimmerEffect,
		typing_indicator: &'a TypingIndicator,
	) -> Self {
		Self {
			messages,
			theme,
			scroll_offset,
			shimmer: Some(shimmer),
			typing_indicator: Some(typing_indicator),
			show_timestamps: true,
			scrollbar_hovered: false,
			selection: None,
			text_selection_start: None,
			text_selection_end: None,
			user_label: "You",
			streaming: false,
			hovered_message_index: None,
		}
	}

	pub fn show_timestamps(mut self, show: bool) -> Self {
		self.show_timestamps = show;
		self
	}

	pub fn scrollbar_hovered(mut self, hovered: bool) -> Self {
		self.scrollbar_hovered = hovered;
		self
	}

	pub fn selection(mut self, range: Option<(usize, usize)>) -> Self {
		self.selection = range;
		self
	}

	pub fn text_selection(
		mut self,
		start: Option<(usize, usize)>,
		end: Option<(usize, usize)>,
	) -> Self {
		self.text_selection_start = start;
		self.text_selection_end = end;
		self
	}

	pub fn user_label(mut self, label: &'a str) -> Self {
		self.user_label = label;
		self
	}

	pub fn streaming(mut self, streaming: bool) -> Self {
		self.streaming = streaming;
		self
	}

	pub fn hovered_message_index(mut self, index: Option<usize>) -> Self {
		self.hovered_message_index = index;
		self
	}
}

impl Widget for MessageList<'_> {
	fn render(self, area: Rect, buf: &mut Buffer) {
		let mut y = area.y;
		let mut skipped_lines = 0usize;

		// Leave room for scrollbar + right padding (before sidebar, not inside it).
		let content_w = area
			.width
			.saturating_sub(SCROLLBAR_TRACK_WIDTH)
			.saturating_sub(MESSAGE_LIST_RIGHT_PAD)
			.max(1);
		if content_w == 0 || area.height == 0 {
			return;
		}

		// Full clear of the message viewport every frame — prevents ghost glyphs
		// from previous frames / shorter lines / scroll from sticking on top.
		for row in area.top()..area.bottom() {
			for col in area.left()..area.right() {
				let cell = &mut buf[(col, row)];
				cell.reset();
				cell.set_bg(self.theme.bg);
			}
		}

		let total_height = messages_total_height_for_width(self.messages, content_w as usize);

		for (msg_idx, msg) in self.messages.iter().enumerate() {
			if msg.hidden {
				continue;
			}
			if y >= area.bottom() {
				break;
			}

			let is_selected = self.selection.is_some_and(|(lo, hi)| msg_idx >= lo && msg_idx <= hi);
			let full_h = message_rendered_height_with_context(self.messages, msg_idx, content_w as usize);
			// Gap between messages is included in full_h as trailing 1.

			let mut message_skip_lines = 0;
			if skipped_lines < self.scroll_offset {
				let skip_amount = full_h.min(self.scroll_offset - skipped_lines);
				skipped_lines += skip_amount;
				if skip_amount < full_h {
					message_skip_lines = skip_amount;
				} else {
					continue;
				}
			}

			match msg.role {
				MessageRole::User => {
					// Tight bubble: ONLY body text + 2 border rows. No "You"/header row ever.
					const H_PAD: u16 = 1;
					const RIGHT_EDGE_GAP: u16 = 1;
					// Available columns for text inside a max-width bubble (limit to ~80% of width to ensure right-alignment)
					let user_max_w = ((content_w as usize) * 80 / 100).max(40);
					let text_w = user_max_w
						.saturating_sub(2) // left/right borders
						.saturating_sub((H_PAD * 2) as usize)
						.saturating_sub(RIGHT_EDGE_GAP as usize)
						.max(8);

					// Build body once — same list used for size + paint
					let mut body = user_media_chip_lines(&msg.content, self.theme);
					body.extend(render_user_message_lines(&msg.content, self.theme, text_w));
					body = clip_lines_to_width(body, text_w);
					// Kill any blank rows (this was the phantom "You" gap)
					body.retain(|l| l.spans.iter().any(|s| !s.content.trim().is_empty()));
					if body.is_empty() {
						let fallback: String = msg
							.content
							.lines()
							.find(|l| !l.trim().is_empty())
							.unwrap_or("")
							.chars()
							.take(text_w)
							.collect();
						if !fallback.is_empty() {
							body.push(Line::from(Span::raw(fallback)));
						} else {
							body.push(Line::from(Span::raw(" ")));
						}
					}

					let body_h = body.len();
					let max_line = body.iter().map(|l| line_display_width(l)).max().unwrap_or(1);
					// border(1) + hpad + text + hpad + border(1)
					let msg_width = (max_line + 2 + (H_PAD as usize) * 2)
						.min(content_w.saturating_sub(RIGHT_EDGE_GAP) as usize)
						.max(max_line + 2)
						.max(4) as u16;
					let msg_x = area.x + content_w.saturating_sub(msg_width).saturating_sub(RIGHT_EDGE_GAP);

					// full_h from height fn should equal body_h + 2; use live body_h for paint
					let total_rows = body_h + 2; // top border + body + bottom border
					let paint_h =
						total_rows.saturating_sub(message_skip_lines).min((area.bottom() - y) as usize).max(1);

					let msg_area = Rect { x: msg_x, y, width: msg_width, height: paint_h as u16 };

					let is_hovered = self.hovered_message_index == Some(msg_idx);
					let border_col = if is_selected {
						self.theme.accent
					} else if is_hovered {
						self.theme.primary
					} else {
						self.theme.border
					};
					// Soft filled plate on hover (card surface / primary-tinted).
					let bubble_bg = if is_hovered { user_bubble_hover_bg(self.theme) } else { self.theme.bg };

					// Fill bubble plate first so rounded border sits on colored surface.
					for row in msg_area.top()..msg_area.bottom() {
						for col in msg_area.left()..msg_area.right() {
							let cell = &mut buf[(col, row)];
							cell.reset();
							cell.set_bg(bubble_bg);
							cell.set_char(' ');
						}
					}

					// Draw border frame
					let block = Block::default()
						.borders(Borders::ALL)
						.border_type(ratatui::widgets::BorderType::Rounded)
						.border_style(Style::default().fg(border_col).bg(bubble_bg))
						.style(Style::default().bg(bubble_bg));
					let inner = block.inner(msg_area);
					block.render(msg_area, buf);

					// Paint body lines on the first inner row (immediately under top border).
					if inner.width > 0 && inner.height > 0 {
						let pad_w = inner.width.saturating_sub(H_PAD * 2).max(1) as usize;
						let mut painted = clip_lines_to_width(body, pad_w);
						painted.retain(|l| l.spans.iter().any(|s| !s.content.trim().is_empty()));
						if painted.is_empty() {
							painted.push(Line::from(Span::raw(" ")));
						}
						// Selection before pad so char indices match mouse hit-tests.
						apply_text_selection_to_lines(
							&mut painted,
							msg_idx,
							self.text_selection_start,
							self.text_selection_end,
						);
						painted = pad_lines_to_width(painted, pad_w, bubble_bg);

						// full bubble skip includes top border row; body starts after that.
						let body_skip = message_skip_lines.saturating_sub(1);
						let text_x = inner.x.saturating_add(H_PAD);
						let text_w_u16 = inner.width.saturating_sub(H_PAD * 2).max(1);

						// Clear only the text columns, keep border cells from Block
						for row in 0..inner.height {
							for col in 0..text_w_u16 {
								let cell = &mut buf[(text_x + col, inner.y + row)];
								cell.reset();
								cell.set_bg(bubble_bg);
								cell.set_char(' ');
							}
						}

						for (i, line) in painted.iter().skip(body_skip).enumerate() {
							if i as u16 >= inner.height {
								break;
							}
							Paragraph::new(line.clone())
								.style(Style::default().fg(self.theme.fg).bg(bubble_bg))
								.render(
									Rect { x: text_x, y: inner.y + i as u16, width: text_w_u16, height: 1 },
									buf,
								);
						}
					}

					// Advance by height-fn full_h minus any skipped lines
					y = y.saturating_add(full_h.saturating_sub(message_skip_lines) as u16);
				}
				MessageRole::Assistant => {
					// Production stream: structured cards + markdown body.
					let mut content_lines = if msg.content.is_empty() {
						// Empty stream: "Working…" when tools may be next, not always "Thinking".
						let label = "Thinking";
						if let Some(shimmer) = self.shimmer {
							let text = label.to_string();
							let mut spans = Vec::new();
							for (i, ch) in text.chars().enumerate() {
								let position = i as f32 / text.len().max(1) as f32;
								let shimmer_color = shimmer.shimmer_color_at(position);
								spans.push(Span::styled(
									ch.to_string(),
									Style::default().fg(shimmer_color).add_modifier(Modifier::ITALIC),
								));
							}
							vec![Line::from(spans)]
						} else {
							vec![Line::from(label)]
						}
					} else {
						let think_dur = msg.thinking_duration.or_else(|| {
							let open = msg.content.contains("<think>") || msg.content.contains("<thinking>");
							let closed = msg.content.contains("</think>") || msg.content.contains("</thinking>");
							if self.streaming && open && !closed {
								let now = chrono::Local::now();
								let ms = (now - msg.timestamp).num_milliseconds().max(0) as u64;
								Some(std::time::Duration::from_millis(ms))
							} else {
								msg.footer_duration.filter(|_| open || closed)
							}
						});
						let paint_w = content_w.saturating_sub(MESSAGE_SELECTION_PAD).max(1) as usize;
						// Prefer live thinking duration while streaming (footer may lag).
						let _ = think_dur;
						let painted: Vec<Line<'static>> =
							render_assistant_lines(msg, self.theme, Some(paint_w), self.streaming)
								.into_iter()
								.map(|(l, _)| l)
								.filter(|l| l.spans.iter().any(|s| !s.content.trim().is_empty()))
								.collect();
						// While only TITLE:/meta has arrived, keep the soft working label
						// instead of flashing empty / raw meta lines.
						if painted.is_empty() && self.streaming {
							vec![Line::from(Span::styled(
								"Thinking".to_string(),
								Style::default().fg(self.theme.muted_fg).add_modifier(Modifier::ITALIC),
							))]
						} else {
							painted
						}
					};

					// Paint width must match height calculation (selection pad inset).
					let pad = MESSAGE_SELECTION_PAD;
					let max_w = content_w.saturating_sub(pad).max(1) as usize;
					content_lines = clip_lines_to_width(content_lines, max_w);
					// Metrics footer only (no regenerate/branch action buttons).
					if let Some(foot) = msg.footer_line() {
						content_lines
							.push(Line::from(Span::styled(foot, Style::default().fg(self.theme.muted_fg))));
					}

					// Apply selection BEFORE padding so char indices match mouse hit-tests.
					apply_text_selection_to_lines(
						&mut content_lines,
						msg_idx,
						self.text_selection_start,
						self.text_selection_end,
					);

					// Pad every line to full width so residual cells never ghost
					content_lines = pad_lines_to_width(content_lines, max_w, self.theme.bg);

					let body_h = content_lines.len().max(1);
					let paint_h =
						body_h.saturating_sub(message_skip_lines).min((area.bottom() - y) as usize).max(1);

					let msg_area = Rect {
						x: area.x.saturating_add(pad),
						y,
						width: content_w.saturating_sub(pad),
						height: paint_h as u16,
					};

					if paint_h > 0 && msg_area.width > 0 {
						Paragraph::new(Text::from(content_lines))
							.style(Style::default().fg(self.theme.fg))
							.scroll((message_skip_lines as u16, 0))
							.render(msg_area, buf);
					}

					y = y.saturating_add(full_h.saturating_sub(message_skip_lines) as u16);
				}
			}
		}

		let viewport_height = area.height as usize;
		let max_scroll = total_height.saturating_sub(viewport_height);
		let position = self.scroll_offset.min(max_scroll);

		if max_scroll > 0 {
			render_scrollbar_track_hover(
				area,
				buf,
				total_height,
				position,
				self.scrollbar_hovered,
				SCROLLBAR_TRACK_WIDTH,
			);
		}
	}
}

/// Split text into hard-wrapped lines by **display columns** (not char count).
fn hard_wrap_text(s: &str, width: usize) -> Vec<String> {
	if width == 0 {
		return vec![s.to_string()];
	}
	let mut out = Vec::new();
	for line in s.lines() {
		if line.is_empty() {
			out.push(String::new());
			continue;
		}
		let mut current = String::new();
		let mut cols = 0usize;
		for ch in line.chars() {
			let cw = UnicodeWidthChar::width(ch).unwrap_or(0).max(1);
			if cols + cw > width && !current.is_empty() {
				out.push(std::mem::take(&mut current));
				cols = 0;
			}
			// If a single char is wider than width, still emit it alone
			current.push(ch);
			cols += cw;
			if cols >= width {
				out.push(std::mem::take(&mut current));
				cols = 0;
			}
		}
		if !current.is_empty() || cols == 0 {
			// push remainder; empty line already handled
			if !current.is_empty() {
				out.push(current);
			}
		}
	}
	if out.is_empty() {
		out.push(String::new());
	}
	out
}

/// Hard-wrap styled lines to `width` **display columns**. Never leaves overflow
/// that could paint into neighbouring message rects.
pub fn clip_lines_to_width(lines: Vec<Line<'static>>, width: usize) -> Vec<Line<'static>> {
	if width == 0 {
		return lines;
	}
	let mut out = Vec::new();
	for line in lines {
		if line_display_width(&line) <= width {
			out.push(line);
			continue;
		}

		// Preserve leading gutter prefixes on wrapped continuations
		let mut prefix_span: Option<Span<'static>> = None;
		let mut prefix_w = 0usize;
		if let Some(first) = line.spans.first() {
			let c = first.content.as_ref();
			if c == "  │ " || c == "▎ " || c.starts_with("  │") {
				prefix_span = Some(first.clone());
				prefix_w = display_width(c);
			}
		}

		let mut cur_spans: Vec<Span<'static>> = Vec::new();
		let mut cur_w = 0usize;

		let flush = |spans: &mut Vec<Span<'static>>,
		             out: &mut Vec<Line<'static>>,
		             cur_w: &mut usize,
		             prefix: &Option<Span<'static>>,
		             prefix_w: usize| {
			if spans.is_empty() {
				return;
			}
			// Drop orphan prefix-only lines
			if let Some(p) = prefix
				&& spans.len() == 1
				&& spans[0].content == p.content
			{
				spans.clear();
				*cur_w = 0;
				return;
			}
			out.push(Line::from(std::mem::take(spans)));
			*cur_w = 0;
			if let Some(p) = prefix {
				spans.push(p.clone());
				*cur_w = prefix_w;
			}
		};

		for span in line.spans {
			let style = span.style;
			let text = span.content.to_string();
			let mut rest = text.as_str();
			while !rest.is_empty() {
				let available = width.saturating_sub(cur_w);
				if available == 0 {
					flush(&mut cur_spans, &mut out, &mut cur_w, &prefix_span, prefix_w);
					continue;
				}
				// Take as many chars as fit in `available` display columns
				let mut take_cols = 0usize;
				let mut take_bytes = 0usize;
				for (i, ch) in rest.char_indices() {
					let cw = UnicodeWidthChar::width(ch).unwrap_or(0).max(1);
					if take_cols + cw > available {
						break;
					}
					take_cols += cw;
					take_bytes = i + ch.len_utf8();
				}
				if take_bytes == 0 {
					// Single wide char that can't fit remaining — force on new line
					if cur_w > 0 {
						flush(&mut cur_spans, &mut out, &mut cur_w, &prefix_span, prefix_w);
						continue;
					}
					// Force emit one char even if wider than width
					let ch = rest.chars().next().unwrap();
					let bl = ch.len_utf8();
					cur_spans.push(Span::styled(rest[..bl].to_string(), style));
					out.push(Line::from(std::mem::take(&mut cur_spans)));
					cur_w = 0;
					if let Some(p) = &prefix_span {
						cur_spans.push(p.clone());
						cur_w = prefix_w;
					}
					rest = &rest[bl..];
					continue;
				}
				let piece = &rest[..take_bytes];
				cur_spans.push(Span::styled(piece.to_string(), style));
				cur_w += take_cols;
				rest = &rest[take_bytes..];
				if cur_w >= width {
					flush(&mut cur_spans, &mut out, &mut cur_w, &prefix_span, prefix_w);
				}
			}
		}
		if !cur_spans.is_empty() {
			if let Some(p) = &prefix_span {
				if !(cur_spans.len() == 1 && cur_spans[0].content == p.content) {
					out.push(Line::from(cur_spans));
				}
			} else {
				out.push(Line::from(cur_spans));
			}
		}
	}
	if out.is_empty() {
		out.push(Line::from(""));
	}
	out
}

/// Pad each line with spaces to exactly `width` display columns so shorter lines
/// wipe previous-frame glyphs (prevents "fixed" ghost characters).
pub fn pad_lines_to_width(
	lines: Vec<Line<'static>>,
	width: usize,
	_bg: Color,
) -> Vec<Line<'static>> {
	if width == 0 {
		return lines;
	}
	lines
		.into_iter()
		.map(|mut line| {
			let w = line_display_width(&line);
			if w < width {
				line.spans.push(Span::raw(" ".repeat(width - w)));
			} else if w > width {
				// Safety: re-clip single line if somehow still wide
				return clip_lines_to_width(vec![line], width)
					.into_iter()
					.next()
					.unwrap_or_else(|| Line::from(" ".repeat(width)));
			}
			line
		})
		.collect()
}

pub fn apply_text_selection_to_lines(
	content_lines: &mut Vec<Line<'static>>,
	msg_idx: usize,
	text_selection_start: Option<(usize, usize)>,
	text_selection_end: Option<(usize, usize)>,
) {
	let mut char_idx_start = 0;
	let mut char_idx_end = 0;
	let mut has_text_selection = false;
	if let (Some(t_start), Some(t_end)) = (text_selection_start, text_selection_end) {
		let (m1, c1) = t_start;
		let (m2, c2) = t_end;
		let (s_m, s_c, e_m, e_c) = if m1 < m2 {
			(m1, c1, m2, c2)
		} else if m1 > m2 {
			(m2, c2, m1, c1)
		} else {
			let (c_start, c_end) = if c1 < c2 { (c1, c2) } else { (c2, c1) };
			(m1, c_start, m1, c_end)
		};
		if msg_idx >= s_m && msg_idx <= e_m {
			char_idx_start = if msg_idx == s_m { s_c } else { 0 };
			char_idx_end = if msg_idx == e_m { e_c } else { usize::MAX };
			if char_idx_start != char_idx_end {
				has_text_selection = true;
			}
		}
	}

	if !has_text_selection {
		return;
	}

	let mut current_char_idx = 0;
	let mut new_lines = Vec::new();
	for line in std::mem::take(content_lines) {
		let mut new_spans = Vec::new();
		for span in line.spans {
			let text = span.content.as_ref();
			let char_count = text.chars().count();
			let span_end = current_char_idx + char_count;

			if current_char_idx < char_idx_end && span_end > char_idx_start {
				let mut s_start = char_idx_start.saturating_sub(current_char_idx);
				let mut s_end = char_idx_end.saturating_sub(current_char_idx);
				s_start = s_start.min(char_count);
				s_end = s_end.min(char_count);

				if s_start > 0 {
					let prefix: String = text.chars().take(s_start).collect();
					new_spans.push(Span::styled(prefix, span.style));
				}
				if s_end > s_start {
					let mid: String = text.chars().skip(s_start).take(s_end - s_start).collect();
					new_spans.push(Span::styled(
						mid,
						span.style.bg(ratatui::style::Color::White).fg(ratatui::style::Color::Black),
					));
				}
				if s_end < char_count {
					let suffix: String = text.chars().skip(s_end).collect();
					new_spans.push(Span::styled(suffix, span.style));
				}
			} else {
				new_spans.push(span);
			}
			current_char_idx += char_count;
		}
		new_lines.push(Line::from(new_spans));
		// +1 matches char_index_at_display_pos newline accounting between rows.
		current_char_idx += 1;
	}
	*content_lines = new_lines;
}

/// Lines used for mouse selection / copy — must match message-list paint (no pad).
pub fn message_selection_lines(
	messages: &[Message],
	msg_idx: usize,
	list_w: usize,
	streaming: bool,
) -> Vec<Line<'static>> {
	let Some(msg) = messages.get(msg_idx) else {
		return Vec::new();
	};
	let theme = ChatTheme::dark_fallback();
	let paint_w = list_w.saturating_sub(MESSAGE_SELECTION_PAD as usize).max(8);
	let mut lines: Vec<Line<'static>> = if msg.role == MessageRole::Assistant {
		if msg.content.is_empty() {
			vec![Line::from("Thinking…".to_string())]
		} else {
			render_assistant_lines(msg, &theme, Some(paint_w), streaming)
				.into_iter()
				.map(|(l, _)| l)
				.collect()
		}
	} else {
		let text_w = list_w.saturating_sub(2 + 2 + 1).max(8);
		let mut body = user_media_chip_lines(&msg.content, &theme);
		body.extend(render_user_message_lines(&msg.content, &theme, text_w));
		body.retain(|l| l.spans.iter().any(|s| !s.content.trim().is_empty()));
		if body.is_empty() {
			body.push(Line::from(" "));
		}
		body
	};
	lines = clip_lines_to_width(lines, paint_w);
	// Turn marker removed — no longer shown between messages
	if let Some(foot) = msg.footer_line() {
		lines.push(Line::from(foot));
	}
	lines
}

/// Map a display column on a line to a character offset (Unicode width aware).
pub fn display_col_to_char_offset(text: &str, col: usize) -> usize {
	use unicode_width::UnicodeWidthChar;
	let mut cols = 0usize;
	for (i, ch) in text.chars().enumerate() {
		let w = ch.width().unwrap_or(0);
		if cols + w > col {
			return i;
		}
		cols += w;
		if cols >= col {
			return i + 1;
		}
	}
	text.chars().count()
}

/// Flatten selection lines to plain text with newlines (same layout as indices).
pub fn flatten_selection_lines(lines: &[Line<'static>]) -> String {
	let mut out = String::new();
	for (i, line) in lines.iter().enumerate() {
		if i > 0 {
			out.push('\n');
		}
		for span in &line.spans {
			out.push_str(span.content.as_ref());
		}
	}
	out
}
