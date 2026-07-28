use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VimMode {
	#[default]
	Normal,
	Insert,
	Visual,
	Command,
}

impl VimMode {
	pub fn label(self) -> &'static str {
		match self {
			Self::Normal => "NORMAL",
			Self::Insert => "INSERT",
			Self::Visual => "VISUAL",
			Self::Command => "COMMAND",
		}
	}

	pub fn color_index(self) -> u8 {
		match self {
			Self::Normal => 4,
			Self::Insert => 2,
			Self::Visual => 5,
			Self::Command => 3,
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VimAction {
	EnterInsertMode,
	EnterInsertAtEnd,
	EnterInsertBeforeLine,
	EnterInsertAfterLine,
	EnterCommandMode,
	EnterVisualMode,
	MoveLeft,
	MoveDown,
	MoveUp,
	MoveRight,
	MoveWordForward,
	MoveWordBackward,
	MoveToLineStart,
	MoveToLineEnd,
	MoveToTop,
	MoveToBottom,
	DeleteChar,
	DeleteLine,
	YankLine,
	PasteAfter,
	PasteBefore,
	Undo,
	Redo,
	Search,
	RepeatLast,
	JoinLines,
	Indent,
	Dedent,
	None,
}

#[derive(Debug, Clone)]
pub struct VimKeymap {
	pub mode: VimMode,
	pub enabled: bool,
	/// Whether the user has pressed a leader key (e.g. 'g' in Normal mode) and is waiting for the next key.
	pending_leader: Option<char>,
	/// Registry of yanked/deleted lines.
	pub clipboard: Vec<String>,
	/// Visual selection start message index.
	pub visual_anchor: Option<usize>,
	/// Last executed command (for `.` repeat).
	last_command: Option<VimAction>,
	/// Command-line buffer for `:` mode.
	pub command_buffer: String,
	/// Last search query.
	pub last_search: String,
}

impl VimKeymap {
	pub fn new() -> Self {
		Self {
			mode: VimMode::Normal,
			enabled: false,
			pending_leader: None,
			clipboard: Vec::new(),
			visual_anchor: None,
			last_command: None,
			command_buffer: String::new(),
			last_search: String::new(),
		}
	}

	pub fn toggle(&mut self) {
		self.enabled = !self.enabled;
		if !self.enabled {
			self.mode = VimMode::Normal;
			self.pending_leader = None;
		}
	}

	pub fn reset_to_normal(&mut self) {
		self.mode = VimMode::Normal;
		self.pending_leader = None;
		self.command_buffer.clear();
	}

	fn enter_normal_mode(&mut self) {
		self.mode = VimMode::Normal;
		self.pending_leader = None;
		self.visual_anchor = None;
	}

	pub fn handle_key(&mut self, key: KeyEvent) -> VimAction {
		if !self.enabled {
			return VimAction::None;
		}
		if self.mode == VimMode::Insert {
			if key.code == KeyCode::Esc {
				self.enter_normal_mode();
			}
			return VimAction::None;
		}

		if self.mode == VimMode::Command {
			return self.handle_command_mode(key);
		}

		let action = match key.code {
			KeyCode::Char(c) if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT => {
				if let Some(leader) = self.pending_leader.take() {
					self.handle_leader_chord(leader, c, key.modifiers.contains(KeyModifiers::SHIFT))
				} else {
					self.handle_normal_char(c, key.modifiers.contains(KeyModifiers::SHIFT))
				}
			}
			KeyCode::Esc => {
				if self.mode == VimMode::Visual {
					self.visual_anchor = None;
				} else if self.mode == VimMode::Normal && self.pending_leader.is_some() {
					self.pending_leader = None;
				}
				self.enter_normal_mode();
				VimAction::None
			}
			KeyCode::Enter if self.mode == VimMode::Normal => {
				self.mode = VimMode::Insert;
				VimAction::EnterInsertMode
			}
			KeyCode::Backspace if self.mode == VimMode::Command => {
				self.command_buffer.pop();
				VimAction::None
			}
			_ => VimAction::None,
		};

		if action != VimAction::None {
			self.last_command = Some(action);
		}
		action
	}

	fn handle_leader_chord(&mut self, leader: char, second: char, _shift: bool) -> VimAction {
		match (leader, second) {
			('g', 'g') => VimAction::MoveToTop,
			('G', 'G') | ('g', 'G') => VimAction::MoveToBottom,
			('y', 'y') => VimAction::YankLine,
			('d', 'd') => VimAction::DeleteLine,
			('p', 'p') | ('p', _) => VimAction::PasteAfter,
			('u', _) => VimAction::Undo,
			('r', 'r') => {
				self.mode = VimMode::Insert;
				VimAction::EnterInsertMode
			}
			('J', _) | ('j', 'J') => VimAction::JoinLines,
			('>', '>') => VimAction::Indent,
			('<', '<') => VimAction::Dedent,
			_ => {
				self.pending_leader = None;
				VimAction::None
			}
		}
	}

	fn handle_normal_char(&mut self, c: char, shift: bool) -> VimAction {
		match (c, shift) {
			('i', false) => {
				self.mode = VimMode::Insert;
				VimAction::EnterInsertMode
			}
			('a', false) => {
				self.mode = VimMode::Insert;
				VimAction::EnterInsertAtEnd
			}
			('I', _) => {
				self.mode = VimMode::Insert;
				VimAction::EnterInsertBeforeLine
			}
			('A', _) => {
				self.mode = VimMode::Insert;
				VimAction::EnterInsertAfterLine
			}
			('h', false) => VimAction::MoveLeft,
			('j', false) => VimAction::MoveDown,
			('k', false) => VimAction::MoveUp,
			('l', false) => VimAction::MoveRight,
			('w', false) => VimAction::MoveWordForward,
			('b', false) => VimAction::MoveWordBackward,
			('0', false) => VimAction::MoveToLineStart,
			('$', _) => VimAction::MoveToLineEnd,
			('g', false) | ('G', _) => {
				self.pending_leader = Some(c);
				VimAction::None
			}
			('d', false) => {
				self.pending_leader = Some('d');
				VimAction::None
			}
			('y', false) => {
				self.pending_leader = Some('y');
				VimAction::None
			}
			('p', false) => VimAction::PasteAfter,
			('P', _) => VimAction::PasteBefore,
			('u', false) => VimAction::Undo,
			('r', false) => VimAction::RepeatLast,
			('J', _) | ('j', true) => VimAction::JoinLines,
			('v', false) => {
				self.mode = VimMode::Visual;
				VimAction::EnterVisualMode
			}
			('V', _) => {
				self.mode = VimMode::Visual;
				VimAction::EnterVisualMode
			}
			('/', false) => {
				self.mode = VimMode::Command;
				self.command_buffer = String::from('/');
				VimAction::Search
			}
			(':', _) => {
				self.mode = VimMode::Command;
				self.command_buffer = String::from(':');
				VimAction::None
			}
			('x', false) => VimAction::DeleteChar,
			('X', _) => VimAction::DeleteChar,
			('.', _) => VimAction::RepeatLast,
			('n', false) => VimAction::Search,
			('N', _) => VimAction::Search,
			('o', false) => {
				self.mode = VimMode::Insert;
				VimAction::EnterInsertAfterLine
			}
			('O', _) => {
				self.mode = VimMode::Insert;
				VimAction::EnterInsertBeforeLine
			}
			('\r', _) => VimAction::EnterInsertMode,
			_ => VimAction::None,
		}
	}

	fn handle_command_mode(&mut self, key: KeyEvent) -> VimAction {
		match key.code {
			KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
				self.command_buffer.push(c);
				VimAction::None
			}
			KeyCode::Backspace => {
				self.command_buffer.pop();
				VimAction::None
			}
			KeyCode::Enter => {
				let cmd = self.command_buffer.clone();
				self.enter_normal_mode();
				if let Some(rest) = cmd.strip_prefix(':') {
					match rest.trim() {
						"q" | "quit" => return VimAction::Undo,
						_ => {}
					}
				} else if let Some(rest) = cmd.strip_prefix('/').or_else(|| cmd.strip_prefix('?')) {
					self.last_search = rest.to_string();
				}
				VimAction::None
			}
			KeyCode::Esc => {
				self.enter_normal_mode();
				VimAction::None
			}
			_ => VimAction::None,
		}
	}

	pub fn pending_leader_char(&self) -> Option<char> {
		self.pending_leader
	}
}

impl Default for VimKeymap {
	fn default() -> Self {
		Self::new()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

	fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
		KeyEvent::new(code, modifiers)
	}

	#[test]
	fn test_default_mode_is_normal() {
		let km = VimKeymap::new();
		assert_eq!(km.mode, VimMode::Normal);
		assert!(!km.enabled);
	}

	#[test]
	fn test_toggle_enables_disables() {
		let mut km = VimKeymap::new();
		km.toggle();
		assert!(km.enabled);
		km.toggle();
		assert!(!km.enabled);
	}

	#[test]
	fn test_insert_mode_entered_on_i() {
		let mut km = VimKeymap::new();
		km.enabled = true;
		let action = km.handle_key(key(KeyCode::Char('i'), KeyModifiers::NONE));
		assert_eq!(action, VimAction::EnterInsertMode);
		assert_eq!(km.mode, VimMode::Insert);
	}

	#[test]
	fn test_esc_returns_to_normal() {
		let mut km = VimKeymap::new();
		km.enabled = true;
		km.mode = VimMode::Insert;
		let action = km.handle_key(key(KeyCode::Esc, KeyModifiers::NONE));
		assert_eq!(action, VimAction::None);
		assert_eq!(km.mode, VimMode::Normal);
	}

	#[test]
	fn test_h_j_k_l_navigation() {
		let mut km = VimKeymap::new();
		km.enabled = true;
		assert_eq!(km.handle_key(key(KeyCode::Char('h'), KeyModifiers::NONE)), VimAction::MoveLeft);
		assert_eq!(km.handle_key(key(KeyCode::Char('j'), KeyModifiers::NONE)), VimAction::MoveDown);
		assert_eq!(km.handle_key(key(KeyCode::Char('k'), KeyModifiers::NONE)), VimAction::MoveUp);
		assert_eq!(km.handle_key(key(KeyCode::Char('l'), KeyModifiers::NONE)), VimAction::MoveRight);
	}

	#[test]
	fn test_yy_yank_line() {
		let mut km = VimKeymap::new();
		km.enabled = true;
		let action = km.handle_key(key(KeyCode::Char('y'), KeyModifiers::NONE));
		assert_eq!(action, VimAction::None);
		assert!(km.pending_leader_char().is_some());
		let action = km.handle_key(key(KeyCode::Char('y'), KeyModifiers::NONE));
		assert_eq!(action, VimAction::YankLine);
	}

	#[test]
	fn test_dd_delete_line() {
		let mut km = VimKeymap::new();
		km.enabled = true;
		km.handle_key(key(KeyCode::Char('d'), KeyModifiers::NONE));
		let action = km.handle_key(key(KeyCode::Char('d'), KeyModifiers::NONE));
		assert_eq!(action, VimAction::DeleteLine);
	}

	#[test]
	fn test_gg_top() {
		let mut km = VimKeymap::new();
		km.enabled = true;
		km.handle_key(key(KeyCode::Char('g'), KeyModifiers::NONE));
		let action = km.handle_key(key(KeyCode::Char('g'), KeyModifiers::NONE));
		assert_eq!(action, VimAction::MoveToTop);
	}

	#[test]
	fn test_command_mode_colon() {
		let mut km = VimKeymap::new();
		km.enabled = true;
		let action = km.handle_key(key(KeyCode::Char(':'), KeyModifiers::SHIFT));
		assert_eq!(action, VimAction::None);
		assert_eq!(km.mode, VimMode::Command);
		assert_eq!(km.command_buffer, ":");
	}

	#[test]
	fn test_search_forward_slash() {
		let mut km = VimKeymap::new();
		km.enabled = true;
		let action = km.handle_key(key(KeyCode::Char('/'), KeyModifiers::NONE));
		assert_eq!(action, VimAction::Search);
		assert_eq!(km.mode, VimMode::Command);
	}

	#[test]
	fn test_visual_mode() {
		let mut km = VimKeymap::new();
		km.enabled = true;
		let action = km.handle_key(key(KeyCode::Char('v'), KeyModifiers::NONE));
		assert_eq!(action, VimAction::EnterVisualMode);
		assert_eq!(km.mode, VimMode::Visual);
	}

	#[test]
	fn test_reset_to_normal_clears_state() {
		let mut km = VimKeymap::new();
		km.enabled = true;
		km.mode = VimMode::Command;
		km.command_buffer = ":q".to_string();
		km.pending_leader = Some('g');
		km.reset_to_normal();
		assert_eq!(km.mode, VimMode::Normal);
		assert!(km.command_buffer.is_empty());
		assert!(km.pending_leader.is_none());
	}

	#[test]
	fn test_label() {
		assert_eq!(VimMode::Normal.label(), "NORMAL");
		assert_eq!(VimMode::Insert.label(), "INSERT");
		assert_eq!(VimMode::Visual.label(), "VISUAL");
	}

	#[test]
	fn test_disabled_returns_none() {
		let mut km = VimKeymap::new();
		km.enabled = false;
		let action = km.handle_key(key(KeyCode::Char('i'), KeyModifiers::NONE));
		assert_eq!(action, VimAction::None);
	}
}
