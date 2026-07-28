// Menu rendering logic
use super::menu_data::Menu;
use ratatui::{
	buffer::Buffer,
	layout::Rect,
	style::{Modifier, Style},
	text::{Line, Span, Text},
	widgets::{Block, Widget},
};

fn char_count(value: &str) -> usize {
	value.chars().count()
}

fn truncate_with_ellipsis(value: &str, max_width: usize) -> String {
	if char_count(value) <= max_width {
		return value.to_string();
	}

	let visible = max_width.saturating_sub(3);
	let prefix: String = value.chars().take(visible).collect();
	format!("{prefix}...")
}

impl Menu {
	pub fn render_in_area(
		&mut self,
		area: Rect,
		buf: &mut Buffer,
		_theme_mode: &crate::theme::ThemeVariant,
	) {
		// Create a centered content area
		let content_width = (area.width * 7 / 10).min(80);
		let content_height = (area.height * 75 / 100).min(32);

		let x_offset = (area.width - content_width) / 2;
		let y_offset = (area.height - content_height) / 2;

		let content_area = Rect {
			x: area.x + x_offset,
			y: area.y + y_offset,
			width: content_width,
			height: content_height,
		};

		self.menu_area = content_area;

		// Determine menu title
		let menu_title = if let Some(ref custom) = self.custom_title {
			format!("{} ({} items)", custom, self.menu_items.len())
		} else if let Some(submenu_idx) = self.current_submenu {
			let parent_name = self
				.main_menu
				.get(submenu_idx)
				.map(|(t, _)| {
					t.trim_start_matches(|c: char| c.is_numeric() || c == '.' || c.is_whitespace())
				})
				.unwrap_or("Menu");
			let item_count =
				self.menu_items.len().saturating_sub(if self.opened_directly { 0 } else { 1 });

			format!("{} ({} items)", parent_name, item_count)
		} else {
			let item_count = self.menu_items.len();
			format!("Command Palette ({} items)", item_count)
		};

		Block::default()
			.borders(ratatui::widgets::Borders::ALL)
			.border_style(Style::default().fg(self.theme.border))
			.border_type(ratatui::widgets::BorderType::Rounded)
			.title(Span::styled(
				format!(" {} ", menu_title),
				Style::default().fg(self.theme.accent).add_modifier(Modifier::BOLD),
			))
			.render(content_area, buf);

		let padded_area = Rect {
			x: content_area.x + 2,
			y: content_area.y + 1,
			width: content_area.width.saturating_sub(4),
			height: content_area.height.saturating_sub(2),
		};

		let text_fg = self.theme.fg;
		let selected_bg = self.theme.accent;
		let selected_fg = self.theme.bg;
		let hover_bg = self.theme.border;

		let visible_items = padded_area.height as usize;

		if self.selected_menu_item < self.scroll_offset {
			self.scroll_offset = self.selected_menu_item;
		} else if self.selected_menu_item >= self.scroll_offset + visible_items {
			self.scroll_offset = self.selected_menu_item - visible_items + 1;
		}

		let mut lines = Vec::new();
		let end_idx = (self.scroll_offset + visible_items).min(self.menu_items.len());
		let exact_width = padded_area.width as usize;

		self.menu_item_areas.clear();

		for (idx, (title, description)) in
			self.menu_items[self.scroll_offset..end_idx].iter().enumerate()
		{
			let idx = self.scroll_offset + idx;
			let is_selected = idx == self.selected_menu_item;
			let is_hovered = self.hovered_menu_item == Some(idx);
			let current_y = padded_area.y + (idx - self.scroll_offset) as u16;

			self.menu_item_areas.push(Rect {
				x: padded_area.x,
				y: current_y,
				width: padded_area.width,
				height: 1,
			});

			// Format the line — Themes style: left title, optional short right label.
			// Dynamic models/channels use payload "id||tag" (tag is the right label only).
			let item_text = if description == "TOGGLE_MODE" {
				format!("{} (Dark)", title)
			} else if description == "TOGGLE_RECORDING" {
				let mode_indicator = if self.recording_mode { "(Recording)" } else { "(Viewing)" };
				format!("{} {}", title, mode_indicator)
			} else if let Some((_id, tag)) = description.split_once("||") {
				let left_part = title.as_str();
				let right_part = tag.trim();
				if right_part.is_empty() {
					left_part.to_string()
				} else {
					let available_width = exact_width.saturating_sub(char_count(right_part) + 3);
					if char_count(left_part) > available_width {
						format!("{}  {}", truncate_with_ellipsis(left_part, available_width), right_part)
					} else {
						let padding = available_width.saturating_sub(char_count(left_part));
						format!("{}{}  {}", left_part, " ".repeat(padding), right_part)
					}
				}
			} else if !description.is_empty()
				&& !description.starts_with("__")
				&& description != "TOGGLE_MODE"
				&& description != "TOGGLE_RECORDING"
				&& !description.starts_with("model:")
			{
				// Theme keys etc. are stored as second field — show title only for clean rows
				// unless it looks like a short UI label (no path-like ids).
				if description.len() <= 18 && !description.contains('/') && !description.contains('-') {
					let left_part = title.as_str();
					let right_part = description.as_str();
					let available_width = exact_width.saturating_sub(char_count(right_part) + 3);
					if char_count(left_part) > available_width {
						format!("{}  {}", truncate_with_ellipsis(left_part, available_width), right_part)
					} else {
						let padding = available_width.saturating_sub(char_count(left_part));
						format!("{}{}  {}", left_part, " ".repeat(padding), right_part)
					}
				} else {
					// Long id payloads (theme keys): show title only — full-width row
					title.to_string()
				}
			} else {
				title.to_string()
			};

			let line_text = if char_count(&item_text) > exact_width {
				truncate_with_ellipsis(&item_text, exact_width)
			} else {
				item_text
			};

			let (fg, bg) = if is_selected {
				(selected_fg, selected_bg)
			} else if is_hovered {
				(self.theme.bg, hover_bg)
			} else {
				(text_fg, self.theme.bg)
			};

			let style = if is_selected {
				Style::default().fg(fg).bg(bg).add_modifier(Modifier::BOLD)
			} else {
				Style::default().fg(fg).bg(bg)
			};

			let padded_line = format!(" {:<width$}", line_text, width = exact_width.saturating_sub(1));
			lines.push(Line::from(Span::styled(padded_line, style)));
		}

		// Fill remaining lines
		let items_shown = end_idx - self.scroll_offset;
		for _ in items_shown..visible_items {
			let empty_line = " ".repeat(exact_width);
			lines.push(Line::from(Span::styled(empty_line, Style::default().bg(self.theme.bg))));
		}

		let main_text = Text::from(lines);
		ratatui::widgets::Paragraph::new(main_text).render(padded_area, buf);
	}
}

#[cfg(test)]
mod tests {
	use super::truncate_with_ellipsis;

	#[test]
	fn truncation_preserves_utf8_boundaries() {
		assert_eq!(truncate_with_ellipsis("αβγδε", 4), "α...");
		assert_eq!(truncate_with_ellipsis("αβγ", 5), "αβγ");
	}
}
