// Keyboard Shortcuts submenu - uses dynamic mappings
use crate::menu::keyboard_mappings::{KeyboardMappings, MenuAction};

pub fn get_submenu_with_mappings(mappings: &KeyboardMappings) -> Vec<(String, String)> {
	MenuAction::all_actions()
		.into_iter()
		.enumerate()
		.map(|(i, action)| {
			let title = format!("{}. {}", i + 1, action.display_name());
			let shortcut = mappings.get(action);
			(title, shortcut)
		})
		.collect()
}

// Static version for initial loading (will be replaced with dynamic version)
pub fn get_submenu() -> Vec<(&'static str, &'static str)> {
	vec![
		("1. Context Control Panel", "0 or Ctrl+P"),
		("2. Speak selection / last answer (TTS)", "Ctrl+T"),
		("3. Voice listen + wave bars (STT)", "Ctrl+S"),
		("4. Providers", "Ctrl+Shift+P"),
		("5. Plugins & Apps", "Ctrl+Shift+A"),
		("6. Skills", "Ctrl+Shift+S"),
		("7. Sandbox", "Ctrl+Shift+B"),
		("8. Web Search", "Ctrl+Shift+W"),
		("9. MCP Servers", "Ctrl+Shift+M"),
		("10. Memory & History", "Ctrl+Shift+H"),
		("11. Multi-Agent", "Ctrl+Shift+G"),
		("12. Notifications", "Ctrl+Shift+N"),
		("13. Voice / Realtime", "Ctrl+Shift+V"),
		("14. Image & Vision", "Ctrl+Shift+I"),
		("15. Profiles", "Ctrl+Shift+R"),
		("16. Worktree", "Ctrl+Shift+T"),
		("17. Authentication", "Ctrl+Shift+U"),
		("18. Network & Proxy", "Ctrl+Shift+X"),
		("19. Hooks & Events", "Ctrl+Shift+E"),
		("20. Session Resume", "Ctrl+Shift+L"),
		("21. Approval Policy", "Ctrl+Shift+O"),
		("22. Shell Environment", "Ctrl+Shift+J"),
		("23. Execution Rules", "Ctrl+Shift+C"),
		("24. Project Trust", "Ctrl+Shift+Y"),
		("25. Developer Instructions", "Ctrl+Shift+D"),
		("26. Feature Flags", "Ctrl+Shift+F"),
	]
}
