#[derive(Debug, Clone)]
pub struct Command {
	pub name: &'static str,
	pub description: &'static str,
	pub category: CommandCategory,
	pub action: CommandAction,
	pub shortcut: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandCategory {
	File,
	Edit,
	View,
	Session,
	Tools,
	Help,
}

impl CommandCategory {
	pub fn label(self) -> &'static str {
		match self {
			Self::File => "File",
			Self::Edit => "Edit",
			Self::View => "View",
			Self::Session => "Session",
			Self::Tools => "Tools",
			Self::Help => "Help",
		}
	}
}

#[derive(Debug, Clone)]
pub enum CommandAction {
	NewSession,
	ResumeSession,
	RenameSession,
	ExportSession,
	ShareSession,
	ForkSession,
	OpenDiff,
	ToggleSidebar,
	ToggleTimestamps,
	ToggleThinking,
	ToggleTheme,
	CycleMode,
	CycleModel,
	ToggleRuntime,
	OpenMenu(u8),
	ToggleNotifications,
	OpenVoice,
	OpenHelp,
	OpenStatus,
	OpenSessions,
	TogglePerf,
	ClearChat,
	CopyLastResponse,
	Interrupt,
	RequestExit,
	Custom(String),
}

fn default_commands() -> Vec<Command> {
	vec![
		Command {
			name: "New Session",
			description: "Start a new chat session",
			category: CommandCategory::Session,
			action: CommandAction::NewSession,
			shortcut: Some("Ctrl+N"),
		},
		Command {
			name: "Resume Session",
			description: "List and resume saved sessions",
			category: CommandCategory::Session,
			action: CommandAction::ResumeSession,
			shortcut: Some("/sessions"),
		},
		Command {
			name: "Rename Session",
			description: "Rename the current session",
			category: CommandCategory::Session,
			action: CommandAction::RenameSession,
			shortcut: Some("/rename"),
		},
		Command {
			name: "Export Session",
			description: "Export transcript to file",
			category: CommandCategory::Session,
			action: CommandAction::ExportSession,
			shortcut: Some("/export"),
		},
		Command {
			name: "Share Session",
			description: "Share session to a channel",
			category: CommandCategory::Session,
			action: CommandAction::ShareSession,
			shortcut: Some("/share"),
		},
		Command {
			name: "Fork Session",
			description: "Fork from a message",
			category: CommandCategory::Session,
			action: CommandAction::ForkSession,
			shortcut: Some("/fork"),
		},
		Command {
			name: "Open Diff",
			description: "Open full-screen diff viewer",
			category: CommandCategory::View,
			action: CommandAction::OpenDiff,
			shortcut: Some("Ctrl+D"),
		},
		Command {
			name: "Toggle Sidebar",
			description: "Show/hide the right sidebar",
			category: CommandCategory::View,
			action: CommandAction::ToggleSidebar,
			shortcut: Some("Ctrl+B"),
		},
		Command {
			name: "Toggle Timestamps",
			description: "Show/hide message timestamps",
			category: CommandCategory::View,
			action: CommandAction::ToggleTimestamps,
			shortcut: None,
		},
		Command {
			name: "Toggle Thinking",
			description: "Expand/collapse thinking blocks",
			category: CommandCategory::View,
			action: CommandAction::ToggleThinking,
			shortcut: Some("Alt+T"),
		},
		Command {
			name: "Toggle Theme",
			description: "Switch between light/dark themes",
			category: CommandCategory::View,
			action: CommandAction::ToggleTheme,
			shortcut: Some("T"),
		},
		Command {
			name: "Cycle Mode",
			description: "Cycle agent mode (Ask/Write/Plan/Goal)",
			category: CommandCategory::Tools,
			action: CommandAction::CycleMode,
			shortcut: Some("Tab"),
		},
		Command {
			name: "Cycle Model",
			description: "Switch to next model",
			category: CommandCategory::Tools,
			action: CommandAction::CycleModel,
			shortcut: Some("Ctrl+M"),
		},
		Command {
			name: "Toggle Runtime",
			description: "Switch Local/Remote runtime",
			category: CommandCategory::Tools,
			action: CommandAction::ToggleRuntime,
			shortcut: Some("Ctrl+L"),
		},
		Command {
			name: "Menu: Theme",
			description: "Open theme selection menu",
			category: CommandCategory::Tools,
			action: CommandAction::OpenMenu(0),
			shortcut: Some("2"),
		},
		Command {
			name: "Menu: Providers",
			description: "Open provider settings",
			category: CommandCategory::Tools,
			action: CommandAction::OpenMenu(2),
			shortcut: Some("4"),
		},
		Command {
			name: "Menu: Notifications",
			description: "Open notification settings",
			category: CommandCategory::Tools,
			action: CommandAction::OpenMenu(10),
			shortcut: None,
		},
		Command {
			name: "Open Voice",
			description: "Open voice STT/TTS panel",
			category: CommandCategory::Tools,
			action: CommandAction::OpenVoice,
			shortcut: Some("Space"),
		},
		Command {
			name: "Help",
			description: "Show slash command help",
			category: CommandCategory::Help,
			action: CommandAction::OpenHelp,
			shortcut: Some("/help"),
		},
		Command {
			name: "Status",
			description: "Show session status",
			category: CommandCategory::Help,
			action: CommandAction::OpenStatus,
			shortcut: Some("/status"),
		},
		Command {
			name: "Open Sessions",
			description: "Browse all saved sessions",
			category: CommandCategory::Session,
			action: CommandAction::OpenSessions,
			shortcut: Some("/sessions"),
		},
		Command {
			name: "Toggle Performance",
			description: "Show/hide performance overlay",
			category: CommandCategory::View,
			action: CommandAction::TogglePerf,
			shortcut: None,
		},
		Command {
			name: "Clear Chat",
			description: "Clear all messages",
			category: CommandCategory::Edit,
			action: CommandAction::ClearChat,
			shortcut: None,
		},
		Command {
			name: "Copy Last Response",
			description: "Copy the last assistant response",
			category: CommandCategory::Edit,
			action: CommandAction::CopyLastResponse,
			shortcut: None,
		},
		Command {
			name: "Interrupt",
			description: "Stop the current generation",
			category: CommandCategory::Tools,
			action: CommandAction::Interrupt,
			shortcut: Some("Ctrl+C"),
		},
		Command {
			name: "Exit",
			description: "Quit the TUI",
			category: CommandCategory::File,
			action: CommandAction::RequestExit,
			shortcut: Some("Ctrl+C"),
		},
	]
}

fn fuzzy_score(query: &str, target: &str) -> u32 {
	let query = query.to_lowercase();
	let target = target.to_lowercase();
	if query.is_empty() {
		return 100;
	}
	if target == query {
		return 200;
	}
	if target.starts_with(&query) {
		return 150;
	}
	if target.contains(&query) {
		return 100;
	}
	let mut qi = query.chars().peekable();
	let mut score = 0u32;
	let mut prev_match = false;
	for ch in target.chars() {
		if let Some(&qc) = qi.peek() {
			if ch == qc {
				qi.next();
				if prev_match {
					score = score.saturating_add(3);
				} else {
					score = score.saturating_add(1);
				}
				prev_match = true;
			} else {
				prev_match = false;
			}
		}
	}
	if qi.next().is_some() {
		return 0;
	}
	score
}

fn find_best_by_category<'a>(commands: &'a [Command], query: &str) -> Vec<(&'a Command, u32)> {
	let mut results: Vec<(&Command, u32)> = commands
		.iter()
		.filter_map(|cmd| {
			let score = fuzzy_score(query, cmd.name)
				.max(fuzzy_score(query, cmd.description))
				.max(fuzzy_score(query, cmd.category.label()));
			if score > 0 { Some((cmd, score)) } else { None }
		})
		.collect();
	results.sort_unstable_by_key(|b| std::cmp::Reverse(b.1));
	results.truncate(20);
	results
}

#[derive(Debug, Clone)]
pub struct CommandPalette {
	pub open: bool,
	pub query: String,
	pub cursor: usize,
	pub results: Vec<(usize, usize)>, // (command_index_in_filtered, category_index)
	commands: Vec<Command>,
	category_order: Vec<CommandCategory>,
}

impl CommandPalette {
	pub fn new() -> Self {
		let commands = default_commands();
		let category_order = vec![
			CommandCategory::Session,
			CommandCategory::File,
			CommandCategory::Edit,
			CommandCategory::View,
			CommandCategory::Tools,
			CommandCategory::Help,
		];
		Self {
			open: false,
			query: String::new(),
			cursor: 0,
			results: Vec::new(),
			commands,
			category_order,
		}
	}

	pub fn toggle(&mut self) {
		self.open = !self.open;
		if !self.open {
			self.query.clear();
			self.cursor = 0;
			self.results.clear();
		} else {
			self.search();
		}
	}

	pub fn open(&mut self) {
		self.open = true;
		self.search();
	}

	pub fn close(&mut self) {
		self.open = false;
		self.query.clear();
		self.cursor = 0;
		self.results.clear();
	}

	pub fn push_char(&mut self, c: char) {
		self.query.push(c);
		self.cursor = 0;
		self.search();
	}

	pub fn pop_char(&mut self) {
		self.query.pop();
		self.cursor = 0;
		self.search();
	}

	pub fn move_cursor(&mut self, delta: i32) {
		let len = self.results.len();
		if len == 0 {
			return;
		}
		if delta < 0 {
			self.cursor = self.cursor.saturating_sub((-delta) as usize);
		} else {
			self.cursor = (self.cursor + delta as usize).min(len.saturating_sub(1));
		}
	}

	pub fn selected_command(&self) -> Option<&Command> {
		let (_ci, _cati) = self.results.get(self.cursor)?;
		// Re-derive from filtered list
		if self.query.is_empty() {
			let mut cat_offset = 0usize;
			for cat in &self.category_order {
				let cat_commands: Vec<&Command> =
					self.commands.iter().filter(|c| c.category == *cat).collect();
				if cat_commands.is_empty() {
					continue;
				}
				let cat_len = cat_commands.len();
				if self.cursor < cat_offset + cat_len {
					return Some(cat_commands[self.cursor - cat_offset]);
				}
				cat_offset += cat_len + 1; // +1 for category header
			}
			None
		} else {
			let scored = find_best_by_category(&self.commands, &self.query);
			scored.get(self.cursor).map(|(cmd, _)| *cmd)
		}
	}

	pub fn search(&mut self) {
		self.results.clear();
		if self.query.is_empty() {
			let mut offset = 0usize;
			for cat in &self.category_order {
				let count = self.commands.iter().filter(|c| c.category == *cat).count();
				if count == 0 {
					continue;
				}
				for ci in 0..count {
					self.results.push((ci, offset));
				}
				offset += count + 1;
			}
			return;
		}
		let scored = find_best_by_category(&self.commands, &self.query);
		self.results = scored.iter().enumerate().map(|(i, _)| (i, 0)).collect();
	}

	pub fn grouped_results(&self) -> Vec<(CommandCategory, Vec<&Command>)> {
		if self.query.is_empty() {
			self
				.category_order
				.iter()
				.map(|cat| {
					let cmds: Vec<&Command> = self.commands.iter().filter(|c| c.category == *cat).collect();
					(*cat, cmds)
				})
				.filter(|(_, cmds)| !cmds.is_empty())
				.collect()
		} else {
			let scored = find_best_by_category(&self.commands, &self.query);
			let mut groups: Vec<(CommandCategory, Vec<&Command>)> = Vec::new();
			for (cmd, _score) in &scored {
				let last = groups.last_mut();
				if let Some((cat, cmds)) = last
					&& *cat == cmd.category
				{
					cmds.push(cmd);
					continue;
				}
				groups.push((cmd.category, vec![*cmd]));
			}
			groups
		}
	}

	pub fn all_commands(&self) -> &[Command] {
		&self.commands
	}

	pub fn visible_count(&self) -> usize {
		if self.query.is_empty() {
			self.commands.len() + self.category_order.len()
		} else {
			self.results.len()
		}
	}
}

impl Default for CommandPalette {
	fn default() -> Self {
		Self::new()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_palette_default_state() {
		let p = CommandPalette::new();
		assert!(!p.open);
		assert!(p.query.is_empty());
		assert!(!p.commands.is_empty());
	}

	#[test]
	fn test_toggle() {
		let mut p = CommandPalette::new();
		p.toggle();
		assert!(p.open);
		p.toggle();
		assert!(!p.open);
	}

	#[test]
	fn test_search_finds_commands() {
		let mut p = CommandPalette::new();
		p.open();
		p.push_char('n');
		p.push_char('e');
		p.push_char('w');
		assert!(!p.results.is_empty());
		let cmd = p.selected_command();
		assert!(cmd.is_some());
		assert!(cmd.unwrap().name.to_lowercase().contains("new"));
	}

	#[test]
	fn test_fuzzy_scoring() {
		let score = fuzzy_score("sess", "Session Resume");
		assert!(score > 0);
		let score2 = fuzzy_score("xyz", "Session Resume");
		assert_eq!(score2, 0);
	}

	#[test]
	fn test_grouped_results_when_empty_query() {
		let p = CommandPalette::new();
		let groups = p.grouped_results();
		assert!(!groups.is_empty());
		let cat_count: usize = groups.iter().map(|(_, cmds)| cmds.len()).sum();
		assert_eq!(cat_count, p.commands.len());
	}

	#[test]
	fn test_cursor_bounds() {
		let mut p = CommandPalette::new();
		p.open();
		assert_eq!(p.cursor, 0);
		p.move_cursor(1);
		assert_eq!(p.cursor, 1);
		p.move_cursor(-5);
		assert_eq!(p.cursor, 0);
	}

	#[test]
	fn test_close_resets_state() {
		let mut p = CommandPalette::new();
		p.open();
		p.push_char('e');
		assert!(!p.query.is_empty());
		p.close();
		assert!(p.query.is_empty());
		assert_eq!(p.cursor, 0);
		assert!(p.results.is_empty());
	}
}
