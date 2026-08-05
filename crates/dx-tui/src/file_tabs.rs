use std::path::PathBuf;

use ratatui::{
	buffer::Buffer,
	layout::Rect,
	style::{Color, Modifier, Style},
	text::{Line, Span},
	widgets::Widget,
};

#[derive(Debug, Clone)]
pub struct FileTab {
	pub path: PathBuf,
	pub name: String,
	pub language: String,
	pub modified: bool,
	pub scroll_position: usize,
	pub content_preview: Option<String>,
}

impl FileTab {
	pub fn new(path: PathBuf) -> Self {
		let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("untitled").to_string();
		let language = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_string();
		Self { path, name, language, modified: false, scroll_position: 0, content_preview: None }
	}

	pub fn label(&self) -> String {
		if self.modified { format!("* {}", self.name) } else { self.name.clone() }
	}
}

#[derive(Debug, Clone)]
pub struct FileTabBar {
	pub tabs: Vec<FileTab>,
	pub active_index: usize,
	theme_bg: Color,
	theme_fg: Color,
	theme_accent: Color,
	theme_border: Color,
}

impl FileTabBar {
	pub fn new(theme_bg: Color, theme_fg: Color, theme_accent: Color, theme_border: Color) -> Self {
		Self { tabs: Vec::new(), active_index: 0, theme_bg, theme_fg, theme_accent, theme_border }
	}

	pub fn update_theme(&mut self, bg: Color, fg: Color, accent: Color, border: Color) {
		self.theme_bg = bg;
		self.theme_fg = fg;
		self.theme_accent = accent;
		self.theme_border = border;
	}

	pub fn open_tab(&mut self, path: PathBuf) -> usize {
		if let Some(pos) = self.tabs.iter().position(|t| t.path == path) {
			self.active_index = pos;
			return pos;
		}
		let tab = FileTab::new(path);
		self.tabs.push(tab);
		self.active_index = self.tabs.len() - 1;
		self.active_index
	}

	pub fn close_tab(&mut self, index: usize) -> bool {
		if index >= self.tabs.len() {
			return false;
		}
		self.tabs.remove(index);
		if self.tabs.is_empty() {
			self.active_index = 0;
			return true;
		}
		if self.active_index >= index && self.active_index > 0 {
			self.active_index -= 1;
		}
		if self.active_index >= self.tabs.len() {
			self.active_index = self.tabs.len() - 1;
		}
		true
	}

	pub fn close_active_tab(&mut self) -> bool {
		self.close_tab(self.active_index)
	}

	pub fn activate_next(&mut self) {
		if self.tabs.len() <= 1 {
			return;
		}
		self.active_index = (self.active_index + 1) % self.tabs.len();
	}

	pub fn activate_prev(&mut self) {
		if self.tabs.is_empty() {
			return;
		}
		self.active_index =
			if self.active_index == 0 { self.tabs.len() - 1 } else { self.active_index - 1 };
	}

	pub fn activate_index(&mut self, index: usize) {
		if index < self.tabs.len() {
			self.active_index = index;
		}
	}

	pub fn active_tab(&self) -> Option<&FileTab> {
		self.tabs.get(self.active_index)
	}

	pub fn active_tab_mut(&mut self) -> Option<&mut FileTab> {
		self.tabs.get_mut(self.active_index)
	}

	pub fn mark_active_modified(&mut self, modified: bool) {
		if let Some(tab) = self.tabs.get_mut(self.active_index) {
			tab.modified = modified;
		}
	}

	pub fn is_empty(&self) -> bool {
		self.tabs.is_empty()
	}

	pub fn len(&self) -> usize {
		self.tabs.len()
	}
}

impl Widget for &FileTabBar {
	fn render(self, area: Rect, buf: &mut Buffer) {
		if area.width < 4 || area.height == 0 {
			return;
		}

		let mut x = area.x;
		let max_x = area.right().saturating_sub(4);

		for (i, tab) in self.tabs.iter().enumerate() {
			if x >= max_x {
				render_overflow_indicator(x, area.y, buf);
				break;
			}

			let is_active = i == self.active_index;
			let label = tab.label();
			let max_w = (max_x.saturating_sub(x) as usize).min(24);
			let display: String = label.chars().take(max_w.saturating_sub(1)).collect();
			let tab_w = (display.chars().count() as u16 + 2)
				.min(area.width.saturating_sub(x.saturating_sub(area.x)));

			if tab_w < 4 {
				break;
			}

			for cx in x..x.saturating_add(tab_w) {
				let cell = &mut buf[(cx, area.y)];
				if is_active {
					cell.set_bg(self.theme_accent);
					cell.set_fg(self.theme_bg);
				} else {
					cell.set_bg(self.theme_border);
					cell.set_fg(self.theme_fg);
				}
			}

			let style = if is_active {
				Style::default().bg(self.theme_accent).fg(self.theme_bg).add_modifier(Modifier::BOLD)
			} else {
				Style::default().bg(self.theme_border).fg(self.theme_fg)
			};

			let text = format!(" {} ", display);

			Line::from(Span::styled(text, style))
				.render(Rect { x, y: area.y, width: tab_w, height: 1 }, buf);

			x = x.saturating_add(tab_w);
		}
	}
}

fn render_overflow_indicator(x: u16, y: u16, buf: &mut Buffer) {
	buf[(x, y)].set_char('…');
	buf[(x, y)].set_style(Style::default().add_modifier(Modifier::DIM));
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::path::PathBuf;

	#[test]
	fn test_open_tab_creates_new() {
		let mut bar = FileTabBar::new(Color::Black, Color::White, Color::Blue, Color::Gray);
		let idx = bar.open_tab(PathBuf::from("test.rs"));
		assert_eq!(idx, 0);
		assert_eq!(bar.tabs.len(), 1);
		assert_eq!(bar.tabs[0].name, "test.rs");
	}

	#[test]
	fn test_open_tab_reuses_existing() {
		let mut bar = FileTabBar::new(Color::Black, Color::White, Color::Blue, Color::Gray);
		bar.open_tab(PathBuf::from("a.rs"));
		let idx = bar.open_tab(PathBuf::from("a.rs"));
		assert_eq!(idx, 0);
		assert_eq!(bar.tabs.len(), 1);
	}

	#[test]
	fn test_close_tab_removes_and_adjusts_active() {
		let mut bar = FileTabBar::new(Color::Black, Color::White, Color::Blue, Color::Gray);
		bar.open_tab(PathBuf::from("a.rs"));
		bar.open_tab(PathBuf::from("b.rs"));
		assert_eq!(bar.active_index, 1);
		bar.close_tab(1);
		assert_eq!(bar.tabs.len(), 1);
		assert_eq!(bar.active_index, 0);
	}

	#[test]
	fn test_close_active_tab() {
		let mut bar = FileTabBar::new(Color::Black, Color::White, Color::Blue, Color::Gray);
		bar.open_tab(PathBuf::from("a.rs"));
		assert!(bar.close_active_tab());
		assert!(bar.tabs.is_empty());
	}

	#[test]
	fn test_activate_next_cycles() {
		let mut bar = FileTabBar::new(Color::Black, Color::White, Color::Blue, Color::Gray);
		bar.open_tab(PathBuf::from("a.rs"));
		bar.open_tab(PathBuf::from("b.rs"));
		assert_eq!(bar.active_index, 1);
		bar.activate_next();
		assert_eq!(bar.active_index, 0);
	}

	#[test]
	fn test_activate_index() {
		let mut bar = FileTabBar::new(Color::Black, Color::White, Color::Blue, Color::Gray);
		bar.open_tab(PathBuf::from("a.rs"));
		bar.open_tab(PathBuf::from("b.rs"));
		bar.activate_index(0);
		assert_eq!(bar.active_index, 0);
	}

	#[test]
	fn test_mark_modified() {
		let mut bar = FileTabBar::new(Color::Black, Color::White, Color::Blue, Color::Gray);
		bar.open_tab(PathBuf::from("a.rs"));
		bar.mark_active_modified(true);
		assert!(bar.tabs[0].modified);
	}

	#[test]
	fn test_label_shows_modified_indicator() {
		let mut tab = FileTab::new(PathBuf::from("test.rs"));
		assert_eq!(tab.label(), "test.rs");
		tab.modified = true;
		assert_eq!(tab.label(), "* test.rs");
	}

	#[test]
	fn test_is_empty() {
		let bar = FileTabBar::new(Color::Black, Color::White, Color::Blue, Color::Gray);
		assert!(bar.is_empty());
	}
}
