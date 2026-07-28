//! Branch picker + left-rail minimap for conversation forks.

#![allow(dead_code)]

use ratatui::{
	buffer::Buffer,
	layout::Rect,
	style::{Modifier, Style},
	text::{Line, Span},
	widgets::{Block, Borders, Clear, Paragraph, Widget},
};

use crate::components::{Message, MessageRole};
use crate::theme::ChatTheme;

#[derive(Debug, Clone)]
pub struct BranchInfo {
	pub id: String,
	pub label: String,
	pub message_count: usize,
	pub tip_preview: String,
}

#[derive(Debug, Clone, Default)]
pub struct BranchPickerState {
	pub open: bool,
	pub selected: usize,
}

/// Collect unique branches from messages (oldest-first order of first appearance).
pub fn list_branches(messages: &[Message], active: &str) -> Vec<BranchInfo> {
	let mut order: Vec<String> = Vec::new();
	for m in messages {
		if !order.iter().any(|b| b == &m.branch_id) {
			order.push(m.branch_id.clone());
		}
	}
	if order.is_empty() {
		order.push("main".into());
	}
	order
		.into_iter()
		.map(|id| {
			let count = messages.iter().filter(|m| m.branch_id == id).count();
			let tip = messages
				.iter()
				.rev()
				.find(|m| m.branch_id == id && m.role == MessageRole::User)
				.map(|m| m.content.chars().take(48).collect::<String>())
				.or_else(|| {
					messages
						.iter()
						.rev()
						.find(|m| m.branch_id == id)
						.map(|m| m.content.chars().take(48).collect())
				})
				.unwrap_or_default();
			let label = if id == "main" {
				"main".into()
			} else if id == active {
				format!("{id}  ← active")
			} else {
				id.clone()
			};
			let _ = active;
			BranchInfo { id, label, message_count: count, tip_preview: tip.replace('\n', " ") }
		})
		.collect()
}

/// Compact left-rail branch glyphs (2 cols) for the message list.
pub fn render_branch_rail(
	area: Rect,
	buf: &mut Buffer,
	messages: &[Message],
	active: &str,
	theme: &ChatTheme,
) {
	if area.width == 0 || area.height == 0 {
		return;
	}
	let branches = list_branches(messages, active);
	let mut y = area.y;
	for (i, b) in branches.iter().enumerate() {
		if y >= area.bottom() {
			break;
		}
		let active_b = b.id == active;
		let glyph = if active_b { "●" } else { "○" };
		let style = if active_b {
			Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)
		} else {
			Style::default().fg(theme.muted_fg)
		};
		let label = if b.id == "main" {
			format!("{glyph}m")
		} else {
			format!("{glyph}{}", (b'a' + (i as u8 % 26)) as char)
		};
		let _cell = &mut buf[(area.x, y)];
		// write first char
		for (col, ch) in label.chars().take(area.width as usize).enumerate() {
			if area.x + col as u16 >= area.right() {
				break;
			}
			let c = &mut buf[(area.x + col as u16, y)];
			c.set_char(ch);
			c.set_style(style);
		}
		y = y.saturating_add(1);
		// connector
		if y < area.bottom() && i + 1 < branches.len() {
			let c = &mut buf[(area.x, y)];
			c.set_char('│');
			c.set_style(Style::default().fg(theme.border));
			y = y.saturating_add(1);
		}
	}
}

/// Modal branch picker overlay.
pub fn render_branch_picker(
	area: Rect,
	buf: &mut Buffer,
	messages: &[Message],
	active: &str,
	picker: &BranchPickerState,
	theme: &ChatTheme,
) {
	if !picker.open {
		return;
	}
	let branches = list_branches(messages, active);
	let width = 48u16.min(area.width.saturating_sub(4)).max(24);
	let height = ((branches.len() as u16) + 4).min(area.height.saturating_sub(2)).max(6);
	let x = area.x + area.width.saturating_sub(width) / 2;
	let y = area.y + area.height.saturating_sub(height) / 2;
	let rect = Rect { x, y, width, height };
	Clear.render(rect, buf);
	let block = Block::default()
		.borders(Borders::ALL)
		.title(" Branches  ↑/↓  Enter switch  n new  Esc ")
		.border_style(Style::default().fg(theme.primary))
		.style(Style::default().bg(theme.card).fg(theme.fg));
	let inner = block.inner(rect);
	block.render(rect, buf);

	let mut lines: Vec<Line> = Vec::new();
	if branches.is_empty() {
		lines.push(Line::from(Span::styled("  (no branches)", Style::default().fg(theme.muted_fg))));
	}
	// Tree edges: main at root, forks as children
	for (i, b) in branches.iter().enumerate() {
		let sel = i == picker.selected;
		let is_main = b.id == "main";
		let tree = if is_main {
			"●"
		} else if i + 1 == branches.len() {
			"└─"
		} else {
			"├─"
		};
		let marker = if sel { "›" } else { " " };
		let active_mark = if b.id == active { " ←" } else { "" };
		let style = if sel {
			Style::default().fg(theme.bg).bg(theme.primary).add_modifier(Modifier::BOLD)
		} else if b.id == active {
			Style::default().fg(theme.primary).add_modifier(Modifier::BOLD)
		} else {
			Style::default().fg(theme.fg)
		};
		lines.push(Line::from(Span::styled(
			format!("{marker}{tree} {}{} · {} msgs", b.id, active_mark, b.message_count),
			style,
		)));
		if !b.tip_preview.is_empty() {
			let indent = if is_main { "    " } else { "    │ " };
			lines.push(Line::from(Span::styled(
				format!("{indent}{}", b.tip_preview.chars().take(40).collect::<String>()),
				if sel {
					Style::default().fg(theme.bg).bg(theme.primary)
				} else {
					Style::default().fg(theme.muted_fg)
				},
			)));
		}
	}
	Paragraph::new(lines).style(Style::default().bg(theme.card)).render(inner, buf);
}
