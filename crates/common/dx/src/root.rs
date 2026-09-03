use fb_binding::elements::render_once;
use fb_core::Core;
use fb_plugin::LUA;
use fb_shared::url::UrlLike;
use mlua::{ObjectLike, Table};
use ratatui::{buffer::Buffer, layout::Rect, text::Line, widgets::{Paragraph, Widget, Wrap}};
use tracing::error;

use crate::file_browser::{cmp, confirm, help, input, mgr, pick, spot, tasks, which};
use crate::{
	bridge::{AppMode, TuiSession},
	state::AnimationType,
};

pub struct TerminalRoot<'a> {
	core: &'a Core,
	bridge: &'a mut TuiSession,
}

impl<'a> TerminalRoot<'a> {
	pub fn new(core: &'a Core, bridge: &'a mut TuiSession) -> Self {
		Self { core, bridge }
	}

	pub fn reflow(area: Rect) -> mlua::Result<Table> {
		let area = fb_binding::elements::Rect::from(area);
		let root = LUA.globals().raw_get::<Table>("Root")?.call_method::<Table>("new", area)?;
		root.call_method("reflow", ())
	}

	fn render_file_browser(&self, area: Rect, buf: &mut Buffer) -> bool {
		let mut f = || {
			let lua_area = fb_binding::elements::Rect::from(area);
			let root = LUA.globals().raw_get::<Table>("Root")?.call_method::<Table>("new", lua_area)?;
			render_once(root.call_method("redraw", ())?, buf, |p| self.core.mgr.area(p));
			Ok::<_, mlua::Error>(())
		};
		match f() {
			Ok(()) => true,
			Err(e) => {
				error!("Failed to redraw the `Root` component:\n{e}");
				false
			}
		}
	}

	fn render_file_browser_components(&mut self, area: Rect, buf: &mut Buffer, lua_ok: bool) {
		self.bridge.chat_state.ui.fb_scrollbar_area =
			Rect::new(area.right().saturating_sub(1), area.y, 1, area.height);
		if !lua_ok {
			let cwd = self.core.mgr.cwd();
			let fallback = format!(
				"File Browser\n\n\
				 Navigate: ←/→ arrows · Esc to return\n{}",
				cwd.os_str().to_string_lossy()
			);
			Paragraph::new(Line::from(fallback)).wrap(Wrap { trim: false }).render(area, buf);
			return;
		}
		mgr::Preview::new(self.core).render(area, buf);
		mgr::Modal::new(self.core).render(area, buf);
		if self.core.tasks.visible {
			tasks::Tasks::new(self.core).render(area, buf);
		}
		if self.core.active().spot.visible() {
			spot::Spot::new(self.core).render(area, buf);
		}
		if self.core.pick.visible {
			pick::Pick::new(self.core).render(area, buf);
		}
		if self.core.input.visible {
			input::Input::new(self.core).render(area, buf);
		}
		if self.core.confirm.visible {
			confirm::Confirm::new(self.core).render(area, buf);
		}
		if self.core.help.visible {
			help::Help::new(self.core).render(area, buf);
		}
		if self.core.cmp.visible {
			cmp::Cmp::new(self.core).render(area, buf);
		}
		if self.core.which.active {
			which::Which::new(self.core).render(area, buf);
		}

		// Scrollbar for the current file listing (editor-style, draggable)
		let folder = &self.core.mgr.active().current;
		let total = folder.files.len();
		let visible = area.height as usize;
		let hovered = self.bridge.chat_state.ui.fb_scrollbar_hovered;
		if total > visible {
			let scrollbar_area = Rect::new(area.x + area.width.saturating_sub(1), area.y, 1, area.height);
			crate::components::render_scrollbar_track_hover(
				scrollbar_area,
				buf,
				total,
				folder.offset,
				hovered,
				1,
			);
		}
	}

	/// Split terminal into file-browser pane + bottom chat chrome tall enough
	/// for the input row, bottom controls, and any open suggestion list.
	fn file_browser_layout(area: Rect, chat: &crate::state::ChatState) -> (Rect, Rect) {
		let suggest_h = chat.input.suggestion_bar_height();
		let input_h = chat.input.preferred_height();
		// suggestions + input + 1 control row; leave at least 8 rows for the browser.
		let bottom = suggest_h
			.saturating_add(input_h)
			.saturating_add(1)
			.max(2)
			.min(area.height.saturating_sub(8).max(2));
		let chunks = ratatui::layout::Layout::default()
			.direction(ratatui::layout::Direction::Vertical)
			.constraints([
				ratatui::layout::Constraint::Min(8),
				ratatui::layout::Constraint::Length(bottom),
			])
			.split(area);
		(chunks[0], chunks[1])
	}
}

impl Widget for TerminalRoot<'_> {
	fn render(mut self, area: Rect, buf: &mut Buffer) {
		// Always paint the active chat theme background first so theme switches
		// recolor the entire TUI (chat, splash, animation frames, dimmed chrome).
		let theme_bg = self.bridge.chat_state.theme.bg;
		for y in area.top()..area.bottom() {
			for x in area.left()..area.right() {
				let cell = &mut buf[(x, y)];
				cell.reset();
				cell.set_bg(theme_bg);
			}
		}

		// PRIORITY 1: Check if we're in animation mode (splash/animations carousel)
		if self.bridge.chat_state.animation.animation_mode {
			let current_anim = self.bridge.chat_state.current_animation();

			if current_anim == AnimationType::FileBrowser {
				let (file_browser_area, chat_area) =
					Self::file_browser_layout(area, &self.bridge.chat_state);

				let lua_ok = self.render_file_browser(file_browser_area, buf);
				self.render_file_browser_components(file_browser_area, buf, lua_ok);

				// Render chat at the bottom (input + slash suggestions)
				self.bridge.chat_state.render_dimmed(chat_area, area, buf);
				return;
			}

			// All other animations - render chat TUI with animations
			self.bridge.chat_state.render(area, buf);
			return;
		}

		// PRIORITY 2: Check mode for normal operation
		match self.bridge.mode {
			AppMode::Chat => {
				// Full chat mode - render chat TUI
				self.bridge.chat_state.render(area, buf);
			}
			AppMode::FilePicker => {
				let (file_browser_area, chat_area) =
					Self::file_browser_layout(area, &self.bridge.chat_state);

				let lua_ok = self.render_file_browser(file_browser_area, buf);
				self.render_file_browser_components(file_browser_area, buf, lua_ok);

				// Render dimmed chat at the bottom (input + slash suggestions)
				self.bridge.chat_state.render_dimmed(chat_area, area, buf);
			}
			AppMode::Editor => {
				// Full-screen code editor
				self.bridge.editor_adapter.render(area, buf);
			}
		}
	}
}
