use super::editor::EditorAdapter;
use super::state::ChatState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
	Chat,       // Chat TUI is active
	FilePicker, // DX file browser is active (modal)
	Editor,     // DX code editor is active (modal)
}

pub struct TuiSession {
	pub chat_state: ChatState,
	pub mode: AppMode,
	pub editor_adapter: EditorAdapter,
}

impl Default for TuiSession {
	fn default() -> Self {
		Self::new()
	}
}

impl TuiSession {
	pub fn new() -> Self {
		Self { chat_state: ChatState::new(), mode: AppMode::Chat, editor_adapter: EditorAdapter::new() }
	}
}
