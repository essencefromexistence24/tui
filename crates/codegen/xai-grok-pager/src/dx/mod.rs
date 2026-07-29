//! Native DX presentation components.
//!
//! This module is intentionally part of the pager rather than an embedded
//! second application. The pager remains the sole owner of the terminal,
//! event stream, agent state, persistence, and side effects.

pub mod animation;
#[path = "../../../../dx-tui/src/diff_view.rs"]
pub mod diff_view;
#[path = "../../../../dx-tui/src/editor/mod.rs"]
pub mod editor;
pub mod effects;
pub mod file_browser;
#[path = "../../../../dx-tui/src/menu/mod.rs"]
pub mod menu;
pub mod minimap;
pub mod sidebar;
pub mod state;
pub use crate::theme;
#[path = "../../../../dx-tui/src/sound.rs"]
pub mod sound;
#[path = "../../../../dx-tui/src/splash.rs"]
pub mod splash;

pub use state::{DxAction, DxUiState, DxView};
