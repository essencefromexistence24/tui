use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Multi-line paste stored separately so the input shows a compact chip.
#[derive(Debug, Clone)]
pub struct PasteBlock {
	pub id: u32,
	pub content: String,
	pub lines: usize,
	pub chars: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachmentKind {
	File,
	Folder,
	Image,
}

#[derive(Debug, Clone)]
pub struct Attachment {
	pub path: PathBuf,
	pub kind: AttachmentKind,
}

impl Attachment {
	pub fn label(&self) -> String {
		let name = self
			.path
			.file_name()
			.and_then(|n| n.to_str())
			.unwrap_or_else(|| self.path.to_str().unwrap_or("?"));
		match self.kind {
			AttachmentKind::File => format!("📄 {name}"),
			AttachmentKind::Folder => format!("📁 {name}"),
			AttachmentKind::Image => format!("🖼 {name}"),
		}
	}

	pub fn classify(path: &Path) -> Option<Self> {
		if !path.exists() {
			return None;
		}
		let kind = if path.is_dir() {
			AttachmentKind::Folder
		} else if is_image_path(path) {
			AttachmentKind::Image
		} else if path.is_file() {
			AttachmentKind::File
		} else {
			return None;
		};
		Some(Self { path: path.to_path_buf(), kind })
	}
}

fn is_image_path(path: &Path) -> bool {
	path
		.extension()
		.and_then(|e| e.to_str())
		.map(|ext| {
			matches!(
				ext.to_ascii_lowercase().as_str(),
				"png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg" | "ico" | "tiff" | "tif"
			)
		})
		.unwrap_or(false)
}

/// Lines of pasted text before collapsing into a compact chip.
const PASTE_COLLAPSE_LINES: usize = 2;
/// Chars / “words” threshold — longer pastes collapse to `[pasted N lines]`.
const PASTE_COLLAPSE_CHARS: usize = 80;
const MAX_HISTORY: usize = 100;
/// Max visible content rows inside the input box (excluding borders).
const MAX_INPUT_CONTENT_ROWS: u16 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuggestKind {
	Slash,
	Mention,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuggestItem {
	pub value: String,
	pub label: String,
	pub description: String,
}

/// Slash command autocomplete — sourced from the OpenCode-compatible catalog.
fn slash_autocomplete_pairs() -> Vec<(&'static str, &'static str)> {
	crate::slash_commands::autocomplete_pairs()
}

/// @-mention context sources.
const AT_MENTIONS: &[(&str, &str)] = &[
	("@file", "Reference a file"),
	("@folder", "Reference a directory"),
	("@codebase", "Whole project context"),
	("@git", "Git status / recent commits"),
	("@diff", "Unstaged / staged diff"),
	("@web", "Web search context"),
	("@docs", "Project documentation"),
	("@terminal", "Last terminal output"),
	("@clipboard", "Clipboard contents"),
	("@selection", "Current editor selection"),
	("@agent", "Mention an agent"),
	("@model", "Mention a model"),
	("@image", "Attach image context"),
	("@url", "Fetch a URL"),
	("@symbol", "Code symbol / definition"),
	("@tests", "Related tests"),
	("@errors", "Recent diagnostics"),
	("@pr", "Pull request context"),
];

#[derive(Debug, Clone)]
pub struct InputState {
	pub content: String,
	pub cursor_position: usize,
	pub scroll_offset: usize,
	/// Vertical line scroll for multi-line input viewport.
	pub vertical_scroll: usize,
	pub selection_start: Option<usize>,
	pub selection_end: Option<usize>,
	/// Collapsed multi-line pastes referenced by markers in `content`.
	pub paste_blocks: Vec<PasteBlock>,
	next_paste_id: u32,
	/// Prompt history (oldest → newest).
	pub history: Vec<String>,
	/// Index into history while browsing; `None` when editing a fresh draft.
	pub history_index: Option<usize>,
	/// Draft saved when entering history with Up.
	draft_before_history: String,
	/// Attached files / folders / images sent with the next prompt.
	pub attachments: Vec<Attachment>,
	/// Active @ or / suggestion list (rendered above the input).
	pub suggestions: Vec<SuggestItem>,
	pub suggestion_index: usize,
	pub suggestion_kind: Option<SuggestKind>,
	/// Byte range of the token being completed `[start, end)`.
	suggest_token_start: usize,
	/// True while the user is dragging to select text with the mouse.
	pub mouse_selecting: bool,
}

impl InputState {
	pub fn new() -> Self {
		Self {
			content: String::new(),
			cursor_position: 0,
			scroll_offset: 0,
			vertical_scroll: 0,
			selection_start: None,
			selection_end: None,
			paste_blocks: Vec::new(),
			next_paste_id: 1,
			history: Vec::new(),
			history_index: None,
			draft_before_history: String::new(),
			attachments: Vec::new(),
			suggestions: Vec::new(),
			suggestion_index: 0,
			suggestion_kind: None,
			suggest_token_start: 0,
			mouse_selecting: false,
		}
	}

	/// Total widget height in rows including borders (1 content line by default).
	pub fn preferred_height(&self) -> u16 {
		let att: u16 = if self.attachments.is_empty() { 0 } else { 1 };
		let lines = self.line_count_display().max(1) as u16;
		// Stay single-line until content actually has multiple lines
		let content_rows = if lines <= 1 { 1 } else { lines.min(MAX_INPUT_CONTENT_ROWS) };
		// top border + content + bottom border (+ attachment chip row)
		2 + att + content_rows
	}

	pub fn suggestion_bar_height(&self) -> u16 {
		if self.suggestions.is_empty() {
			0
		} else {
			// Compact list: up to 8 rows, full width (no extra header row)
			(self.suggestions.len() as u16).min(8)
		}
	}

	pub fn clear_suggestions(&mut self) {
		self.suggestions.clear();
		self.suggestion_index = 0;
		self.suggestion_kind = None;
		self.suggest_token_start = 0;
	}

	pub fn has_suggestions(&self) -> bool {
		!self.suggestions.is_empty()
	}

	/// Recompute @ / / suggestions from text under the cursor.
	pub fn refresh_suggestions(&mut self) {
		let Some((kind, start, token)) = self.token_at_cursor() else {
			self.clear_suggestions();
			return;
		};
		self.suggest_token_start = start;
		self.suggestion_kind = Some(kind);
		let query = token.to_lowercase();
		let items = match kind {
			SuggestKind::Slash => {
				let pairs = slash_autocomplete_pairs();
				filter_pairs(&pairs, &query)
			}
			SuggestKind::Mention => {
				let mut items = filter_pairs(AT_MENTIONS, &query);
				// Also surface files in cwd matching the suffix after @
				let suffix = token.trim_start_matches('@');
				if !suffix.is_empty() {
					items.extend(cwd_file_mentions(suffix));
				}
				items
			}
		};
		self.suggestions = items;
		if self.suggestion_index >= self.suggestions.len() {
			self.suggestion_index = 0;
		}
		if self.suggestions.is_empty() {
			self.clear_suggestions();
		}
	}

	fn token_at_cursor(&self) -> Option<(SuggestKind, usize, String)> {
		let pos = self.cursor_position.min(self.content.len());
		let before = &self.content[..pos];
		// Token starts after whitespace or at line start
		let start = before.rfind(|c: char| c.is_whitespace()).map(|i| i + 1).unwrap_or(0);
		// `/` and `@` only open menus when they are the first character of the
		// first line (byte offset 0). Mid-text or later lines are plain input.
		if start != 0 {
			return None;
		}
		let token = &before[start..];
		if token.starts_with('/') {
			Some((SuggestKind::Slash, start, token.to_string()))
		} else if token.starts_with('@') {
			Some((SuggestKind::Mention, start, token.to_string()))
		} else {
			None
		}
	}

	fn accept_suggestion(&mut self) -> InputAction {
		if self.suggestions.is_empty() {
			return InputAction::None;
		}
		let item = self.suggestions[self.suggestion_index.min(self.suggestions.len() - 1)].clone();
		let start = self.suggest_token_start.min(self.content.len());
		let end = self.cursor_position.min(self.content.len());

		// Single-word slash command → submit immediately, no trailing space.
		let is_single_slash =
			self.suggestion_kind == Some(SuggestKind::Slash) && !item.value.contains(' ');
		if is_single_slash {
			self.content.replace_range(start..end, &item.value);
			self.cursor_position = start + item.value.len();
			self.clear_suggestions();
			self.clear_selection();
			let msg = std::mem::take(&mut self.content);
			self.scroll_offset = 0;
			self.vertical_scroll = 0;
			return InputAction::Submit(msg);
		}

		if start <= end {
			self.content.replace_range(start..end, &item.value);
			// trailing space for continued typing
			let insert_at = start + item.value.len();
			if !item.value.ends_with(' ') {
				self.content.insert(insert_at, ' ');
				self.cursor_position = insert_at + 1;
			} else {
				self.cursor_position = insert_at;
			}
		}
		self.clear_suggestions();
		self.clear_selection();
		InputAction::Changed
	}

	fn suggestion_move(&mut self, delta: i32) -> InputAction {
		if self.suggestions.is_empty() {
			return InputAction::None;
		}
		let len = self.suggestions.len() as i32;
		let next = (self.suggestion_index as i32 + delta).rem_euclid(len) as usize;
		self.suggestion_index = next;
		InputAction::None
	}

	pub fn has_selection(&self) -> bool {
		match (self.selection_start, self.selection_end) {
			(Some(a), Some(b)) => a != b,
			_ => false,
		}
	}

	pub fn clear_selection(&mut self) {
		self.selection_start = None;
		self.selection_end = None;
	}

	pub fn replace_content(&mut self, content: impl Into<String>) {
		self.content = content.into();
		self.cursor_position = self.content.len();
		self.scroll_offset = 0;
		self.vertical_scroll = 0;
		self.clear_selection();
		self.history_index = None;
	}

	pub fn select_all(&mut self) {
		if !self.content.is_empty() {
			self.selection_start = Some(0);
			self.selection_end = Some(self.content.len());
			self.cursor_position = self.content.len();
		}
	}

	pub fn get_selected_text(&self) -> Option<String> {
		if let (Some(start), Some(end)) = (self.selection_start, self.selection_end) {
			let (start, end) = if start < end { (start, end) } else { (end, start) };
			Some(self.content[start..end].to_string())
		} else {
			None
		}
	}

	pub fn delete_selection(&mut self) {
		if let (Some(start), Some(end)) = (self.selection_start, self.selection_end) {
			let (start, end) = if start < end { (start, end) } else { (end, start) };
			self.content.drain(start..end);
			self.cursor_position = start;
			self.clear_selection();
			self.gc_paste_blocks();
		}
	}

	/// Compact display string: paste markers → `[pasted N lines]` chips.
	pub fn display_content(&self) -> String {
		let mut out = self.content.clone();
		for block in &self.paste_blocks {
			let marker = paste_marker(block.id, block.lines, block.chars);
			let chip = format!("[pasted {} lines]", block.lines);
			out = out.replace(&marker, &chip);
		}
		out
	}

	/// Expand paste markers + attachments into the final message body.
	pub fn compose_submit_message(&self) -> String {
		let mut body = self.content.clone();
		for block in &self.paste_blocks {
			let marker = paste_marker(block.id, block.lines, block.chars);
			body = body.replace(&marker, &block.content);
		}

		if self.attachments.is_empty() {
			return body;
		}

		let mut prefix = String::new();
		for att in &self.attachments {
			let kind = match att.kind {
				AttachmentKind::File => "file",
				AttachmentKind::Folder => "folder",
				AttachmentKind::Image => "image",
			};
			prefix.push_str(&format!("[{kind}: {}]\n", att.path.display()));
		}
		if body.is_empty() { prefix.trim_end().to_string() } else { format!("{prefix}\n{body}") }
	}

	pub fn line_count_display(&self) -> usize {
		let d = self.display_content();
		if d.is_empty() {
			return 1;
		}
		// `str::lines()` drops a trailing empty line after `\n`, so Shift+Enter
		// would look like a no-op. Count newlines + 1 instead.
		d.chars().filter(|&c| c == '\n').count() + 1
	}

	/// Insert a newline at the cursor (Shift/Ctrl/Alt+Enter).
	pub fn insert_newline(&mut self) {
		if self.has_selection() {
			self.delete_selection();
		}
		self.content.insert(self.cursor_position, '\n');
		self.cursor_position += 1;
		self.history_index = None;
		self.clear_suggestions();
		// Keep the new empty line in view (max 5 visible content rows)
		let lines = self.line_count_display();
		let max_vis = MAX_INPUT_CONTENT_ROWS as usize;
		if lines > max_vis {
			self.vertical_scroll = lines.saturating_sub(max_vis);
		}
	}

	/// Byte ranges of the selection mapped onto `display_content` (best-effort).
	pub fn selection_display_range(&self) -> Option<(usize, usize)> {
		let (start, end) = match (self.selection_start, self.selection_end) {
			(Some(a), Some(b)) if a != b => {
				if a < b {
					(a, b)
				} else {
					(b, a)
				}
			}
			_ => return None,
		};
		// Display content length can differ from raw when chips replace markers;
		// clamp to display length for rendering.
		let display = self.display_content();
		let max = display.len();
		Some((start.min(max), end.min(max)))
	}

	pub fn paste_text(&mut self, text: &str) -> InputAction {
		if text.is_empty() {
			return InputAction::None;
		}

		// Pure newline paste (some terminals send Shift+Enter this way)
		if text == "\n" || text == "\r\n" || text == "\r" {
			self.insert_newline();
			return InputAction::Changed;
		}

		// Single existing path → attach instead of dumping path text
		let trimmed = text.trim();
		if !trimmed.contains('\n')
			&& let Ok(path) = shellexpand_path(trimmed)
			&& let Some(att) = Attachment::classify(&path)
		{
			return self.attach(att);
		}

		if self.has_selection() {
			self.delete_selection();
		}

		// Count real lines (including trailing newline as a new line)
		let lines = if text.ends_with('\n') {
			text.chars().filter(|&c| c == '\n').count().max(1)
		} else {
			text.chars().filter(|&c| c == '\n').count() + 1
		};
		let chars = text.chars().count();
		// Short: paste inline. Long / multi-line: collapse to [pasted N lines]
		let collapse = lines >= PASTE_COLLAPSE_LINES || chars >= PASTE_COLLAPSE_CHARS;

		if collapse {
			let id = self.next_paste_id;
			self.next_paste_id = self.next_paste_id.saturating_add(1);
			let marker = paste_marker(id, lines, chars);
			self.paste_blocks.push(PasteBlock { id, content: text.to_string(), lines, chars });
			self.insert_str(&marker);
		} else {
			self.insert_str(text);
		}
		self.refresh_suggestions();
		InputAction::Pasted { lines, chars }
	}

	pub fn attach(&mut self, att: Attachment) -> InputAction {
		let name = att.label();
		// Dedupe by path
		if self.attachments.iter().any(|a| a.path == att.path) {
			return InputAction::None;
		}
		self.attachments.push(att);
		InputAction::Attached { name }
	}

	pub fn try_attach_path(&mut self, raw: &str) -> InputAction {
		let Ok(path) = shellexpand_path(raw.trim()) else {
			return InputAction::None;
		};
		match Attachment::classify(&path) {
			Some(att) => self.attach(att),
			None => InputAction::None,
		}
	}

	pub fn remove_last_attachment(&mut self) -> bool {
		self.attachments.pop().is_some()
	}

	/// Copy only the current selection (never the whole buffer).
	/// Clears selection after a successful copy so the next Ctrl+C is not a repeat.
	pub fn copy_selection(&mut self) -> InputAction {
		let Some(text) = self.get_selected_text() else {
			return InputAction::None;
		};
		if text.is_empty() {
			self.clear_selection();
			return InputAction::None;
		}
		match cli_clipboard::set_contents(text.clone()) {
			Ok(()) => {
				// Keep selection visual clear so "not selecting" is honest state
				self.clear_selection();
				self.mouse_selecting = false;
				InputAction::Copied { chars: text.chars().count() }
			}
			Err(_) => InputAction::None,
		}
	}

	pub fn cut_selection(&mut self) -> InputAction {
		let Some(text) = self.get_selected_text() else {
			return InputAction::None;
		};
		if cli_clipboard::set_contents(text.clone()).is_err() {
			return InputAction::None;
		}
		self.delete_selection();
		self.mouse_selecting = false;
		InputAction::Copied { chars: text.chars().count() }
	}

	pub fn history_prev(&mut self) -> InputAction {
		if self.history.is_empty() {
			return InputAction::None;
		}
		match self.history_index {
			None => {
				self.draft_before_history = std::mem::take(&mut self.content);
				let idx = self.history.len() - 1;
				self.history_index = Some(idx);
				self.apply_history_entry(idx);
				InputAction::PreviousHistory
			}
			Some(0) => InputAction::None,
			Some(i) => {
				let idx = i - 1;
				self.history_index = Some(idx);
				self.apply_history_entry(idx);
				InputAction::PreviousHistory
			}
		}
	}

	pub fn history_next(&mut self) -> InputAction {
		let Some(i) = self.history_index else {
			return InputAction::None;
		};
		if i + 1 >= self.history.len() {
			self.history_index = None;
			self.content = std::mem::take(&mut self.draft_before_history);
			self.cursor_position = self.content.len();
			self.clear_selection();
			self.vertical_scroll = 0;
			self.clear_suggestions();
			return InputAction::NextHistory;
		}
		let idx = i + 1;
		self.history_index = Some(idx);
		self.apply_history_entry(idx);
		InputAction::NextHistory
	}

	#[inline]
	fn apply_history_entry(&mut self, idx: usize) {
		// Swap-assign avoids an extra allocation when possible
		self.content.clone_from(&self.history[idx]);
		self.cursor_position = self.content.len();
		self.clear_selection();
		self.vertical_scroll = 0;
		self.clear_suggestions();
		self.mouse_selecting = false;
	}

	/// Map a click inside the text area (relative col/row) to a byte index in `content`.
	pub fn index_from_click(&self, col: u16, row: u16) -> usize {
		let lines: Vec<&str> =
			if self.content.is_empty() { vec![""] } else { self.content.split('\n').collect() };
		let line_idx = (self.vertical_scroll + row as usize).min(lines.len().saturating_sub(1));
		let mut byte = 0usize;
		for (i, line) in lines.iter().enumerate() {
			if i == line_idx {
				// Advance by `col` chars (clamped)
				let mut c = 0u16;
				for (offset, _ch) in line.char_indices() {
					if c >= col {
						return byte + offset;
					}
					c = c.saturating_add(1);
				}
				return byte + line.len();
			}
			byte += line.len() + 1; // + newline
		}
		self.content.len()
	}

	pub fn begin_mouse_select(&mut self, col: u16, row: u16) {
		let idx = self.index_from_click(col, row);
		self.cursor_position = idx;
		self.selection_start = Some(idx);
		self.selection_end = Some(idx);
		self.mouse_selecting = true;
	}

	pub fn update_mouse_select(&mut self, col: u16, row: u16) {
		if !self.mouse_selecting {
			return;
		}
		let idx = self.index_from_click(col, row);
		self.cursor_position = idx;
		self.selection_end = Some(idx);
		if self.selection_start.is_none() {
			self.selection_start = Some(idx);
		}
	}

	pub fn end_mouse_select(&mut self) {
		self.mouse_selecting = false;
		// Collapse empty selections
		if let (Some(a), Some(b)) = (self.selection_start, self.selection_end)
			&& a == b
		{
			self.clear_selection();
		}
	}

	fn push_history(&mut self, entry: String) {
		if entry.trim().is_empty() {
			return;
		}
		if self.history.last().map(|s| s == &entry).unwrap_or(false) {
			return;
		}
		self.history.push(entry);
		if self.history.len() > MAX_HISTORY {
			self.history.remove(0);
		}
		self.history_index = None;
		self.draft_before_history.clear();
	}

	pub fn handle_key(&mut self, key: KeyEvent) -> InputAction {
		// Suggestion list takes priority for navigation / accept / dismiss
		if self.has_suggestions() {
			match (key.code, key.modifiers) {
				(KeyCode::Up, KeyModifiers::NONE) => return self.suggestion_move(-1),
				(KeyCode::Down, KeyModifiers::NONE) => return self.suggestion_move(1),
				(KeyCode::Tab, _) | (KeyCode::Enter, KeyModifiers::NONE) => {
					return self.accept_suggestion();
				}
				(KeyCode::Esc, _) => {
					self.clear_suggestions();
					return InputAction::None;
				}
				_ => {}
			}
		}

		let action = match (key.code, key.modifiers) {
			// Ctrl+C is exit-only. Selections auto-copy on mouse-up; no Ctrl+C copy.
			(KeyCode::Char('c'), mods) if mods.contains(KeyModifiers::CONTROL) => {
				self.clear_selection();
				self.mouse_selecting = false;
				InputAction::Exit
			}
			(KeyCode::Char('x'), KeyModifiers::CONTROL) => {
				if self.has_selection() {
					self.cut_selection()
				} else {
					InputAction::None
				}
			}
			(KeyCode::Char('d'), KeyModifiers::CONTROL) if self.content.is_empty() => InputAction::Exit,
			(KeyCode::Char('a'), KeyModifiers::CONTROL) => {
				self.select_all();
				InputAction::None
			}
			// Ctrl+V paste · Ctrl+Shift+V attach path from clipboard
			(KeyCode::Char('v'), mods) if mods.contains(KeyModifiers::CONTROL) => {
				if let Ok(clipboard_content) = cli_clipboard::get_contents() {
					if mods.contains(KeyModifiers::SHIFT) {
						let action = self.try_attach_path(clipboard_content.trim());
						if matches!(action, InputAction::None) {
							self.paste_text(&clipboard_content)
						} else {
							action
						}
					} else {
						self.paste_text(&clipboard_content)
					}
				} else {
					InputAction::None
				}
			}
			// Ctrl+O — attach path from clipboard
			(KeyCode::Char('o'), KeyModifiers::CONTROL) => {
				if let Ok(clip) = cli_clipboard::get_contents() {
					let action = self.try_attach_path(clip.trim());
					if matches!(action, InputAction::None) { self.paste_text(&clip) } else { action }
				} else {
					InputAction::None
				}
			}
			// Ctrl+Z clear line / undo-ish clear content
			(KeyCode::Char('z'), KeyModifiers::CONTROL) => {
				if !self.content.is_empty() {
					self.draft_before_history = self.content.clone();
					self.content.clear();
					self.cursor_position = 0;
					self.paste_blocks.clear();
					self.clear_selection();
					self.clear_suggestions();
					InputAction::Changed
				} else if !self.draft_before_history.is_empty() {
					self.content = std::mem::take(&mut self.draft_before_history);
					self.cursor_position = self.content.len();
					InputAction::Changed
				} else {
					InputAction::None
				}
			}
			// Ctrl+L clear input
			(KeyCode::Char('l'), KeyModifiers::CONTROL) => {
				self.content.clear();
				self.cursor_position = 0;
				self.vertical_scroll = 0;
				self.paste_blocks.clear();
				self.clear_selection();
				self.clear_suggestions();
				InputAction::Changed
			}
			// Tab: accept suggestion when open; otherwise leave for agent-mode cycle (dispatcher).
			(KeyCode::Tab, _) => {
				if self.has_suggestions() {
					self.accept_suggestion()
				} else {
					// Do not insert spaces — Tab cycles Ask/Write/Plan/Goal in the dispatcher.
					InputAction::None
				}
			}
			(KeyCode::Esc, _) => {
				if self.has_selection() {
					self.clear_selection();
					InputAction::None
				} else if self.has_suggestions() {
					self.clear_suggestions();
					InputAction::None
				} else {
					InputAction::None
				}
			}
			// Backspace on empty content removes last attachment
			(KeyCode::Backspace, KeyModifiers::NONE)
				if self.content.is_empty() && !self.attachments.is_empty() =>
			{
				self.remove_last_attachment();
				InputAction::Changed
			}
			(KeyCode::Backspace, KeyModifiers::CONTROL) => {
				self.content.clear();
				self.cursor_position = 0;
				self.scroll_offset = 0;
				self.vertical_scroll = 0;
				self.paste_blocks.clear();
				self.clear_selection();
				self.clear_suggestions();
				InputAction::Changed
			}
			// Plain Enter submits. Newlines are Alt+Enter (handled in dispatcher) or Ctrl+J.
			(KeyCode::Enter, m)
				if m.is_empty() && (!self.content.trim().is_empty() || !self.attachments.is_empty()) =>
			{
				let msg = self.compose_submit_message();
				self.push_history(self.content.clone());
				self.content.clear();
				self.cursor_position = 0;
				self.scroll_offset = 0;
				self.vertical_scroll = 0;
				self.paste_blocks.clear();
				self.attachments.clear();
				self.clear_selection();
				self.clear_suggestions();
				self.mouse_selecting = false;
				InputAction::Submit(msg)
			}
			// Alt+Enter may reach here if dispatcher didn't catch it.
			(KeyCode::Enter, m) if m.contains(KeyModifiers::ALT) => {
				self.insert_newline();
				InputAction::Changed
			}
			(KeyCode::Enter, _) => InputAction::None,
			(KeyCode::Char('j'), KeyModifiers::CONTROL) | (KeyCode::Char('\n'), _) => {
				self.insert_newline();
				InputAction::Changed
			}
			(KeyCode::Backspace, _) => {
				if self.has_selection() {
					self.delete_selection();
				} else {
					self.delete_char();
				}
				InputAction::Changed
			}
			(KeyCode::Delete, _) => {
				if self.has_selection() {
					self.delete_selection();
				} else {
					self.delete_char_forward();
				}
				InputAction::Changed
			}
			// Multi-line selection with Shift+arrows
			(KeyCode::Left, KeyModifiers::SHIFT) => {
				if self.selection_start.is_none() {
					self.selection_start = Some(self.cursor_position);
				}
				self.move_cursor_left();
				self.selection_end = Some(self.cursor_position);
				InputAction::None
			}
			(KeyCode::Right, KeyModifiers::SHIFT) => {
				if self.selection_start.is_none() {
					self.selection_start = Some(self.cursor_position);
				}
				self.move_cursor_right();
				self.selection_end = Some(self.cursor_position);
				InputAction::None
			}
			(KeyCode::Up, KeyModifiers::SHIFT) => {
				if self.selection_start.is_none() {
					self.selection_start = Some(self.cursor_position);
				}
				if !self.move_cursor_line(-1) {
					// at top of buffer — stay
				}
				self.selection_end = Some(self.cursor_position);
				InputAction::None
			}
			(KeyCode::Down, KeyModifiers::SHIFT) => {
				if self.selection_start.is_none() {
					self.selection_start = Some(self.cursor_position);
				}
				let _ = self.move_cursor_line(1);
				self.selection_end = Some(self.cursor_position);
				InputAction::None
			}
			(KeyCode::Home, KeyModifiers::SHIFT) => {
				if self.selection_start.is_none() {
					self.selection_start = Some(self.cursor_position);
				}
				self.cursor_position = self.line_start(self.cursor_position);
				self.selection_end = Some(self.cursor_position);
				InputAction::None
			}
			(KeyCode::End, KeyModifiers::SHIFT) => {
				if self.selection_start.is_none() {
					self.selection_start = Some(self.cursor_position);
				}
				self.cursor_position = self.line_end(self.cursor_position);
				self.selection_end = Some(self.cursor_position);
				InputAction::None
			}
			(KeyCode::Left, KeyModifiers::CONTROL) => {
				self.clear_selection();
				self.move_word_left();
				InputAction::None
			}
			(KeyCode::Right, KeyModifiers::CONTROL) => {
				self.clear_selection();
				self.move_word_right();
				InputAction::None
			}
			(KeyCode::Left, _) => {
				self.clear_selection();
				self.move_cursor_left();
				InputAction::None
			}
			(KeyCode::Right, _) => {
				self.clear_selection();
				self.move_cursor_right();
				InputAction::None
			}
			// Up/Down: always prefer history unless multi-line and not on first/last line
			(KeyCode::Up, KeyModifiers::NONE) => {
				self.clear_selection();
				// Multi-line: move up a line when not on first line
				if self.content.contains('\n') {
					let on_first = self.line_start(self.cursor_position) == 0;
					if !on_first && self.move_cursor_line(-1) {
						return InputAction::None;
					}
				}
				self.history_prev()
			}
			(KeyCode::Down, KeyModifiers::NONE) => {
				self.clear_selection();
				if self.content.contains('\n') {
					let end = self.line_end(self.cursor_position);
					let on_last = end >= self.content.len();
					if !on_last && self.move_cursor_line(1) {
						return InputAction::None;
					}
				}
				self.history_next()
			}
			// PageUp/PageDown scroll multi-line input viewport only
			(KeyCode::PageUp, _) => {
				self.vertical_scroll = self.vertical_scroll.saturating_sub(3);
				InputAction::None
			}
			(KeyCode::PageDown, _) => {
				self.vertical_scroll = self.vertical_scroll.saturating_add(3);
				InputAction::None
			}
			(KeyCode::Home, KeyModifiers::CONTROL) => {
				self.clear_selection();
				self.cursor_position = 0;
				self.vertical_scroll = 0;
				InputAction::None
			}
			(KeyCode::End, KeyModifiers::CONTROL) => {
				self.clear_selection();
				self.cursor_position = self.content.len();
				InputAction::None
			}
			(KeyCode::Home, _) => {
				self.clear_selection();
				self.cursor_position = self.line_start(self.cursor_position);
				InputAction::None
			}
			(KeyCode::End, _) => {
				self.clear_selection();
				self.cursor_position = self.line_end(self.cursor_position);
				InputAction::None
			}
			(KeyCode::Char('e'), KeyModifiers::CONTROL) => {
				self.clear_selection();
				self.cursor_position = self.line_end(self.cursor_position);
				InputAction::None
			}
			(KeyCode::Char('u'), KeyModifiers::CONTROL) => {
				let start = self.line_start(self.cursor_position);
				self.content.drain(start..self.cursor_position);
				self.cursor_position = start;
				self.clear_selection();
				self.gc_paste_blocks();
				InputAction::Changed
			}
			(KeyCode::Char('k'), KeyModifiers::CONTROL) => {
				let end = self.line_end(self.cursor_position);
				self.content.drain(self.cursor_position..end);
				self.clear_selection();
				self.gc_paste_blocks();
				InputAction::Changed
			}
			(KeyCode::Char('w'), KeyModifiers::CONTROL) => {
				self.delete_word();
				self.clear_selection();
				self.gc_paste_blocks();
				InputAction::Changed
			}
			// Accept printable chars even when OS reports odd modifier combos (e.g. numpad).
			(KeyCode::Char(c), mods)
				if !mods.contains(KeyModifiers::CONTROL)
					&& !mods.contains(KeyModifiers::ALT)
					&& !c.is_control() =>
			{
				if self.has_selection() {
					self.delete_selection();
				}
				self.insert_char(c);
				self.history_index = None;
				// Immediately refresh so `/` and `@` open the menu on first keystroke.
				self.refresh_suggestions();
				InputAction::Changed
			}
			_ => InputAction::None,
		};

		// Suggestions: only refresh on typing / short cursor moves — never on history
		// (history + cwd scan was making fast ↑/↓ feel broken / laggy).
		match action {
			InputAction::PreviousHistory | InputAction::NextHistory | InputAction::Submit(_) => {}
			InputAction::Changed | InputAction::Pasted { .. } => {
				self.refresh_suggestions();
			}
			InputAction::None => {
				if matches!(
					key.code,
					KeyCode::Char(_)
						| KeyCode::Backspace
						| KeyCode::Delete
						| KeyCode::Left
						| KeyCode::Right
						| KeyCode::Home
						| KeyCode::End
				) {
					self.refresh_suggestions();
				}
			}
			_ => {}
		}

		action
	}

	fn insert_char(&mut self, c: char) {
		self.content.insert(self.cursor_position, c);
		self.cursor_position += c.len_utf8();
	}

	fn insert_str(&mut self, s: &str) {
		self.content.insert_str(self.cursor_position, s);
		self.cursor_position += s.len();
	}

	fn delete_char(&mut self) {
		if self.cursor_position > 0 {
			let prev_pos = self.prev_char_boundary();
			self.content.drain(prev_pos..self.cursor_position);
			self.cursor_position = prev_pos;
			self.gc_paste_blocks();
		}
	}

	fn delete_char_forward(&mut self) {
		if self.cursor_position < self.content.len() {
			let next_pos = self.next_char_boundary();
			self.content.drain(self.cursor_position..next_pos);
			self.gc_paste_blocks();
		}
	}

	fn delete_word(&mut self) {
		if self.cursor_position == 0 {
			return;
		}

		let before_cursor = &self.content[..self.cursor_position];
		let mut delete_start = self.cursor_position;
		let mut found_word = false;

		for (index, ch) in before_cursor.char_indices().rev() {
			if !found_word {
				delete_start = index;
				if !ch.is_whitespace() {
					found_word = true;
				}
				continue;
			}

			if ch.is_whitespace() {
				break;
			}
			delete_start = index;
		}

		self.content.drain(delete_start..self.cursor_position);
		self.cursor_position = delete_start;
	}

	fn move_cursor_left(&mut self) {
		if self.cursor_position > 0 {
			self.cursor_position = self.prev_char_boundary();
		}
	}

	fn move_cursor_right(&mut self) {
		if self.cursor_position < self.content.len() {
			self.cursor_position = self.next_char_boundary();
		}
	}

	fn move_word_left(&mut self) {
		if self.cursor_position == 0 {
			return;
		}
		let before = &self.content[..self.cursor_position];
		let mut pos = self.cursor_position;
		let mut seen_word = false;
		for (index, ch) in before.char_indices().rev() {
			if !seen_word {
				if !ch.is_whitespace() {
					seen_word = true;
				}
				pos = index;
				continue;
			}
			if ch.is_whitespace() {
				break;
			}
			pos = index;
		}
		self.cursor_position = pos;
	}

	fn move_word_right(&mut self) {
		if self.cursor_position >= self.content.len() {
			return;
		}
		let after = &self.content[self.cursor_position..];
		let mut pos = self.cursor_position;
		let mut seen_word = false;
		for (index, ch) in after.char_indices() {
			let abs = self.cursor_position + index;
			if !seen_word {
				if !ch.is_whitespace() {
					seen_word = true;
				}
				pos = abs + ch.len_utf8();
				continue;
			}
			if ch.is_whitespace() {
				pos = abs;
				break;
			}
			pos = abs + ch.len_utf8();
		}
		self.cursor_position = pos.min(self.content.len());
	}

	fn line_start(&self, pos: usize) -> usize {
		self.content[..pos.min(self.content.len())].rfind('\n').map(|i| i + 1).unwrap_or(0)
	}

	fn line_end(&self, pos: usize) -> usize {
		let pos = pos.min(self.content.len());
		self.content[pos..].find('\n').map(|i| pos + i).unwrap_or(self.content.len())
	}

	/// Move cursor by one visual line; returns false if at boundary.
	fn move_cursor_line(&mut self, dir: i32) -> bool {
		let pos = self.cursor_position;
		let line_start = self.line_start(pos);
		let col = pos - line_start;

		if dir < 0 {
			if line_start == 0 {
				return false;
			}
			let prev_end = line_start - 1;
			let prev_start = self.line_start(prev_end);
			let prev_len = prev_end - prev_start;
			self.cursor_position = prev_start + col.min(prev_len);
			true
		} else {
			let line_end = self.line_end(pos);
			if line_end >= self.content.len() {
				return false;
			}
			let next_start = line_end + 1;
			let next_end = self.line_end(next_start);
			let next_len = next_end - next_start;
			self.cursor_position = next_start + col.min(next_len);
			true
		}
	}

	fn prev_char_boundary(&self) -> usize {
		let mut pos = self.cursor_position.saturating_sub(1);
		while pos > 0 && !self.content.is_char_boundary(pos) {
			pos -= 1;
		}
		pos
	}

	fn next_char_boundary(&self) -> usize {
		let mut pos = self.cursor_position + 1;
		while pos < self.content.len() && !self.content.is_char_boundary(pos) {
			pos += 1;
		}
		pos.min(self.content.len())
	}

	fn gc_paste_blocks(&mut self) {
		self.paste_blocks.retain(|b| {
			let m = paste_marker(b.id, b.lines, b.chars);
			self.content.contains(&m)
		});
	}

	/// Keep vertical_scroll so the cursor line stays in view.
	pub fn ensure_cursor_visible(&mut self, viewport_lines: usize) {
		if viewport_lines == 0 {
			return;
		}
		let display = self.display_content();
		// Approximate cursor line from raw position ratio
		let total = display.lines().count().max(1);
		let cursor_line = self.content[..self.cursor_position.min(self.content.len())]
			.lines()
			.count()
			.saturating_sub(1)
			.min(total.saturating_sub(1));
		if cursor_line < self.vertical_scroll {
			self.vertical_scroll = cursor_line;
		} else if cursor_line >= self.vertical_scroll + viewport_lines {
			self.vertical_scroll = cursor_line + 1 - viewport_lines;
		}
		let max_scroll = total.saturating_sub(viewport_lines);
		self.vertical_scroll = self.vertical_scroll.min(max_scroll);
	}

	pub fn visible_content(&self, width: usize) -> &str {
		if width == 0 {
			return "";
		}

		let start = self.clamped_char_boundary(self.scroll_offset);
		let mut end = start;
		for (count, (offset, ch)) in self.content[start..].char_indices().enumerate() {
			if count >= width {
				break;
			}
			end = start + offset + ch.len_utf8();
		}
		&self.content[start..end]
	}

	pub fn update_scroll(&mut self, width: usize) {
		if width == 0 {
			self.scroll_offset = self.cursor_position;
			return;
		}

		if self.cursor_position < self.scroll_offset {
			self.scroll_offset = self.cursor_position;
		} else if self.cursor_position >= self.scroll_offset + width {
			self.scroll_offset = self.cursor_position.saturating_sub(width - 1);
		}
	}

	fn clamped_char_boundary(&self, index: usize) -> usize {
		let mut boundary = index.min(self.content.len());
		while boundary > 0 && !self.content.is_char_boundary(boundary) {
			boundary -= 1;
		}
		boundary
	}
}

fn paste_marker(id: u32, lines: usize, chars: usize) -> String {
	format!("⟦paste:{id}:{lines}:{chars}⟧")
}

fn filter_pairs(pairs: &[(&str, &str)], query: &str) -> Vec<SuggestItem> {
	let q = query.to_lowercase();
	// Bare `/` or `@` (or just the sigil) → show the full catalog.
	let stripped = q.trim_start_matches(['/', '@']);
	let show_all = q.is_empty() || q == "/" || q == "@" || stripped.is_empty();

	pairs
		.iter()
		.filter(|(cmd, desc)| {
			if show_all {
				return true;
			}
			let cmd_l = cmd.to_lowercase();
			let desc_l = desc.to_lowercase();
			// Match full token, stripped suffix, or description.
			cmd_l.contains(&q)
				|| cmd_l.contains(stripped)
				|| cmd_l.trim_start_matches(['/', '@']).starts_with(stripped)
				|| desc_l.contains(&q)
				|| desc_l.contains(stripped)
		})
		.take(32)
		.map(|(cmd, desc)| SuggestItem {
			value: (*cmd).to_string(),
			label: (*cmd).to_string(),
			description: (*desc).to_string(),
		})
		.collect()
}

fn cwd_file_mentions(suffix: &str) -> Vec<SuggestItem> {
	let Ok(rd) = std::fs::read_dir(".") else {
		return Vec::new();
	};
	let needle = suffix.to_lowercase();
	let mut out = Vec::new();
	for entry in rd.flatten().take(200) {
		let name = entry.file_name().to_string_lossy().to_string();
		if name.starts_with('.') {
			continue;
		}
		if !name.to_lowercase().contains(&needle) {
			continue;
		}
		let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
		let value = if is_dir { format!("@{name}/") } else { format!("@{name}") };
		out.push(SuggestItem {
			label: value.clone(),
			value,
			description: if is_dir { "folder in cwd".into() } else { "file in cwd".into() },
		});
		if out.len() >= 12 {
			break;
		}
	}
	out
}

fn shellexpand_path(raw: &str) -> Result<PathBuf, ()> {
	let s = raw.trim().trim_matches('"').trim_matches('\'');
	if s.is_empty() {
		return Err(());
	}
	let expanded = if s.starts_with('~') {
		if let Some(home) = dirs::home_dir() {
			if s == "~" {
				home
			} else if let Some(rest) = s.strip_prefix("~/").or_else(|| s.strip_prefix("~\\")) {
				home.join(rest)
			} else {
				PathBuf::from(s)
			}
		} else {
			PathBuf::from(s)
		}
	} else {
		PathBuf::from(s)
	};
	Ok(expanded)
}

impl Default for InputState {
	fn default() -> Self {
		Self::new()
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputAction {
	None,
	Submit(String),
	Exit,
	PreviousHistory,
	NextHistory,
	Changed,
	Pasted { lines: usize, chars: usize },
	Copied { chars: usize },
	Attached { name: String },
}

#[cfg(test)]
mod tests {
	use super::{InputAction, InputState};
	use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

	#[test]
	fn slash_opens_suggestion_menu() {
		let mut input = InputState::new();
		let action = input.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
		assert_eq!(action, InputAction::Changed);
		assert!(input.has_suggestions(), "expected / suggestions");
		assert!(input.suggestions.iter().any(|s| s.value.starts_with('/')));
	}

	#[test]
	fn at_opens_mention_suggestions() {
		let mut input = InputState::new();
		let action = input.handle_key(KeyEvent::new(KeyCode::Char('@'), KeyModifiers::NONE));
		assert_eq!(action, InputAction::Changed);
		assert!(input.has_suggestions(), "expected @ suggestions");
		assert!(input.suggestions.iter().any(|s| s.value.starts_with('@')));
	}

	#[test]
	fn slash_mid_text_is_plain_input() {
		let mut input = InputState::new();
		input.insert_str("hello ");
		let action = input.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
		assert_eq!(action, InputAction::Changed);
		assert!(!input.has_suggestions(), "/ mid-text must not open suggestions");
		assert_eq!(input.content, "hello /");
	}

	#[test]
	fn at_mid_text_is_plain_input() {
		let mut input = InputState::new();
		input.insert_str("see ");
		let action = input.handle_key(KeyEvent::new(KeyCode::Char('@'), KeyModifiers::NONE));
		assert_eq!(action, InputAction::Changed);
		assert!(!input.has_suggestions(), "@ mid-text must not open suggestions");
		assert_eq!(input.content, "see @");
	}

	#[test]
	fn slash_on_second_line_is_plain_input() {
		let mut input = InputState::new();
		input.insert_str("line1");
		input.insert_newline();
		let action = input.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
		assert_eq!(action, InputAction::Changed);
		assert!(!input.has_suggestions(), "/ on line 2 must not open suggestions");
	}

	#[test]
	fn slash_completion_stays_open_from_first_char() {
		let mut input = InputState::new();
		input.handle_key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE));
		input.handle_key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE));
		assert!(input.has_suggestions(), "typing after leading / should keep suggestions");
	}

	#[test]
	fn delete_word_handles_unicode_before_cursor() {
		let mut input = InputState::new();
		input.content = "hello 世界".to_string();
		input.cursor_position = input.content.len();

		let action = input.handle_key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL));

		assert_eq!(action, InputAction::Changed);
		assert_eq!(input.content, "hello ");
		assert_eq!(input.cursor_position, "hello ".len());
	}

	#[test]
	fn visible_content_respects_char_boundaries() {
		let mut input = InputState::new();
		input.content = "αβγδε".to_string();
		input.scroll_offset = "α".len();

		assert_eq!(input.visible_content(2), "βγ");
	}

	#[test]
	fn update_scroll_handles_zero_width() {
		let mut input = InputState::new();
		input.content = "abc".to_string();
		input.cursor_position = 2;

		input.update_scroll(0);

		assert_eq!(input.scroll_offset, 2);
	}

	#[test]
	fn replace_content_moves_cursor_to_end_and_clears_selection() {
		let mut input = InputState::new();
		input.content = "old".to_string();
		input.cursor_position = 1;
		input.scroll_offset = 1;
		input.selection_start = Some(0);
		input.selection_end = Some(2);

		input.replace_content("dx status --json");

		assert_eq!(input.content, "dx status --json");
		assert_eq!(input.cursor_position, "dx status --json".len());
		assert_eq!(input.scroll_offset, 0);
		assert!(!input.has_selection());
	}

	#[test]
	fn paste_collapses_multiline_into_chip() {
		let mut input = InputState::new();
		let text = "line1\nline2\nline3";
		let action = input.paste_text(text);
		assert!(matches!(action, InputAction::Pasted { lines: 3, .. }));
		assert!(input.display_content().contains("[pasted 3 lines]"));
		assert!(!input.display_content().contains("line2"));
		let composed = input.compose_submit_message();
		assert!(composed.contains("line2"));
	}

	#[test]
	fn insert_newline_grows_line_count() {
		let mut input = InputState::new();
		input.insert_str("hello");
		assert_eq!(input.line_count_display(), 1);
		input.insert_newline();
		assert_eq!(input.line_count_display(), 2);
		assert!(input.content.ends_with('\n'));
	}

	#[test]
	fn copy_without_selection_is_noop() {
		let mut input = InputState::new();
		input.insert_str("hello world");
		// No selection — must not copy whole buffer
		let action = input.copy_selection();
		assert_eq!(action, InputAction::None);
	}
}
