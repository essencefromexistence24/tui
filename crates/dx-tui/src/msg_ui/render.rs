//! Production theme-aware renderer for stream parts.

#![allow(dead_code)]

use ratatui::{
	style::{Color, Modifier, Style},
	text::{Line, Span},
};
use std::collections::HashMap;
use std::time::Duration;

use crate::components::InteractiveBlock;
use crate::theme::ChatTheme;

use super::parse::{
	ExpandMode, LINE_CLIP_COLS, PartStatus, SUBAGENT_FULL_LINES, SUBAGENT_PREVIEW_LINES, StreamPart,
	TOOL_FULL_LINES, TOOL_PREVIEW_LINES, parse_stream_parts, resolve_subagent_expand,
	resolve_thinking_expand, resolve_tool_expand,
};

pub type TaggedLine = (Line<'static>, Option<InteractiveBlock>);

/// Context for a full assistant-message paint.
pub struct RenderCtx<'a> {
	pub theme: &'a ChatTheme,
	pub thinking_expanded: bool,
	pub commands_expanded: bool,
	pub subagents_expanded: bool,
	pub thinking_duration: Option<Duration>,
	pub content_width: Option<usize>,
	pub command_expand: &'a HashMap<usize, bool>,
	pub subagent_expand: &'a HashMap<usize, bool>,
	/// Per-thinking-block override (optional; falls back to thinking_expanded).
	pub thinking_expand: &'a HashMap<usize, bool>,
	pub streaming: bool,
}

/// Render content → tagged lines (parses wire format).
pub fn render_parts_tagged(content: &str, ctx: &RenderCtx<'_>) -> Vec<TaggedLine> {
	let parts = parse_stream_parts(content, ctx.thinking_duration);
	render_parts_list(&parts, ctx)
}

/// Render pre-built in-memory parts (primary production path).
pub fn render_parts_list(parts: &[StreamPart], ctx: &RenderCtx<'_>) -> Vec<TaggedLine> {
	if parts.is_empty() {
		return vec![(Line::from(""), None)];
	}
	let mut lines: Vec<TaggedLine> = Vec::new();
	let mut first = true;
	for part in parts {
		let need_gap = !first
			&& matches!(
				part,
				StreamPart::Text { .. }
					| StreamPart::Thinking { .. }
					| StreamPart::Compaction { .. }
					| StreamPart::Error { .. }
					| StreamPart::Approval { .. }
					| StreamPart::Interrupted
					| StreamPart::Question { .. }
					| StreamPart::Plan { .. }
					| StreamPart::Pty { .. }
			);
		if need_gap {
			lines.push((Line::from(""), None));
		}
		first = false;
		render_part(part, ctx, &mut lines);
	}
	if lines.is_empty() {
		lines.push((Line::from(""), None));
	}
	lines
}

/// Total logical lines for height calc (must match paint).
pub fn part_line_count(content: &str, ctx: &RenderCtx<'_>) -> usize {
	render_parts_tagged(content, ctx).len().max(1)
}

/// Hit-test: which interactive block owns relative_y.
pub fn hit_test_parts(
	content: &str,
	relative_y: usize,
	ctx: &RenderCtx<'_>,
) -> Option<InteractiveBlock> {
	let tagged = render_parts_tagged(content, ctx);
	let w = ctx.content_width.unwrap_or(80).max(8);
	let mut y = 0usize;
	for (line, tag) in tagged {
		let h = wrap_height(&line, w);
		if relative_y >= y && relative_y < y + h {
			return tag;
		}
		y += h;
	}
	None
}

fn wrap_height(line: &Line<'_>, width: usize) -> usize {
	let cols: usize =
		line.spans.iter().map(|s| unicode_width::UnicodeWidthStr::width(s.content.as_ref())).sum();
	if width == 0 {
		return 1;
	}
	cols.max(1).div_ceil(width).max(1)
}

// ── Part renderers ───────────────────────────────────────────────────────────

fn render_part(part: &StreamPart, ctx: &RenderCtx<'_>, out: &mut Vec<TaggedLine>) {
	match part {
		StreamPart::Text { body } => render_text(body, ctx, out),
		StreamPart::Thinking { index, body, duration, streaming } => {
			render_thinking(*index, body, *duration, *streaming, ctx, out)
		}
		StreamPart::Tool { index, name, title, status, preview, body, duration, .. } => {
			render_tool(*index, name, title, *status, preview, body, *duration, ctx, out)
		}
		StreamPart::Subagent { index, name, body, status } => {
			render_subagent(*index, name, body, *status, ctx, out)
		}
		StreamPart::Approval { call_id: _, tool, body, decision } => {
			render_approval(tool, body, decision, ctx, out)
		}
		StreamPart::Question { id: _, prompt, options, answer } => {
			render_question(prompt, options, answer, ctx, out)
		}
		StreamPart::Compaction { label, summary } => render_compaction(label, summary, ctx, out),
		StreamPart::Error { body } => render_error(body, ctx, out),
		StreamPart::Retry { body } => render_retry(body, ctx, out),
		StreamPart::ContextGroup { label } => render_context_group(label, "", ctx, out),
		StreamPart::Plan { title, body, steps } => render_plan(title, body, steps, ctx, out),
		StreamPart::Pty { id, title, lines, attached, alive } => {
			render_pty(id, title, lines, *attached, *alive, ctx, out)
		}
		StreamPart::Interrupted => render_interrupted(ctx, out),
	}
}

fn has_table(text: &str) -> bool {
	// A markdown table has a separator row like |---|---|
	text.lines().any(|l| {
		let t = l.trim();
		t.starts_with('|') && t.contains("---")
	})
}

fn render_text(body: &str, ctx: &RenderCtx<'_>, out: &mut Vec<TaggedLine>) {
	// Never paint TITLE: meta (or incomplete TITLE prefixes mid-stream).
	let body = crate::session_meta::sanitize_assistant_display_text(body, ctx.streaming);
	if body.trim().is_empty() {
		return;
	}
	let fg = Style::default().fg(ctx.theme.fg);
	let fence_ticks = body.matches("```").count() + body.matches("~~~").count();
	let has_open_fence = fence_ticks % 2 == 1;
	let has_closed_fence = fence_ticks >= 2 && fence_ticks.is_multiple_of(2);
	let has_table = has_table(&body);

	// While streaming: prefer streaming-safe inline so raw `**` / `#` / bare fences never flash.
	// Completed fences/tables still use the block renderer.
	if ctx.streaming && !has_table && !(has_closed_fence && !has_open_fence) {
		for raw in body.lines() {
			let t = raw.trim();
			if t.is_empty() {
				continue;
			}
			// Hide incomplete fence openers: ``` / ```rust / ~~~
			if t.starts_with("```") || t.starts_with("~~~") {
				continue;
			}
			let cleaned = strip_streaming_block_markers(raw);
			if cleaned.trim().is_empty() {
				continue;
			}
			out.push((inline_md(&cleaned, fg), None));
		}
		return;
	}

	// Incomplete streaming emphasis (** / __) must not flash raw markers.
	// Prefer block markdown for fences and tables; otherwise use streaming-safe inline.
	if has_closed_fence
		|| has_table
		|| (!body.contains("**") && !body.contains("__") && !body.contains("~~"))
	{
		let preview = crate::markdown_render::render_markdown_blocks(&body, fg, ctx.content_width);
		let joined: String =
			preview.iter().flat_map(|l| l.spans.iter().map(|s| s.content.as_ref())).collect();
		// If block MD still leaked raw markers (incomplete stream), fall back.
		if !(preview.is_empty()
			|| joined.contains("**")
			|| joined.contains("__")
			|| (joined.contains("~~") && body.contains("~~"))
			|| joined.contains("```")
			|| joined.contains("TITLE:"))
		{
			for l in preview {
				if l.spans.iter().all(|s| s.content.trim().is_empty()) {
					continue;
				}
				out.push((l, None));
			}
			return;
		}
	}
	for raw in body.lines() {
		if raw.trim().is_empty() {
			continue;
		}
		out.push((inline_md(raw, fg), None));
	}
}

/// Drop incomplete block markers so streaming paint never shows raw `#` / list-only junk.
fn strip_streaming_block_markers(line: &str) -> String {
	let mut t = line.trim_start();
	// Headings: "# ", "## ", "### "
	while t.starts_with('#') {
		t = t[1..].trim_start();
	}
	// Blockquote marker
	if let Some(rest) = t.strip_prefix("> ") {
		t = rest;
	} else if t == ">" {
		return String::new();
	}
	t.to_string()
}

fn render_thinking(
	index: usize,
	body: &str,
	duration: Option<Duration>,
	streaming: bool,
	ctx: &RenderCtx<'_>,
	out: &mut Vec<TaggedLine>,
) {
	let lines_n = body.lines().filter(|l| !l.trim().is_empty()).count();
	let open =
		resolve_thinking_expand(index, streaming, lines_n, ctx.thinking_expanded, ctx.thinking_expand)
			|| (streaming && ctx.streaming);
	let chevron = if open { "▾" } else { "▸" };
	let think_fg = ctx.theme.muted_fg;
	let hdr = Style::default().fg(think_fg).add_modifier(Modifier::BOLD);
	let label = if streaming && open {
		match duration {
			Some(d) => format!("Thinking · {}", format_dur(d)),
			None => "Thinking…".into(),
		}
	} else if let Some(d) = duration {
		format!("Thought · {}", format_dur(d))
	} else if lines_n > 0 {
		format!("Thought · {lines_n} lines")
	} else {
		"Thought".into()
	};
	out.push((
		Line::from(vec![Span::styled(format!("{chevron} "), hdr), Span::styled(label, hdr)]),
		Some(InteractiveBlock::Thinking),
	));
	if open {
		let body_style =
			Style::default().fg(blend(ctx.theme.muted_fg, think_fg, 0.7)).add_modifier(Modifier::ITALIC);
		let gutter = Style::default().fg(think_fg);
		if body.trim().is_empty() {
			out.push((
				Line::from(vec![Span::styled("  │ ", gutter), Span::styled("…", body_style)]),
				None,
			));
		} else {
			let md = crate::markdown_render::render_markdown_blocks(
				body,
				body_style,
				ctx.content_width.map(|w| w.saturating_sub(4)),
			);
			if md.is_empty() {
				for line in body.lines() {
					let mut spans = vec![Span::styled("  │ ", gutter)];
					spans.extend(inline_md(line, body_style).spans);
					out.push((Line::from(spans), None));
				}
			} else {
				for mut line in md {
					let mut spans = vec![Span::styled("  │ ", gutter)];
					spans.append(&mut line.spans);
					out.push((Line::from(spans), None));
				}
			}
		}
	}
}

#[allow(clippy::too_many_arguments)]
fn render_tool(
	index: usize,
	name: &str,
	title: &str,
	status: PartStatus,
	preview: &str,
	body: &str,
	duration: Option<Duration>,
	ctx: &RenderCtx<'_>,
	out: &mut Vec<TaggedLine>,
) {
	let expand = resolve_tool_expand(index, status, name, ctx.command_expand, ctx.commands_expanded);
	let kind = crate::tools::ToolKind::from_name(name);
	let is_shell = is_terminal(name) || matches!(kind, Some(crate::tools::ToolKind::Shell));
	let is_diff = is_diff_body(body)
		|| matches!(
			kind,
			Some(
				crate::tools::ToolKind::Write
					| crate::tools::ToolKind::Edit
					| crate::tools::ToolKind::ApplyPatch
			)
		);
	let is_read = matches!(kind, Some(crate::tools::ToolKind::Read));
	let is_todo = matches!(kind, Some(crate::tools::ToolKind::TodoWrite));
	let is_web =
		matches!(kind, Some(crate::tools::ToolKind::WebSearch | crate::tools::ToolKind::WebFetch))
			|| name.to_ascii_lowercase().contains("web");
	let is_mcp = matches!(kind, Some(crate::tools::ToolKind::McpTool)) || name.contains("__");
	let is_lsp = matches!(
		kind,
		Some(
			crate::tools::ToolKind::GoToDefinition
				| crate::tools::ToolKind::FindReferences
				| crate::tools::ToolKind::Hover
				| crate::tools::ToolKind::DocumentSymbols
				| crate::tools::ToolKind::WorkspaceSymbols
				| crate::tools::ToolKind::GoToImplementation
				| crate::tools::ToolKind::CallHierarchy
				| crate::tools::ToolKind::GetDiagnostics
				| crate::tools::ToolKind::CompleteCode
				| crate::tools::ToolKind::FormatCode
		)
	);

	let (icon, color) = match status {
		PartStatus::Running => ("●", ctx.theme.primary),
		PartStatus::Error => ("✗", ctx.theme.danger()),
		PartStatus::Done => ("✓", ctx.theme.success()),
	};
	let open = !matches!(expand, ExpandMode::Collapsed);
	let chevron =
		if matches!(expand, ExpandMode::Full | ExpandMode::Preview) { "▾" } else { "▸" };
	let hdr = Style::default().fg(color).add_modifier(Modifier::BOLD);
	let muted = Style::default().fg(ctx.theme.muted_fg);

	let mut title_s = title.to_string();
	if is_mcp {
		// server__tool → server · tool
		if let Some((srv, tool)) = name.split_once("__") {
			title_s = format!("MCP · {srv} · {tool}");
		}
	} else if is_lsp {
		title_s = format!("LSP · {title}");
	}

	let body_lines: Vec<&str> = body.lines().collect();
	let total = body_lines.len();
	let dur = duration.map(|d| format!(" · {}", format_dur(d))).unwrap_or_default();
	let prev = {
		let p = preview.trim();
		if p.is_empty() {
			String::new()
		} else {
			// Prefer tail of long paths
			let shown = if p.len() > 72 {
				let chars: Vec<char> = p.chars().collect();
				let take = 69.min(chars.len());
				format!("…{}", chars[chars.len() - take..].iter().collect::<String>())
			} else {
				p.to_string()
			};
			format!("  {shown}")
		}
	};
	let count = if total > 0 { format!(" · {total}") } else { String::new() };

	out.push((
		Line::from(vec![
			Span::styled(format!("{chevron} "), hdr),
			Span::styled(format!("{icon} "), hdr),
			Span::styled(format!("{title_s}{prev}{count}{dur}"), hdr),
		]),
		Some(InteractiveBlock::Command { index }),
	));

	// Body slice
	let (show_n, tail) = match expand {
		ExpandMode::Collapsed => (0, false),
		ExpandMode::Preview => {
			let n = total.min(TOOL_PREVIEW_LINES);
			(n, is_shell || is_diff)
		}
		ExpandMode::Full => (total.min(TOOL_FULL_LINES), false),
	};

	if show_n > 0 {
		let slice: Vec<&str> = if tail && show_n < total {
			body_lines[total - show_n..].to_vec()
		} else {
			body_lines[..show_n.min(total)].to_vec()
		};

		if is_diff {
			push_diff_body(&slice, ctx, out);
			// Claude-class review strip (one action per row for reliable hit-test)
			out.push((
				Line::from(Span::styled(
					"  ▶ [a] Accept change",
					Style::default().fg(ctx.theme.success()).add_modifier(Modifier::BOLD),
				)),
				Some(InteractiveBlock::DiffReview { index, action: 0 }),
			));
			out.push((
				Line::from(Span::styled(
					"  ▶ [r] Reject / restore file",
					Style::default().fg(ctx.theme.danger()).add_modifier(Modifier::BOLD),
				)),
				Some(InteractiveBlock::DiffReview { index, action: 1 }),
			));
			out.push((
				Line::from(Span::styled(
					"  ▶ [o] Open file in editor",
					Style::default().fg(ctx.theme.primary).add_modifier(Modifier::BOLD),
				)),
				Some(InteractiveBlock::DiffReview { index, action: 2 }),
			));
		} else if is_read {
			for (i, l) in slice.iter().enumerate() {
				out.push((render_read_line(l, i + 1, ctx), None));
			}
			// Path chip → open
			if let Some(path) = crate::msg_ui::extract_diff_path(body, preview) {
				out.push((
					Line::from(Span::styled(
						format!("  ▶ open {}", path.display()),
						Style::default().fg(ctx.theme.primary).add_modifier(Modifier::UNDERLINED),
					)),
					Some(InteractiveBlock::OpenPath { index }),
				));
			}
		} else if is_todo {
			for l in &slice {
				out.push((render_todo_line(l, ctx), None));
			}
		} else if is_web {
			render_web_card(name, title, &slice, preview, status, index, ctx, out);
		} else if is_mcp {
			render_mcp_card(name, title, &slice, preview, status, index, ctx, out);
		} else if is_lsp {
			render_lsp_card(name, title, &slice, preview, index, ctx, out);
		} else if is_shell {
			let gutter = Style::default().fg(ctx.theme.primary);
			let base = if status == PartStatus::Error {
				Style::default().fg(ctx.theme.danger())
			} else {
				Style::default().fg(ctx.theme.fg)
			};
			for l in &slice {
				let clipped = clip_cols(l, LINE_CLIP_COLS);
				if clipped.contains('\u{1b}') {
					// Live ANSI-colored terminal output
					out.push((crate::msg_ui::ansi_line(&clipped, base, gutter), None));
				} else if clipped.trim_start().starts_with('$') {
					out.push((
						Line::from(vec![
							Span::styled("  │ ", gutter),
							Span::styled(
								clipped,
								Style::default().fg(ctx.theme.success()).add_modifier(Modifier::BOLD),
							),
						]),
						None,
					));
				} else {
					out.push((
						Line::from(vec![Span::styled("  │ ", gutter), Span::styled(clipped, base)]),
						None,
					));
				}
			}
		} else {
			let gutter = Style::default().fg(ctx.theme.muted_fg);
			for l in &slice {
				let clipped = clip_cols(l, LINE_CLIP_COLS);
				let mut spans = vec![Span::styled("  │ ", gutter)];
				spans.extend(inline_md(&clipped, Style::default().fg(ctx.theme.fg)).spans);
				out.push((Line::from(spans), None));
			}
		}
	}

	// Disclosure footer (keyboard-friendly; no "Click")
	if total > 0 {
		let hidden = total.saturating_sub(show_n);
		let foot = match expand {
			ExpandMode::Full if total > TOOL_PREVIEW_LINES => Some(("  ▴ collapse".to_string(), muted)),
			ExpandMode::Collapsed => Some((
				format!("  ▸ expand · {total} lines"),
				Style::default().fg(ctx.theme.primary).add_modifier(Modifier::DIM),
			)),
			ExpandMode::Preview if hidden > 0 => Some((
				format!("  ▸ expand · +{hidden} more"),
				Style::default().fg(ctx.theme.primary).add_modifier(Modifier::DIM),
			)),
			_ => None,
		};
		if let Some((text, style)) = foot {
			out.push((Line::from(Span::styled(text, style)), Some(InteractiveBlock::Command { index })));
		}
	} else if status == PartStatus::Running {
		out.push((
			Line::from(vec![
				Span::styled("  │ ", Style::default().fg(ctx.theme.primary)),
				Span::styled(
					"running…",
					Style::default().fg(ctx.theme.muted_fg).add_modifier(Modifier::ITALIC),
				),
			]),
			None,
		));
	}

	let _ = open;
}

fn render_subagent(
	index: usize,
	name: &str,
	body: &str,
	status: PartStatus,
	ctx: &RenderCtx<'_>,
	out: &mut Vec<TaggedLine>,
) {
	let expand = resolve_subagent_expand(index, status, ctx.subagent_expand, ctx.subagents_expanded);
	let color = ctx.theme.accent;
	let hdr = Style::default().fg(color).add_modifier(Modifier::BOLD);

	// Nested tools: re-parse subagent body for command fences / text.
	let children = parse_stream_parts(body, None);
	let nested_tools = children.iter().filter(|p| matches!(p, StreamPart::Tool { .. })).count();
	let text_lines: usize = children
		.iter()
		.map(|p| match p {
			StreamPart::Text { body } | StreamPart::Thinking { body, .. } => {
				body.lines().filter(|l| !l.trim().is_empty()).count()
			}
			_ => 0,
		})
		.sum();

	let chevron = if matches!(expand, ExpandMode::Collapsed) { "▸" } else { "▾" };
	let st = match status {
		PartStatus::Running => " · running",
		PartStatus::Error => " · failed",
		PartStatus::Done => "",
	};
	let meta = if nested_tools > 0 {
		format!(" · {nested_tools} tools")
	} else if text_lines > 0 {
		format!(" · {text_lines} lines")
	} else {
		String::new()
	};
	out.push((
		Line::from(vec![
			Span::styled(format!("{chevron} "), hdr),
			Span::styled("◆ ", hdr),
			Span::styled(format!("Subagent · {name}{st}{meta}"), hdr),
		]),
		Some(InteractiveBlock::Subagent { index }),
	));

	if matches!(expand, ExpandMode::Collapsed) {
		if nested_tools + text_lines > 0 {
			out.push((
				Line::from(Span::styled(
					"  ▸ expand nested work",
					Style::default().fg(color).add_modifier(Modifier::DIM),
				)),
				Some(InteractiveBlock::Subagent { index }),
			));
		}
		return;
	}

	// Render nested tree with indent.
	let indent = "  │ ";
	let max_children = match expand {
		ExpandMode::Preview => 6usize,
		ExpandMode::Full => 40,
		ExpandMode::Collapsed => 0,
	};
	let mut shown = 0usize;
	for child in &children {
		if shown >= max_children {
			break;
		}
		match child {
			StreamPart::Tool { title, status: st, preview, body: tb, .. } => {
				let (icon, col) = match st {
					PartStatus::Running => ("●", ctx.theme.primary),
					PartStatus::Error => ("✗", ctx.theme.danger()),
					PartStatus::Done => ("✓", ctx.theme.success()),
				};
				let prev = if preview.is_empty() {
					String::new()
				} else {
					format!("  {}", preview.chars().take(48).collect::<String>())
				};
				out.push((
					Line::from(vec![
						Span::styled(indent.to_string(), Style::default().fg(color)),
						Span::styled(
							format!("{icon} {title}{prev}"),
							Style::default().fg(col).add_modifier(Modifier::BOLD),
						),
					]),
					Some(InteractiveBlock::Subagent { index }),
				));
				// Tiny nested body preview (2 lines)
				for l in tb.lines().take(2) {
					if l.trim().is_empty() {
						continue;
					}
					out.push((
						Line::from(vec![
							Span::styled("  │   ", Style::default().fg(color)),
							Span::styled(clip_cols(l, 80), Style::default().fg(ctx.theme.muted_fg)),
						]),
						None,
					));
				}
				shown += 1;
			}
			StreamPart::Text { body } => {
				for l in body.lines().take(4) {
					if l.trim().is_empty() {
						continue;
					}
					out.push((
						Line::from(vec![
							Span::styled(indent.to_string(), Style::default().fg(color)),
							Span::styled(
								clip_cols(l, 100),
								Style::default()
									.fg(blend(ctx.theme.muted_fg, color, 0.5))
									.add_modifier(Modifier::ITALIC),
							),
						]),
						None,
					));
					shown += 1;
					if shown >= max_children {
						break;
					}
				}
			}
			_ => {}
		}
	}
	if children.len() > max_children || nested_tools + text_lines > shown {
		out.push((
			Line::from(Span::styled(
				if matches!(expand, ExpandMode::Full) {
					"  ▴ collapse".to_string()
				} else {
					"  ▸ expand · more nested work".to_string()
				},
				Style::default().fg(color).add_modifier(Modifier::DIM),
			)),
			Some(InteractiveBlock::Subagent { index }),
		));
	} else if !matches!(expand, ExpandMode::Collapsed) && shown > 0 {
		out.push((
			Line::from(Span::styled(
				"  ▴ collapse",
				Style::default().fg(color).add_modifier(Modifier::DIM),
			)),
			Some(InteractiveBlock::Subagent { index }),
		));
	}

	let _ = (SUBAGENT_PREVIEW_LINES, SUBAGENT_FULL_LINES);
}

fn render_approval(
	tool: &str,
	body: &str,
	decision: &str,
	ctx: &RenderCtx<'_>,
	out: &mut Vec<TaggedLine>,
) {
	let color = ctx.theme.warning();
	let hdr = Style::default().fg(color).add_modifier(Modifier::BOLD);
	let pending = decision == "pending" || decision.is_empty();
	let title = if pending {
		format!("⚠ Approval · {tool}")
	} else {
		format!("⚠ Approval · {tool} · {decision}")
	};
	out.push((
		Line::from(vec![Span::styled("▸ ", hdr), Span::styled(title, hdr)]),
		Some(InteractiveBlock::Approval),
	));
	for l in body.lines().take(8) {
		let t = l.trim();
		if t.is_empty() || t.starts_with('[') {
			continue;
		}
		out.push((
			Line::from(vec![
				Span::styled("  │ ", Style::default().fg(color)),
				Span::styled(t.to_string(), Style::default().fg(blend(ctx.theme.fg, color, 0.35))),
			]),
			None,
		));
	}
	if pending {
		// One action per row so mouse hit-testing is reliable.
		out.push((
			Line::from(Span::styled(
				"  ▶ [y] Allow once",
				Style::default().fg(ctx.theme.success()).add_modifier(Modifier::BOLD),
			)),
			Some(InteractiveBlock::PermissionAction { action: 0 }),
		));
		out.push((
			Line::from(Span::styled(
				"  ▶ [a] Always allow this tool",
				Style::default().fg(ctx.theme.primary).add_modifier(Modifier::BOLD),
			)),
			Some(InteractiveBlock::PermissionAction { action: 1 }),
		));
		out.push((
			Line::from(Span::styled(
				"  ▶ [n] Deny",
				Style::default().fg(ctx.theme.danger()).add_modifier(Modifier::BOLD),
			)),
			Some(InteractiveBlock::PermissionAction { action: 2 }),
		));
	}
}

fn render_question(
	prompt: &str,
	options: &[String],
	answer: &str,
	ctx: &RenderCtx<'_>,
	out: &mut Vec<TaggedLine>,
) {
	let color = ctx.theme.primary;
	let hdr = Style::default().fg(color).add_modifier(Modifier::BOLD);
	out.push((Line::from(vec![Span::styled("▸ ", hdr), Span::styled("? Question", hdr)]), None));
	out.push((
		Line::from(Span::styled(format!("  {prompt}"), Style::default().fg(ctx.theme.fg))),
		None,
	));
	if !answer.is_empty() {
		out.push((
			Line::from(Span::styled(
				format!("  → {answer}"),
				Style::default().fg(ctx.theme.success()).add_modifier(Modifier::BOLD),
			)),
			None,
		));
		return;
	}
	for (i, opt) in options.iter().enumerate() {
		out.push((
			Line::from(vec![
				Span::styled(format!("  ({}) ", i + 1), Style::default().fg(ctx.theme.muted_fg)),
				Span::styled(opt.clone(), Style::default().fg(ctx.theme.fg).add_modifier(Modifier::BOLD)),
			]),
			Some(InteractiveBlock::QuestionOption { index: i }),
		));
	}
	out.push((
		Line::from(Span::styled(
			"  · ↑/↓ select · Enter confirm · click option",
			Style::default().fg(ctx.theme.muted_fg).add_modifier(Modifier::ITALIC),
		)),
		Some(InteractiveBlock::QuestionConfirm),
	));
}

fn render_compaction(label: &str, summary: &str, ctx: &RenderCtx<'_>, out: &mut Vec<TaggedLine>) {
	let style = Style::default().fg(ctx.theme.muted_fg).add_modifier(Modifier::ITALIC);
	out.push((Line::from(Span::styled(format!("  ── {label} ──"), style)), None));
	if !summary.trim().is_empty() {
		for l in summary.lines().take(6) {
			out.push((Line::from(Span::styled(format!("  {l}"), style)), None));
		}
	}
}

fn render_error(body: &str, ctx: &RenderCtx<'_>, out: &mut Vec<TaggedLine>) {
	let color = ctx.theme.danger();
	out.push((
		Line::from(Span::styled("✗ Error", Style::default().fg(color).add_modifier(Modifier::BOLD))),
		None,
	));
	for l in body.lines().take(10) {
		out.push((
			Line::from(vec![
				Span::styled("  │ ", Style::default().fg(color)),
				Span::styled(l.to_string(), Style::default().fg(blend(ctx.theme.fg, color, 0.4))),
			]),
			None,
		));
	}
}

fn render_retry(body: &str, ctx: &RenderCtx<'_>, out: &mut Vec<TaggedLine>) {
	let color = ctx.theme.warning();
	out.push((
		Line::from(Span::styled(
			"↻ Retry available — send again or re-run",
			Style::default().fg(color).add_modifier(Modifier::ITALIC),
		)),
		None,
	));
	for l in body.lines().take(3) {
		if !l.trim().is_empty() {
			out.push((
				Line::from(Span::styled(format!("  {l}"), Style::default().fg(ctx.theme.muted_fg))),
				None,
			));
		}
	}
}

fn render_context_group(label: &str, detail: &str, ctx: &RenderCtx<'_>, out: &mut Vec<TaggedLine>) {
	out.push((
		Line::from(Span::styled(
			format!("  ▸ {label}"),
			Style::default().fg(ctx.theme.muted_fg).add_modifier(Modifier::ITALIC),
		)),
		Some(InteractiveBlock::ContextGroup),
	));
	if !detail.is_empty() {
		for l in detail.lines().take(24) {
			out.push((
				Line::from(Span::styled(format!("    {l}"), Style::default().fg(ctx.theme.muted_fg))),
				None,
			));
		}
	} else {
		out.push((
			Line::from(Span::styled(
				"  ▸ expand context tools",
				Style::default().fg(ctx.theme.primary).add_modifier(Modifier::DIM),
			)),
			Some(InteractiveBlock::ContextGroup),
		));
	}
}

fn render_plan(
	title: &str,
	body: &str,
	steps: &[crate::msg_ui::PlanStep],
	ctx: &RenderCtx<'_>,
	out: &mut Vec<TaggedLine>,
) {
	let color = ctx.theme.primary;
	let hdr = Style::default().fg(color).add_modifier(Modifier::BOLD);
	let done_n = steps.iter().filter(|s| s.done).count();
	let total = steps.len();
	out.push((
		Line::from(vec![
			Span::styled("▾ ", hdr),
			Span::styled(format!("◆ {title}"), hdr),
			if total > 0 {
				Span::styled(format!(" · {done_n}/{total}"), Style::default().fg(ctx.theme.muted_fg))
			} else {
				Span::raw("")
			},
		]),
		Some(InteractiveBlock::Plan),
	));
	if !steps.is_empty() {
		for (i, s) in steps.iter().enumerate() {
			let (icon, col) =
				if s.done { ("☑", ctx.theme.success()) } else { ("☐", ctx.theme.muted_fg) };
			out.push((
				Line::from(vec![
					Span::styled(format!("  {icon} "), Style::default().fg(col).add_modifier(Modifier::BOLD)),
					Span::styled(s.text.clone(), Style::default().fg(ctx.theme.fg)),
				]),
				Some(InteractiveBlock::PlanStep { index: i }),
			));
		}
	} else {
		for l in body.lines().take(40) {
			if l.trim().is_empty() {
				continue;
			}
			out.push((
				Line::from(Span::styled(format!("  {l}"), Style::default().fg(ctx.theme.fg))),
				None,
			));
		}
	}
}

fn render_pty(
	id: &str,
	title: &str,
	lines: &[String],
	attached: bool,
	alive: bool,
	ctx: &RenderCtx<'_>,
	out: &mut Vec<TaggedLine>,
) {
	let color = ctx.theme.primary;
	let hdr = Style::default().fg(color).add_modifier(Modifier::BOLD);
	let state = if !alive {
		" · ended"
	} else if attached {
		" · ATTACHED"
	} else {
		" · live"
	};
	out.push((
		Line::from(vec![Span::styled("▾ ", hdr), Span::styled(format!("■ PTY · {title}{state}"), hdr)]),
		Some(InteractiveBlock::PtyAttach { session_id_hash: fnv1a(id) }),
	));
	let gutter = Style::default().fg(color);
	let show = lines.len().min(24);
	let start = lines.len().saturating_sub(show);
	for l in &lines[start..] {
		let clipped = clip_cols(l, LINE_CLIP_COLS);
		if clipped.contains('\u{1b}') {
			out.push((
				crate::msg_ui::ansi_line(&clipped, Style::default().fg(ctx.theme.fg), gutter),
				None,
			));
		} else {
			out.push((
				Line::from(vec![
					Span::styled("  │ ", gutter),
					Span::styled(clipped, Style::default().fg(ctx.theme.fg)),
				]),
				None,
			));
		}
	}
	if alive {
		out.push((
			Line::from(Span::styled(
				if attached {
					"  ▶ [Esc] Detach · type to send keys"
				} else {
					"  ▶ Click / Enter to attach interactive shell"
				},
				Style::default()
					.fg(if attached { ctx.theme.warning() } else { ctx.theme.primary })
					.add_modifier(Modifier::BOLD),
			)),
			Some(InteractiveBlock::PtyAttach { session_id_hash: fnv1a(id) }),
		));
		out.push((
			Line::from(Span::styled("  ▶ Kill session", Style::default().fg(ctx.theme.danger()))),
			Some(InteractiveBlock::PtyKill { session_id_hash: fnv1a(id) }),
		));
	}
}

fn fnv1a(s: &str) -> u64 {
	let mut h: u64 = 0xcbf2_9ce4_8422_2325;
	for b in s.as_bytes() {
		h ^= u64::from(*b);
		h = h.wrapping_mul(0x100_0000_01b3);
	}
	h
}

/// Pseudo-favicon: first letter of domain (TUI-safe, no network).
fn domain_badge(domain: &str) -> char {
	domain
		.trim()
		.trim_start_matches("www.")
		.chars()
		.find(|c| c.is_ascii_alphanumeric())
		.map(|c| c.to_ascii_uppercase())
		.unwrap_or('·')
}

fn render_interrupted(ctx: &RenderCtx<'_>, out: &mut Vec<TaggedLine>) {
	out.push((
		Line::from(Span::styled(
			"■ Interrupted",
			Style::default().fg(ctx.theme.warning()).add_modifier(Modifier::BOLD),
		)),
		None,
	));
}

// ── Specialized bodies ───────────────────────────────────────────────────────

fn push_diff_body(body: &[&str], ctx: &RenderCtx<'_>, out: &mut Vec<TaggedLine>) {
	let add = ctx.theme.success();
	let del = ctx.theme.danger();
	let hunk = ctx.theme.primary;
	let muted = ctx.theme.muted_fg;
	let sep = Style::default().fg(ctx.theme.border);

	// Path header from --- / +++ lines
	let mut path_a = String::new();
	let mut path_b = String::new();
	let mut adds = 0u32;
	let mut dels = 0u32;
	for l in body {
		let t = l.trim_start();
		if let Some(p) = t.strip_prefix("--- ") {
			path_a = p.trim().trim_start_matches("a/").to_string();
		} else if let Some(p) = t.strip_prefix("+++ ") {
			path_b = p.trim().trim_start_matches("b/").to_string();
		} else if t.starts_with('+') && !t.starts_with("+++") {
			adds += 1;
		} else if t.starts_with('-') && !t.starts_with("---") {
			dels += 1;
		}
	}
	let path = if !path_b.is_empty() && path_b != "/dev/null" {
		path_b
	} else if !path_a.is_empty() {
		path_a
	} else {
		String::new()
	};
	if !path.is_empty() || adds + dels > 0 {
		out.push((
			Line::from(vec![
				Span::styled("  ┌ ", sep),
				Span::styled(
					if path.is_empty() { "diff".into() } else { path },
					Style::default().fg(ctx.theme.fg).add_modifier(Modifier::BOLD),
				),
				Span::styled(format!("  +{adds} −{dels}"), Style::default().fg(muted)),
			]),
			None,
		));
	}

	let mut old_ln = 0i64;
	let mut new_ln = 0i64;
	for l in body {
		let t = l.trim_end_matches('\r');
		let trimmed = t.trim_start();
		if let Some(rest) = trimmed.strip_prefix("@@") {
			// @@ -a,b +c,d @@
			if let Some((old, neu)) = parse_hunk_header(rest) {
				old_ln = old;
				new_ln = neu;
			}
			out.push((
				Line::from(Span::styled(
					format!("  {trimmed}"),
					Style::default().fg(hunk).add_modifier(Modifier::BOLD),
				)),
				None,
			));
			continue;
		}
		if trimmed.starts_with("+++") || trimmed.starts_with("---") || trimmed.starts_with("diff ") {
			continue; // already in header
		}
		let clipped = clip_cols(t, LINE_CLIP_COLS);
		if trimmed.starts_with('+') {
			out.push((
				Line::from(vec![
					Span::styled(format!("  {new_ln:>4} "), Style::default().fg(muted)),
					Span::styled(clipped, Style::default().fg(add)),
				]),
				None,
			));
			new_ln += 1;
		} else if trimmed.starts_with('-') {
			out.push((
				Line::from(vec![
					Span::styled(format!("  {old_ln:>4} "), Style::default().fg(muted)),
					Span::styled(clipped, Style::default().fg(del)),
				]),
				None,
			));
			old_ln += 1;
		} else {
			let ln = if new_ln > 0 { new_ln } else { old_ln };
			out.push((
				Line::from(vec![
					Span::styled(format!("  {ln:>4} "), Style::default().fg(muted)),
					Span::styled(clipped, Style::default().fg(ctx.theme.fg)),
				]),
				None,
			));
			if new_ln > 0 {
				new_ln += 1;
			}
			if old_ln > 0 {
				old_ln += 1;
			}
		}
	}
}

fn parse_hunk_header(rest: &str) -> Option<(i64, i64)> {
	// rest like " -10,5 +12,7 @@"
	let mut old = None;
	let mut neu = None;
	for tok in rest.split_whitespace() {
		if let Some(s) = tok.strip_prefix('-') {
			let n = s.split(',').next()?.parse().ok()?;
			old = Some(n);
		} else if let Some(s) = tok.strip_prefix('+') {
			let n = s.split(',').next()?.parse().ok()?;
			neu = Some(n);
		}
	}
	Some((old.unwrap_or(1), neu.unwrap_or(1)))
}

fn render_read_line(line: &str, fallback_n: usize, ctx: &RenderCtx<'_>) -> Line<'static> {
	let muted = Style::default().fg(ctx.theme.muted_fg);
	let (num, code) = if let Some((n, rest)) = line.split_once('|').or_else(|| line.split_once('│'))
	{
		let n = n.trim();
		if n.chars().all(|c| c.is_ascii_digit()) {
			(n.to_string(), rest)
		} else {
			(format!("{fallback_n}"), line)
		}
	} else {
		(format!("{fallback_n}"), line)
	};
	let mut spans = vec![Span::styled(format!("  {num:>4}│ "), muted)];
	spans.extend(tint_code(code, ctx));
	Line::from(spans)
}

fn tint_code(code: &str, ctx: &RenderCtx<'_>) -> Vec<Span<'static>> {
	// Lightweight token tint (not a full highlighter — production-readable).
	let keywords = [
		"fn",
		"let",
		"mut",
		"const",
		"pub",
		"struct",
		"enum",
		"impl",
		"use",
		"mod",
		"if",
		"else",
		"match",
		"for",
		"while",
		"loop",
		"return",
		"async",
		"await",
		"self",
		"Self",
		"true",
		"false",
		"null",
		"None",
		"Some",
		"Ok",
		"Err",
		"class",
		"def",
		"import",
		"from",
		"var",
		"function",
		"const",
		"export",
		"type",
		"interface",
		"package",
	];
	let mut spans = Vec::new();
	let mut word = String::new();
	let mut other = String::new();
	let flush_other = |other: &mut String, spans: &mut Vec<Span<'static>>, ctx: &RenderCtx<'_>| {
		if !other.is_empty() {
			let is_str = other.starts_with('"') || other.starts_with('\'');
			let style = if is_str {
				Style::default().fg(blend(ctx.theme.success(), ctx.theme.fg, 0.3))
			} else if other.starts_with("//") || other.starts_with('#') {
				Style::default().fg(ctx.theme.muted_fg).add_modifier(Modifier::ITALIC)
			} else {
				Style::default().fg(ctx.theme.fg)
			};
			spans.push(Span::styled(std::mem::take(other), style));
		}
	};
	for c in code.chars() {
		if c.is_ascii_alphanumeric() || c == '_' {
			if !other.is_empty() {
				flush_other(&mut other, &mut spans, ctx);
			}
			word.push(c);
		} else {
			if !word.is_empty() {
				let color = if keywords.contains(&word.as_str()) {
					ctx.theme.danger()
				} else if word.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
					ctx.theme.primary
				} else {
					ctx.theme.fg
				};
				spans.push(Span::styled(std::mem::take(&mut word), Style::default().fg(color)));
			}
			other.push(c);
		}
	}
	if !word.is_empty() {
		let color = if keywords.contains(&word.as_str()) { ctx.theme.danger() } else { ctx.theme.fg };
		spans.push(Span::styled(word, Style::default().fg(color)));
	}
	flush_other(&mut other, &mut spans, ctx);
	if spans.is_empty() {
		spans.push(Span::raw(String::new()));
	}
	spans
}

fn render_todo_line(line: &str, ctx: &RenderCtx<'_>) -> Line<'static> {
	let t = line.trim();
	let (icon, color, text) =
		if let Some(rest) = t.strip_prefix("✓ ").or_else(|| t.strip_prefix("[x] ")) {
			("☑", ctx.theme.success(), rest)
		} else if let Some(rest) = t.strip_prefix("◐ ") {
			("◐", ctx.theme.primary, rest)
		} else if let Some(rest) = t.strip_prefix("✕ ").or_else(|| t.strip_prefix("✗ ")) {
			("☒", ctx.theme.danger(), rest)
		} else if let Some(rest) = t.strip_prefix("○ ").or_else(|| t.strip_prefix("[ ] ")) {
			("☐", ctx.theme.muted_fg, rest)
		} else {
			("•", ctx.theme.fg, t)
		};
	Line::from(vec![
		Span::styled(format!("  {icon} "), Style::default().fg(color).add_modifier(Modifier::BOLD)),
		Span::styled(text.to_string(), Style::default().fg(ctx.theme.fg)),
	])
}

fn render_web_line(line: &str, ctx: &RenderCtx<'_>) -> Line<'static> {
	let t = line.trim();
	let link = Style::default().fg(ctx.theme.primary).add_modifier(Modifier::UNDERLINED);
	if t.contains("](") && t.contains('[') {
		let mut spans = vec![Span::styled("  ↗ ", Style::default().fg(ctx.theme.primary))];
		spans.extend(inline_md(t, Style::default().fg(ctx.theme.fg)).spans);
		return Line::from(spans);
	}
	if t.starts_with("http://") || t.starts_with("https://") {
		return Line::from(vec![
			Span::styled("  ↗ ", Style::default().fg(ctx.theme.primary)),
			Span::styled(t.to_string(), link),
		]);
	}
	if t.chars().next().is_some_and(|c| c.is_ascii_digit()) && t.contains(". ") {
		return Line::from(vec![
			Span::styled("  • ", Style::default().fg(ctx.theme.primary)),
			Span::styled(t.to_string(), Style::default().fg(ctx.theme.fg).add_modifier(Modifier::BOLD)),
		]);
	}
	Line::from(vec![
		Span::styled("  │ ", Style::default().fg(ctx.theme.muted_fg)),
		Span::styled(t.to_string(), Style::default().fg(ctx.theme.muted_fg)),
	])
}

/// Claude-class web search / fetch card: citation index, domain, snippet.
#[allow(clippy::too_many_arguments)]
fn render_web_card(
	name: &str,
	_title: &str,
	slice: &[&str],
	preview: &str,
	_status: PartStatus,
	index: usize,
	ctx: &RenderCtx<'_>,
	out: &mut Vec<TaggedLine>,
) {
	let is_fetch = name.to_ascii_lowercase().contains("fetch");
	if is_fetch {
		let url = preview.trim().lines().find(|l| l.starts_with("http")).unwrap_or(preview.trim());
		out.push((
			Line::from(vec![
				Span::styled("  ┌ ", Style::default().fg(ctx.theme.border)),
				Span::styled("URL ", Style::default().fg(ctx.theme.muted_fg).add_modifier(Modifier::BOLD)),
				Span::styled(
					clip_cols(url, 80),
					Style::default().fg(ctx.theme.primary).add_modifier(Modifier::UNDERLINED),
				),
			]),
			Some(InteractiveBlock::OpenPath { index }),
		));
	}
	let mut cite_i = 0u32;
	for l in slice {
		let t = l.trim();
		if t.is_empty() {
			continue;
		}
		// Markdown link citation
		if t.contains("](") && t.contains('[') {
			cite_i += 1;
			let domain = t
				.split("](")
				.nth(1)
				.and_then(|s| s.strip_suffix(')'))
				.and_then(|u| {
					u.trim_start_matches("https://").trim_start_matches("http://").split('/').next()
				})
				.unwrap_or("");
			let badge = domain_badge(domain);
			out.push((
				Line::from(vec![
					Span::styled(
						format!("  [{cite_i}] "),
						Style::default().fg(ctx.theme.primary).add_modifier(Modifier::BOLD),
					),
					Span::styled(
						format!(" {badge} "),
						Style::default().fg(ctx.theme.bg).bg(ctx.theme.primary).add_modifier(Modifier::BOLD),
					),
					Span::styled(
						if domain.is_empty() {
							format!(" {}", clip_cols(t, 80))
						} else {
							format!(" {} · {}", domain, clip_cols(t, 60))
						},
						Style::default().fg(ctx.theme.fg),
					),
				]),
				None,
			));
			continue;
		}
		if t.starts_with("http://") || t.starts_with("https://") {
			cite_i += 1;
			let domain = t
				.trim_start_matches("https://")
				.trim_start_matches("http://")
				.split('/')
				.next()
				.unwrap_or(t);
			let badge = domain_badge(domain);
			out.push((
				Line::from(vec![
					Span::styled(
						format!("  [{cite_i}] "),
						Style::default().fg(ctx.theme.primary).add_modifier(Modifier::BOLD),
					),
					Span::styled(
						format!(" {badge} "),
						Style::default().fg(ctx.theme.bg).bg(ctx.theme.warning()).add_modifier(Modifier::BOLD),
					),
					Span::styled(
						format!(" {domain}  "),
						Style::default().fg(ctx.theme.muted_fg).add_modifier(Modifier::BOLD),
					),
					Span::styled(
						clip_cols(t, 60),
						Style::default().fg(ctx.theme.primary).add_modifier(Modifier::UNDERLINED),
					),
				]),
				None,
			));
			continue;
		}
		// Snippet / body
		out.push((
			Line::from(vec![
				Span::styled("     ", Style::default()),
				Span::styled(clip_cols(t, 96), Style::default().fg(ctx.theme.muted_fg)),
			]),
			None,
		));
	}
	if cite_i == 0 && slice.is_empty() && !preview.is_empty() {
		out.push((render_web_line(preview, ctx), None));
	}
}

#[allow(clippy::too_many_arguments)]
fn render_mcp_card(
	name: &str,
	title: &str,
	slice: &[&str],
	preview: &str,
	status: PartStatus,
	index: usize,
	ctx: &RenderCtx<'_>,
	out: &mut Vec<TaggedLine>,
) {
	let (server, tool) = name.split_once("__").unwrap_or(("mcp", name));
	out.push((
		Line::from(vec![
			Span::styled("  ┌ ", Style::default().fg(ctx.theme.border)),
			Span::styled("MCP ", Style::default().fg(ctx.theme.primary).add_modifier(Modifier::BOLD)),
			Span::styled(
				server.to_string(),
				Style::default().fg(ctx.theme.warning()).add_modifier(Modifier::BOLD),
			),
			Span::styled(" · ", Style::default().fg(ctx.theme.muted_fg)),
			Span::styled(
				tool.to_string(),
				Style::default().fg(ctx.theme.fg).add_modifier(Modifier::BOLD),
			),
		]),
		Some(InteractiveBlock::Command { index }),
	));
	if !preview.is_empty() {
		out.push((
			Line::from(vec![
				Span::styled("  │ args ", Style::default().fg(ctx.theme.muted_fg)),
				Span::styled(
					clip_cols(preview, 88),
					Style::default().fg(ctx.theme.fg).add_modifier(Modifier::ITALIC),
				),
			]),
			None,
		));
	}
	let st = match status {
		PartStatus::Running => ("running", ctx.theme.primary),
		PartStatus::Done => ("ok", ctx.theme.success()),
		PartStatus::Error => ("error", ctx.theme.danger()),
	};
	out.push((
		Line::from(vec![
			Span::styled("  │ status ", Style::default().fg(ctx.theme.muted_fg)),
			Span::styled(st.0.to_string(), Style::default().fg(st.1).add_modifier(Modifier::BOLD)),
			Span::styled(format!(" · {title}"), Style::default().fg(ctx.theme.muted_fg)),
		]),
		None,
	));
	for l in slice.iter().take(12) {
		out.push((
			Line::from(vec![
				Span::styled("  │ ", Style::default().fg(ctx.theme.border)),
				Span::styled(clip_cols(l, 96), Style::default().fg(ctx.theme.fg)),
			]),
			None,
		));
	}
}

fn render_lsp_card(
	name: &str,
	title: &str,
	slice: &[&str],
	preview: &str,
	index: usize,
	ctx: &RenderCtx<'_>,
	out: &mut Vec<TaggedLine>,
) {
	let kind = match name {
		n if n.contains("definition") => "definition",
		n if n.contains("reference") => "references",
		n if n.contains("hover") => "hover",
		n if n.contains("symbol") => "symbols",
		n if n.contains("diagnostic") => "diagnostics",
		n if n.contains("complet") => "completion",
		n if n.contains("implement") => "implementation",
		n if n.contains("hierarchy") => "call hierarchy",
		n if n.contains("format") => "format",
		_ => "lsp",
	};
	out.push((
		Line::from(vec![
			Span::styled("  ┌ ", Style::default().fg(ctx.theme.border)),
			Span::styled("LSP ", Style::default().fg(ctx.theme.primary).add_modifier(Modifier::BOLD)),
			Span::styled(
				kind.to_string(),
				Style::default().fg(ctx.theme.warning()).add_modifier(Modifier::BOLD),
			),
			Span::styled(format!(" · {title}"), Style::default().fg(ctx.theme.muted_fg)),
		]),
		Some(InteractiveBlock::Command { index }),
	));
	if !preview.is_empty() {
		out.push((
			Line::from(vec![
				Span::styled("  │ ", Style::default().fg(ctx.theme.border)),
				Span::styled("◇ ", Style::default().fg(ctx.theme.primary)),
				Span::styled(
					clip_cols(preview, 90),
					Style::default().fg(ctx.theme.fg).add_modifier(Modifier::BOLD),
				),
			]),
			None,
		));
	}
	for l in slice.iter().take(16) {
		let t = l.trim();
		// file:line:col or path(line)
		let is_loc = t.contains(".rs:")
			|| t.contains(".ts:")
			|| t.contains(".js:")
			|| t.contains(".py:")
			|| t.contains(".go:")
			|| (t.contains(':') && t.chars().any(|c| c.is_ascii_digit()));
		if is_loc {
			out.push((
				Line::from(vec![
					Span::styled("  │ ", Style::default().fg(ctx.theme.border)),
					Span::styled("→ ", Style::default().fg(ctx.theme.success())),
					Span::styled(
						clip_cols(t, 94),
						Style::default().fg(ctx.theme.primary).add_modifier(Modifier::UNDERLINED),
					),
				]),
				Some(InteractiveBlock::OpenPath { index }),
			));
		} else {
			out.push((
				Line::from(vec![
					Span::styled("  │ ", Style::default().fg(ctx.theme.border)),
					Span::styled(clip_cols(t, 96), Style::default().fg(ctx.theme.fg)),
				]),
				None,
			));
		}
	}
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn is_terminal(name: &str) -> bool {
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

fn is_diff_body(body: &str) -> bool {
	body.lines().any(|l| {
		let t = l.trim();
		t.starts_with("--- ")
			|| t.starts_with("+++ ")
			|| t.starts_with("@@ -")
			|| t.starts_with("diff --git")
	})
}

fn format_dur(d: Duration) -> String {
	let ms = d.as_millis();
	if ms < 1000 {
		format!("{ms}ms")
	} else if ms < 60_000 {
		format!("{:.1}s", d.as_secs_f32())
	} else {
		format!("{}m{:02}s", d.as_secs() / 60, d.as_secs() % 60)
	}
}

fn clip_cols(s: &str, max: usize) -> String {
	use unicode_width::UnicodeWidthChar;
	let mut out = String::new();
	let mut cols = 0usize;
	for ch in s.chars() {
		let w = UnicodeWidthChar::width(ch).unwrap_or(0).max(1);
		if cols + w > max {
			out.push('…');
			break;
		}
		out.push(ch);
		cols += w;
	}
	out
}

fn blend(a: Color, b: Color, t: f32) -> Color {
	let t = t.clamp(0.0, 1.0);
	match (a, b) {
		(Color::Rgb(ar, ag, ab), Color::Rgb(br, bg, bb)) => {
			let l = |x: u8, y: u8| -> u8 { ((x as f32) * (1.0 - t) + (y as f32) * t).round() as u8 };
			Color::Rgb(l(ar, br), l(ag, bg), l(ab, bb))
		}
		_ => b,
	}
}

fn inline_md(input: &str, base: Style) -> Line<'static> {
	// Reuse components' streaming-safe inline path via a thin reimplementation
	// to avoid circular module deps for private fns.
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
				i += 1;
			}
			continue;
		}
		if i + 1 < chars.len()
			&& ((chars[i] == '*' && chars[i + 1] == '*') || (chars[i] == '_' && chars[i + 1] == '_'))
		{
			flush(&mut buf, &mut spans, bold, italic, strike, base);
			bold = !bold;
			i += 2;
			continue;
		}
		if i + 1 < chars.len() && chars[i] == '~' && chars[i + 1] == '~' {
			flush(&mut buf, &mut spans, bold, italic, strike, base);
			strike = !strike;
			i += 2;
			continue;
		}
		if chars[i] == '*' || chars[i] == '_' {
			flush(&mut buf, &mut spans, bold, italic, strike, base);
			italic = !italic;
			i += 1;
			continue;
		}
		if chars[i] == '[' {
			// [label](url) → label underlined
			let mut j = i + 1;
			while j < chars.len() && chars[j] != ']' {
				j += 1;
			}
			if j < chars.len() {
				let label: String = chars[i + 1..j].iter().collect();
				j += 1;
				if j < chars.len() && chars[j] == '(' {
					j += 1;
					while j < chars.len() && chars[j] != ')' {
						j += 1;
					}
					if j < chars.len() {
						j += 1;
					}
				}
				flush(&mut buf, &mut spans, bold, italic, strike, base);
				spans.push(Span::styled(
					label,
					style_now(bold, italic, strike, base).add_modifier(Modifier::UNDERLINED),
				));
				i = j;
				continue;
			}
		}
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
