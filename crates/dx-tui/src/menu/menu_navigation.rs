// Menu navigation and selection logic
use super::menu_data::Menu;
use super::submenus::{dx_tools, keyboard_shortcuts};
use crate::theme::ChatTheme;

impl Menu {
	pub fn enter_submenu(&mut self, index: usize) {
		self.enter_submenu_with_mode(index, false);
	}

	pub fn main_menu_len(&self) -> usize {
		self.main_menu.len()
	}

	pub fn main_menu_title(&self, index: usize) -> Option<&str> {
		self.main_menu.get(index).map(|(t, _)| *t)
	}

	pub fn enter_submenu_directly(&mut self, index: usize) {
		self.enter_submenu_with_mode(index, true);
	}

	fn enter_submenu_with_mode(&mut self, index: usize, direct: bool) {
		self.opened_directly = direct;

		// Special handling for Theme submenu (index 0)
		if index == 0 {
			let available_themes = ChatTheme::available_themes();

			let mut submenu_items = vec![];

			// Only add "Back" if not opened directly
			if !direct {
				submenu_items.push(("Back".to_string(), String::new()));
			}

			submenu_items.push(("Toggle Light/Dark Mode".to_string(), "TOGGLE_MODE".to_string()));

			for (i, (name, title)) in available_themes.iter().enumerate() {
				submenu_items.push((format!("{}. {}", i + 1, title), name.clone()));
			}

			self.menu_items = submenu_items;
			self.current_submenu = Some(index);
			self.selected_menu_item = 0;
			self.scroll_offset = 0;
			self.hovered_menu_item = None;
		}
		// Special handling for Keyboard Shortcuts submenu (index 1)
		else if index == 1 {
			let mut submenu_items = vec![];

			// Only add "Back" if not opened directly
			if !direct {
				submenu_items.push(("Back".to_string(), String::new()));
			}

			submenu_items.push(("Toggle Recording Mode".to_string(), "TOGGLE_RECORDING".to_string()));

			// Add keyboard shortcuts with dynamic mappings
			let shortcuts = keyboard_shortcuts::get_submenu_with_mappings(&self.keyboard_mappings);
			submenu_items.extend(shortcuts);

			self.menu_items = submenu_items;
			self.current_submenu = Some(index);
			self.selected_menu_item = 0;
			self.scroll_offset = 0;
			self.hovered_menu_item = None;
		} else if index < self.submenus.len() {
			self.current_submenu = Some(index);

			let mut submenu_items = vec![];

			// Only add "Back" if not opened directly
			if !direct {
				submenu_items.push(("Back".to_string(), String::new()));
			}

			submenu_items
				.extend(self.submenus[index].iter().map(|(a, b)| (a.to_string(), b.to_string())));
			self.menu_items = submenu_items;
			self.selected_menu_item = 0;
			self.scroll_offset = 0;
			self.hovered_menu_item = None;
		}
	}

	pub fn go_back_to_main(&mut self) {
		self.current_submenu = None;
		self.opened_directly = false;
		self.custom_title = None;
		self.menu_items = self.main_menu.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
		self.selected_menu_item = 0;
		self.scroll_offset = 0;
		self.hovered_menu_item = None;
	}

	pub fn select_current_item(&mut self) -> bool {
		if self.current_submenu.is_none() {
			self.enter_submenu(self.selected_menu_item);
			true
		} else if self.is_dynamic_models() || self.is_dynamic_channels() {
			// Dynamic product menus: let the dispatcher execute the payload.
			false
		} else {
			if self.selected_menu_item == 0 && !self.opened_directly {
				self.go_back_to_main();
				true
			} else {
				false
			}
		}
	}

	pub(crate) fn selected_dx_tool_action(&self) -> Option<dx_tools::DxToolAction> {
		if self.current_submenu != Some(dx_tools::DX_TOOLS_SUBMENU_INDEX) {
			return None;
		}
		dx_tools::action_for_menu_item(self.selected_menu_item, self.opened_directly)
	}

	pub fn select_next_menu_item(&mut self) {
		self.selected_menu_item = (self.selected_menu_item + 1) % self.menu_items.len();
	}

	pub fn select_prev_menu_item(&mut self) {
		if self.selected_menu_item == 0 {
			self.selected_menu_item = self.menu_items.len() - 1;
		} else {
			self.selected_menu_item -= 1;
		}
	}

	pub fn page_down(&mut self, visible_items: usize) {
		self.selected_menu_item =
			(self.selected_menu_item + visible_items).min(self.menu_items.len() - 1);
	}

	pub fn page_up(&mut self, visible_items: usize) {
		self.selected_menu_item = self.selected_menu_item.saturating_sub(visible_items);
	}

	pub fn jump_to_top(&mut self) {
		self.selected_menu_item = 0;
	}

	pub fn jump_to_bottom(&mut self) {
		self.selected_menu_item = self.menu_items.len() - 1;
	}

	// Theme-related methods
	pub fn get_selected_theme_name(&self) -> Option<String> {
		if self.current_submenu == Some(0) && self.selected_menu_item > 0 {
			Some(self.menu_items[self.selected_menu_item].1.clone())
		} else {
			None
		}
	}

	pub fn get_highlighted_theme_name(&self) -> Option<String> {
		if self.current_submenu == Some(0) {
			let item = &self.menu_items[self.selected_menu_item];
			if !item.1.is_empty() && item.1 != "TOGGLE_MODE" {
				return Some(item.1.clone());
			}
		}
		None
	}

	pub fn get_hovered_theme_name(&self) -> Option<String> {
		if self.current_submenu == Some(0)
			&& let Some(hovered_idx) = self.hovered_menu_item
			&& hovered_idx < self.menu_items.len()
		{
			let item = &self.menu_items[hovered_idx];
			if !item.1.is_empty() && item.1 != "TOGGLE_MODE" {
				return Some(item.1.clone());
			}
		}
		None
	}

	pub fn is_toggle_mode_selected(&self) -> bool {
		if self.current_submenu == Some(0) && self.selected_menu_item < self.menu_items.len() {
			self.menu_items[self.selected_menu_item].1 == "TOGGLE_MODE"
		} else {
			false
		}
	}

	// Recording mode methods
	pub fn is_toggle_recording_selected(&self) -> bool {
		if self.current_submenu == Some(1) && self.selected_menu_item < self.menu_items.len() {
			self.menu_items[self.selected_menu_item].1 == "TOGGLE_RECORDING"
		} else {
			false
		}
	}

	pub fn toggle_recording_mode(&mut self) {
		self.recording_mode = !self.recording_mode;
	}

	// Get the currently selected keyboard shortcut item for recording
	pub fn get_selected_shortcut_index(&self) -> Option<usize> {
		if self.current_submenu == Some(1) && self.recording_mode {
			// Calculate offset based on whether "Back" is present
			let offset = if self.opened_directly { 1 } else { 2 };

			if self.selected_menu_item >= offset { Some(self.selected_menu_item - offset) } else { None }
		} else {
			None
		}
	}

	// Update a keyboard shortcut
	pub fn update_keyboard_shortcut(&mut self, action_index: usize, new_shortcut: String) {
		use super::keyboard_mappings::MenuAction;

		let actions = MenuAction::all_actions();
		if action_index < actions.len() {
			self.keyboard_mappings.set(actions[action_index], new_shortcut.clone());

			// Refresh the menu items to show the updated shortcut
			if self.current_submenu == Some(1) {
				let mut submenu_items = vec![];

				// Only add "Back" if not opened directly
				if !self.opened_directly {
					submenu_items.push(("Back".to_string(), String::new()));
				}

				submenu_items.push(("Toggle Recording Mode".to_string(), "TOGGLE_RECORDING".to_string()));

				let shortcuts = keyboard_shortcuts::get_submenu_with_mappings(&self.keyboard_mappings);
				submenu_items.extend(shortcuts);

				self.menu_items = submenu_items;
			}
		}
	}
}
