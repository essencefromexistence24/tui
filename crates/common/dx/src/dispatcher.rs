use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent};
use fb_actor::Ctx;
use fb_config::keymap::Key;
use fb_macro::{act, emit, succ};
use fb_parser::app::QuitOpt;
use fb_proxy::AppProxy;
use fb_shared::{
	data::Data,
	event::{ActionCow, Event, NEED_RENDER},
};
use fb_widgets::input::InputMode;
use tracing::warn;

use crate::{
	bridge::AppMode,
	command_palette::CommandAction,
	file_browser::{Executor, Router, app::App},
	slash_commands::SlashResult,
	sound::SoundCue,
	state::{BottomPopup, CommandDialog, ScrollDrag},
};

// Helper function to format key events into readable shortcut strings
fn format_key_event(key: &KeyEvent) -> String {
	let mut parts = Vec::new();

	// For Char keys, check if Shift is needed (uppercase letters need Shift)
	let is_char = matches!(key.code, KeyCode::Char(_));
	let needs_shift = if let KeyCode::Char(c) = key.code {
		c.is_uppercase() || "!@#$%^&*()_+{}|:\"<>?".contains(c)
	} else {
		false
	};

	if key.modifiers.contains(KeyModifiers::CONTROL) {
		parts.push("Ctrl");
	}

	// Only add Shift if it's not a char, or if it's a char that needs explicit Shift
	if key.modifiers.contains(KeyModifiers::SHIFT) && (!is_char || needs_shift) {
		parts.push("Shift");
	}

	if key.modifiers.contains(KeyModifiers::ALT) {
		parts.push("Alt");
	}

	let key_str = match key.code {
		KeyCode::Char(c) => {
			// Always use uppercase for letters
			if c.is_alphabetic() { c.to_uppercase().to_string() } else { c.to_string() }
		}
		KeyCode::F(n) => format!("F{}", n),
		KeyCode::Backspace => "Backspace".to_string(),
		KeyCode::Enter => "Enter".to_string(),
		KeyCode::Left => "Left".to_string(),
		KeyCode::Right => "Right".to_string(),
		KeyCode::Up => "Up".to_string(),
		KeyCode::Down => "Down".to_string(),
		KeyCode::Home => "Home".to_string(),
		KeyCode::End => "End".to_string(),
		KeyCode::PageUp => "PageUp".to_string(),
		KeyCode::PageDown => "PageDown".to_string(),
		KeyCode::Tab => "Tab".to_string(),
		KeyCode::BackTab => "BackTab".to_string(),
		KeyCode::Delete => "Delete".to_string(),
		KeyCode::Insert => "Insert".to_string(),
		KeyCode::Esc => "Esc".to_string(),
		_ => return "Unknown".to_string(),
	};

	parts.push(&key_str);
	parts.join("+")
}

fn sound_for_input_change(key: KeyEvent) -> SoundCue {
	if matches!(key.code, KeyCode::Backspace | KeyCode::Delete) {
		SoundCue::TextDelete
	} else if matches!(key.code, KeyCode::Char(_))
		&& (key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT)
	{
		SoundCue::TextInput
	} else {
		SoundCue::SpecialKey
	}
}

/// Strip code fences / tool markup so TTS reads natural language.
fn strip_for_speech(raw: &str) -> String {
	let mut out = String::with_capacity(raw.len());
	let mut in_fence = false;
	for line in raw.lines() {
		let t = line.trim();
		if t.starts_with("```") {
			in_fence = !in_fence;
			continue;
		}
		if in_fence {
			continue;
		}
		if t.starts_with("```command") || t.starts_with("<think") || t.starts_with("</think") {
			continue;
		}
		if !out.is_empty() {
			out.push(' ');
		}
		out.push_str(t);
	}
	out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DxToolConfirmationKey {
	Confirm,
	Cancel,
	Ignore,
}

fn dx_tool_confirmation_key(key: KeyEvent) -> DxToolConfirmationKey {
	if key.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER) {
		return DxToolConfirmationKey::Ignore;
	}

	match key.code {
		KeyCode::Enter if key.modifiers.is_empty() => DxToolConfirmationKey::Confirm,
		KeyCode::Char('y') if key.modifiers.is_empty() => DxToolConfirmationKey::Confirm,
		KeyCode::Char('Y') if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT => {
			DxToolConfirmationKey::Confirm
		}
		KeyCode::Esc if key.modifiers.is_empty() => DxToolConfirmationKey::Cancel,
		KeyCode::Char('n') if key.modifiers.is_empty() => DxToolConfirmationKey::Cancel,
		KeyCode::Char('N') if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT => {
			DxToolConfirmationKey::Cancel
		}
		_ => DxToolConfirmationKey::Ignore,
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyHandled {
	Consumed,
	Unconsumed,
}

pub(super) struct Dispatcher<'a> {
	app: &'a mut App,
}

#[cfg(test)]
mod tests {
	use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

	use super::{DxToolConfirmationKey, dx_tool_confirmation_key};

	fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
		KeyEvent::new(code, modifiers)
	}

	#[test]
	fn dx_tool_confirmation_keys_accept_only_unmodified_confirm_or_cancel_keys() {
		assert_eq!(
			dx_tool_confirmation_key(key(KeyCode::Enter, KeyModifiers::NONE)),
			DxToolConfirmationKey::Confirm
		);
		assert_eq!(
			dx_tool_confirmation_key(key(KeyCode::Char('y'), KeyModifiers::NONE)),
			DxToolConfirmationKey::Confirm
		);
		assert_eq!(
			dx_tool_confirmation_key(key(KeyCode::Char('n'), KeyModifiers::NONE)),
			DxToolConfirmationKey::Cancel
		);
		assert_eq!(
			dx_tool_confirmation_key(key(KeyCode::Esc, KeyModifiers::NONE)),
			DxToolConfirmationKey::Cancel
		);
		assert_eq!(
			dx_tool_confirmation_key(key(KeyCode::Char('y'), KeyModifiers::CONTROL)),
			DxToolConfirmationKey::Ignore
		);
		assert_eq!(
			dx_tool_confirmation_key(key(KeyCode::Enter, KeyModifiers::CONTROL)),
			DxToolConfirmationKey::Ignore
		);
		assert_eq!(
			dx_tool_confirmation_key(key(KeyCode::Char('Y'), KeyModifiers::SHIFT)),
			DxToolConfirmationKey::Confirm
		);
		assert_eq!(
			dx_tool_confirmation_key(key(KeyCode::Char('N'), KeyModifiers::SHIFT)),
			DxToolConfirmationKey::Cancel
		);
		assert_eq!(
			dx_tool_confirmation_key(key(KeyCode::Enter, KeyModifiers::SHIFT)),
			DxToolConfirmationKey::Ignore
		);
	}
}

impl<'a> Dispatcher<'a> {
	#[inline]
	pub(super) fn new(app: &'a mut App) -> Self {
		Self { app }
	}

	#[inline]
	pub(super) fn dispatch(&mut self, event: Event) -> Result<()> {
		let result = match event {
			Event::Call(action) => self.dispatch_call(action),
			Event::Seq(actions) => self.dispatch_seq(actions),
			Event::Render(partial) => self.dispatch_render(partial),
			Event::Key(key) => self.dispatch_key(key),
			Event::Mouse(mouse) => self.dispatch_mouse(mouse),
			Event::Resize => self.dispatch_resize(),
			Event::Focus => self.dispatch_focus(),
			Event::Paste(str) => self.dispatch_paste(str),
			Event::Timer => self.dispatch_timer(),
		};

		if let Err(err) = result {
			warn!("Event dispatch error: {err:?}");
			self.app.bridge.chat_state.notification_manager.notify_simple(
				"Dispatch Error",
				format!("{err}"),
				crate::notifications::NotificationType::Error,
			);
		}
		Ok(())
	}

	#[inline]
	fn dispatch_call(&mut self, action: ActionCow) -> Result<Data> {
		Executor::new(self.app).execute(action)
	}

	#[inline]
	fn dispatch_seq(&mut self, mut actions: Vec<ActionCow>) -> Result<Data> {
		if let Some(last) = actions.pop() {
			self.dispatch_call(last)?;
		}
		if !actions.is_empty() {
			emit!(Seq(actions));
		}
		succ!();
	}

	#[inline]
	fn dispatch_render(&mut self, partial: bool) -> Result<Data> {
		if partial {
			_ = NEED_RENDER.compare_exchange(0, 2, Ordering::Relaxed, Ordering::Relaxed);
		} else {
			NEED_RENDER.store(1, Ordering::Relaxed);
		}
		succ!()
	}

	fn close_tachyon_menu(&mut self) {
		self.app.bridge.chat_state.clear_pending_dx_tool_confirmation();
		self.app.bridge.chat_state.menu_is_closing = true;
		self.app.bridge.chat_state.menu.pick_closing_effect();
		self.app.bridge.chat_state.show_tachyon_menu = false;
		self.app.bridge.chat_state.play_sound(SoundCue::MenuClose);
	}

	fn dispatch_dx_tool_action(&mut self, action: crate::menu::DxToolAction) -> Result<Data> {
		match action.kind {
			crate::menu::DxToolActionKind::StageInInput => {
				self.app.bridge.chat_state.stage_dx_command(action.command);
				self.app.bridge.chat_state.play_sound(SoundCue::Confirm);
				self.close_tachyon_menu();
			}
			crate::menu::DxToolActionKind::CopyToClipboard => {
				let toast = match cli_clipboard::set_contents(action.command.to_string()) {
					Ok(()) => format!("Copied DX command: {}", action.command),
					Err(error) => format!("Clipboard unavailable: {error}"),
				};
				self.app.bridge.chat_state.show_toast(toast);
				self.app.bridge.chat_state.play_sound(SoundCue::Confirm);
				self.close_tachyon_menu();
			}
			crate::menu::DxToolActionKind::ConfirmThenStage => {
				self.app.bridge.chat_state.request_dx_tool_confirmation(action);
				self.app.bridge.chat_state.play_sound(SoundCue::Toggle);
			}
		}

		NEED_RENDER.store(1, Ordering::Relaxed);
		succ!()
	}

	fn handle_priority_key(&mut self, key: KeyEvent) -> Result<KeyHandled> {
		use crate::tools::PermissionDecision;
		use crossterm::event::KeyCode;

		// PRIORITY 0-branch: branch picker modal
		if self.app.bridge.chat_state.branch_picker.open
			&& self.app.bridge.chat_state.handle_branch_picker_key(key.code)
		{
			NEED_RENDER.store(1, Ordering::Relaxed);
			return Ok(KeyHandled::Consumed);
		}

		// Ctrl+B → open branch picker
		if key.modifiers.contains(KeyModifiers::CONTROL)
			&& matches!(key.code, KeyCode::Char('b') | KeyCode::Char('B'))
		{
			self.app.bridge.chat_state.open_branch_picker();
			NEED_RENDER.store(1, Ordering::Relaxed);
			return Ok(KeyHandled::Consumed);
		}

		// PRIORITY 0-pty: Interactive terminal attached — keys go to shell stdin
		if self.app.bridge.chat_state.pty_host.is_attached() {
			match key.code {
				KeyCode::Esc if key.modifiers.is_empty() => {
					self.app.bridge.chat_state.pty_host.detach_all();
					self.app.bridge.chat_state.sync_pty_parts_into_messages();
					self.app.bridge.chat_state.show_toast("Terminal detached".into());
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				KeyCode::Enter => {
					let _ = self.app.bridge.chat_state.pty_host.write_attached("\n");
					self.app.bridge.chat_state.sync_pty_parts_into_messages();
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				KeyCode::Backspace => {
					let _ = self.app.bridge.chat_state.pty_host.write_attached("\x7f");
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				KeyCode::Char(c) => {
					let mut s = c.to_string();
					if key.modifiers.contains(KeyModifiers::CONTROL) {
						// Ctrl+C etc.
						if c == 'c' {
							s = "\x03".into();
						} else if c == 'd' {
							s = "\x04".into();
						}
					}
					let _ = self.app.bridge.chat_state.pty_host.write_attached(&s);
					self.app.bridge.chat_state.sync_pty_parts_into_messages();
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				_ => {}
			}
		}

		// PRIORITY 0a: Pending agent tool permission (y / a / n / Esc)
		if self.app.bridge.chat_state.permission_hub.pending().is_some() && key.modifiers.is_empty() {
			match key.code {
				KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
					self.app.bridge.chat_state.reply_permission(PermissionDecision::AllowOnce);
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				KeyCode::Char('a') | KeyCode::Char('A') => {
					self.app.bridge.chat_state.reply_permission(PermissionDecision::AllowAlways);
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
					self.app.bridge.chat_state.reply_permission(PermissionDecision::Deny);
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				_ => {}
			}
		}

		// PRIORITY 0b: Pending question dock
		if self.app.bridge.chat_state.question_hub.pending().is_some() && key.modifiers.is_empty() {
			match key.code {
				KeyCode::Up | KeyCode::Char('k') => {
					self.app.bridge.chat_state.question_hub.move_selection(-1);
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				KeyCode::Down | KeyCode::Char('j') => {
					self.app.bridge.chat_state.question_hub.move_selection(1);
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				KeyCode::Enter => {
					if let Some(ans) = self.app.bridge.chat_state.question_hub.confirm() {
						self.app.bridge.chat_state.show_toast(format!("Answered: {ans}"));
					}
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				KeyCode::Esc => {
					self.app.bridge.chat_state.question_hub.reject();
					self.app.bridge.chat_state.show_toast("Question dismissed".into());
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				_ => {}
			}
		}

		// PRIORITY 0c: Ctrl+C while loading -> interrupt (first Ctrl+C soft-stops generation)
		if key.code == KeyCode::Char('c')
			&& key.modifiers.contains(KeyModifiers::CONTROL)
			&& self.app.bridge.chat_state.is_loading
		{
			self.app.bridge.chat_state.interrupt_generation();
			NEED_RENDER.store(1, Ordering::Relaxed);
			return Ok(KeyHandled::Consumed);
		}

		Ok(KeyHandled::Unconsumed)
	}

	fn handle_editor_key(&mut self, key: KeyEvent) -> Result<KeyHandled> {
		use crossterm::event::KeyCode;

		// Editor mode: intercept all keys
		if self.app.bridge.mode == AppMode::Editor {
			// Escape → switch back to Splash
			if key.code == KeyCode::Esc && key.modifiers.is_empty() {
				self.app.bridge.mode = AppMode::Chat;
				self.app.bridge.chat_state.animation.animation_mode = true;
				self.app.bridge.chat_state.animation.current_animation_index = 0;
				self.app.bridge.chat_state.restart_current_animation();
				NEED_RENDER.store(1, Ordering::Relaxed);
				return Ok(KeyHandled::Consumed);
			}

			// Forward all other keys to the editor
			if self.app.bridge.editor_adapter.is_initialized() {
				let needs_render =
					self.app.bridge.editor_adapter.handle_event(crossterm::event::Event::Key(key))?;
				if needs_render {
					NEED_RENDER.store(1, Ordering::Relaxed);
				}
			}
			return Ok(KeyHandled::Consumed);
		}

		Ok(KeyHandled::Unconsumed)
	}

	fn handle_screen_nav_key(&mut self, key: KeyEvent) -> Result<KeyHandled> {
		use crossterm::event::KeyCode;

		let cs = &mut self.app.bridge.chat_state;

		// FilePicker mode: Esc → back to Splash
		if self.app.bridge.mode == AppMode::FilePicker {
			if key.code == KeyCode::Esc && key.modifiers.is_empty() {
				self.app.bridge.mode = AppMode::Chat;
				cs.animation.animation_mode = true;
				cs.animation.current_animation_index = 0;
				cs.restart_current_animation();
				NEED_RENDER.store(1, Ordering::Relaxed);
				return Ok(KeyHandled::Consumed);
			}
			return Ok(KeyHandled::Unconsumed);
		}

		let anim_mode = cs.animation.animation_mode;
		let on_splash = anim_mode && cs.current_animation() == crate::AnimationType::Splash;

		// When command menu is open, Esc closes it
		if cs.show_tachyon_menu || cs.menu_is_closing {
			if key.code == KeyCode::Esc && key.modifiers.is_empty() {
				cs.menu_is_closing = true;
				cs.menu.pick_closing_effect();
				cs.show_tachyon_menu = false;
				NEED_RENDER.store(1, Ordering::Relaxed);
				return Ok(KeyHandled::Consumed);
			}
			// Let the menu handler process navigation while open
			return Ok(KeyHandled::Unconsumed);
		}

		match key.code {
			KeyCode::Up if key.modifiers.is_empty() && on_splash => {
				// Up arrow on splash → open command menu
				cs.menu_is_closing = false;
				cs.show_tachyon_menu = true;
				cs.menu.pick_opening_effect();
				cs.play_sound(SoundCue::MenuOpen);
				NEED_RENDER.store(1, Ordering::Relaxed);
				Ok(KeyHandled::Consumed)
			}
			KeyCode::Down if key.modifiers.is_empty() && anim_mode && on_splash => {
				// Down arrow on splash → enter animation carousel (skip to first non-splash)
				let all = crate::AnimationType::all();
				if let Some(idx) = all.iter().position(|a| *a != crate::AnimationType::Splash) {
					cs.animation.current_animation_index = idx;
				}
				cs.restart_current_animation();
				NEED_RENDER.store(1, Ordering::Relaxed);
				Ok(KeyHandled::Consumed)
			}
			KeyCode::Left if key.modifiers.is_empty() && on_splash => {
				// Left arrow from splash → FileBrowser
				cs.animation.animation_mode = false;
				self.app.bridge.mode = AppMode::FilePicker;
				NEED_RENDER.store(1, Ordering::Relaxed);
				Ok(KeyHandled::Consumed)
			}
			KeyCode::Right if key.modifiers.is_empty() && on_splash => {
				// Right arrow from splash → Editor
				cs.animation.animation_mode = false;
				self.app.bridge.mode = AppMode::Editor;
				self.app.bridge.editor_adapter.schedule_init();
				NEED_RENDER.store(1, Ordering::Relaxed);
				Ok(KeyHandled::Consumed)
			}
			_ => Ok(KeyHandled::Unconsumed),
		}
	}

	fn handle_menu_key(&mut self, key: KeyEvent) -> Result<KeyHandled> {
		use crossterm::event::KeyCode;

		// PRIORITY 1: Global menu navigation keys - work when menu is visible on ANY screen
		if self.app.bridge.chat_state.show_tachyon_menu {
			if self.app.bridge.chat_state.pending_dx_tool_confirmation.is_some() {
				match dx_tool_confirmation_key(key) {
					DxToolConfirmationKey::Confirm => {
						if self.app.bridge.chat_state.confirm_pending_dx_tool().is_some() {
							self.app.bridge.chat_state.play_sound(SoundCue::Confirm);
							self.close_tachyon_menu();
						}
						NEED_RENDER.store(1, Ordering::Relaxed);
						return Ok(KeyHandled::Consumed);
					}
					DxToolConfirmationKey::Cancel => {
						self.app.bridge.chat_state.cancel_pending_dx_tool_confirmation();
						self.app.bridge.chat_state.play_sound(SoundCue::Navigate);
						NEED_RENDER.store(1, Ordering::Relaxed);
						return Ok(KeyHandled::Consumed);
					}
					DxToolConfirmationKey::Ignore => {
						NEED_RENDER.store(1, Ordering::Relaxed);
						return Ok(KeyHandled::Consumed);
					}
				}
			}

			// Check if we're in recording mode in keyboard shortcuts submenu
			if self.app.bridge.chat_state.menu.recording_mode
				&& self.app.bridge.chat_state.menu.current_submenu == Some(1)
				&& let Some(action_index) = self.app.bridge.chat_state.menu.get_selected_shortcut_index()
			{
				let shortcut = format_key_event(&key);

				if !matches!(
					key.code,
					KeyCode::Up
						| KeyCode::Down
						| KeyCode::PageUp
						| KeyCode::PageDown
						| KeyCode::Home
						| KeyCode::End
						| KeyCode::Esc
						| KeyCode::Enter
						| KeyCode::Char('j')
						| KeyCode::Char('k')
						| KeyCode::Char('g')
						| KeyCode::Char('G')
				) {
					self.app.bridge.chat_state.menu.update_keyboard_shortcut(action_index, shortcut);
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
			}

			match key.code {
				KeyCode::Up | KeyCode::Char('k') => {
					self.app.bridge.chat_state.menu.select_prev_menu_item();
					self.app.bridge.chat_state.play_sound(SoundCue::Navigate);
					if let Some(theme_name) = self.app.bridge.chat_state.menu.get_highlighted_theme_name() {
						self
							.app
							.bridge
							.chat_state
							.apply_theme(&theme_name, self.app.bridge.chat_state.theme_mode);
					}
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				KeyCode::Down | KeyCode::Char('j') => {
					self.app.bridge.chat_state.menu.select_next_menu_item();
					self.app.bridge.chat_state.play_sound(SoundCue::Navigate);
					if let Some(theme_name) = self.app.bridge.chat_state.menu.get_highlighted_theme_name() {
						self
							.app
							.bridge
							.chat_state
							.apply_theme(&theme_name, self.app.bridge.chat_state.theme_mode);
					}
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				KeyCode::PageUp => {
					self.app.bridge.chat_state.menu.page_up(10);
					self.app.bridge.chat_state.play_sound(SoundCue::Navigate);
					if let Some(theme_name) = self.app.bridge.chat_state.menu.get_highlighted_theme_name() {
						self
							.app
							.bridge
							.chat_state
							.apply_theme(&theme_name, self.app.bridge.chat_state.theme_mode);
					}
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				KeyCode::PageDown => {
					self.app.bridge.chat_state.menu.page_down(10);
					self.app.bridge.chat_state.play_sound(SoundCue::Navigate);
					if let Some(theme_name) = self.app.bridge.chat_state.menu.get_highlighted_theme_name() {
						self
							.app
							.bridge
							.chat_state
							.apply_theme(&theme_name, self.app.bridge.chat_state.theme_mode);
					}
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				KeyCode::Home | KeyCode::Char('g') => {
					self.app.bridge.chat_state.menu.jump_to_top();
					self.app.bridge.chat_state.play_sound(SoundCue::Navigate);
					if let Some(theme_name) = self.app.bridge.chat_state.menu.get_highlighted_theme_name() {
						self
							.app
							.bridge
							.chat_state
							.apply_theme(&theme_name, self.app.bridge.chat_state.theme_mode);
					}
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				KeyCode::End | KeyCode::Char('G') => {
					self.app.bridge.chat_state.menu.jump_to_bottom();
					self.app.bridge.chat_state.play_sound(SoundCue::Navigate);
					if let Some(theme_name) = self.app.bridge.chat_state.menu.get_highlighted_theme_name() {
						self
							.app
							.bridge
							.chat_state
							.apply_theme(&theme_name, self.app.bridge.chat_state.theme_mode);
					}
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				KeyCode::Char('t') | KeyCode::Char('T')
					if self.app.bridge.chat_state.menu.current_submenu == Some(0) =>
				{
					self.app.bridge.chat_state.toggle_theme_mode();
					self.app.bridge.chat_state.play_sound(SoundCue::Toggle);
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				KeyCode::Enter => {
					if self.app.bridge.chat_state.menu.is_toggle_mode_selected() {
						self.app.bridge.chat_state.toggle_theme_mode();
						self.app.bridge.chat_state.play_sound(SoundCue::Toggle);
						NEED_RENDER.store(1, Ordering::Relaxed);
						return Ok(KeyHandled::Consumed);
					}

					if self.app.bridge.chat_state.menu.is_toggle_recording_selected() {
						self.app.bridge.chat_state.menu.toggle_recording_mode();
						self.app.bridge.chat_state.play_sound(SoundCue::Toggle);
						NEED_RENDER.store(1, Ordering::Relaxed);
						return Ok(KeyHandled::Consumed);
					}

					if let Some(action) = self.app.bridge.chat_state.menu.selected_dx_tool_action() {
						return self.dispatch_dx_tool_action(action).map(|_| KeyHandled::Consumed);
					}

					if self.app.bridge.chat_state.menu.current_submenu == Some(13) {
						match self.app.bridge.chat_state.menu.selected_menu_item {
							1 => {
								let cs = &mut self.app.bridge.chat_state;
								cs.ui.dialog = CommandDialog::UserName;
								cs.ui.dialog_input = cs.user_display_name.clone();
								cs.menu_is_closing = true;
								cs.menu.pick_closing_effect();
								cs.show_tachyon_menu = false;
								cs.play_sound(SoundCue::Confirm);
								NEED_RENDER.store(1, Ordering::Relaxed);
								return Ok(KeyHandled::Consumed);
							}
							2 => {
								let cs = &mut self.app.bridge.chat_state;
								cs.open_popup(BottomPopup::AgentMode);
								cs.menu_is_closing = true;
								cs.menu.pick_closing_effect();
								cs.show_tachyon_menu = false;
								cs.play_sound(SoundCue::Confirm);
								NEED_RENDER.store(1, Ordering::Relaxed);
								return Ok(KeyHandled::Consumed);
							}
							_ => {}
						}
					}

					if self.app.bridge.chat_state.menu.is_dynamic_models()
						|| self.app.bridge.chat_state.menu.is_dynamic_channels()
					{
						if self.app.bridge.chat_state.activate_dynamic_menu_selection() {
							self.app.bridge.chat_state.play_sound(SoundCue::Confirm);
						} else {
							self.app.bridge.chat_state.play_sound(SoundCue::Navigate);
						}
						NEED_RENDER.store(1, Ordering::Relaxed);
						return Ok(KeyHandled::Consumed);
					}

					let theme_name = self.app.bridge.chat_state.menu.get_selected_theme_name();

					let _should_close = !self.app.bridge.chat_state.menu.select_current_item();
					self.app.bridge.chat_state.play_sound(SoundCue::Confirm);

					if theme_name.is_some() {
						self.app.bridge.chat_state.menu_is_closing = true;
						self.app.bridge.chat_state.menu.pick_closing_effect();
						self.app.bridge.chat_state.show_tachyon_menu = false;
						self.app.bridge.chat_state.play_sound(SoundCue::MenuClose);
					}

					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				KeyCode::Esc => {
					if self.app.bridge.chat_state.menu.is_dynamic_models()
						|| self.app.bridge.chat_state.menu.is_dynamic_channels()
					{
						self.app.bridge.chat_state.menu_is_closing = true;
						self.app.bridge.chat_state.menu.pick_closing_effect();
						self.app.bridge.chat_state.show_tachyon_menu = false;
						self.app.bridge.chat_state.menu.custom_title = None;
						self.app.bridge.chat_state.menu.current_submenu = None;
						self.app.bridge.chat_state.play_sound(SoundCue::MenuClose);
						NEED_RENDER.store(1, Ordering::Relaxed);
						return Ok(KeyHandled::Consumed);
					}
					if self.app.bridge.chat_state.menu.current_submenu.is_some() {
						self.app.bridge.chat_state.menu.go_back_to_main();
						self.app.bridge.chat_state.play_sound(SoundCue::Navigate);
						NEED_RENDER.store(1, Ordering::Relaxed);
						return Ok(KeyHandled::Consumed);
					} else {
						self.app.bridge.chat_state.menu_is_closing = true;
						self.app.bridge.chat_state.menu.pick_closing_effect();
						self.app.bridge.chat_state.show_tachyon_menu = false;
						self.app.bridge.chat_state.play_sound(SoundCue::MenuClose);
						NEED_RENDER.store(1, Ordering::Relaxed);
						return Ok(KeyHandled::Consumed);
					}
				}
				_ => {}
			}
		}

		Ok(KeyHandled::Unconsumed)
	}

	fn handle_animation_key(&mut self, key: KeyEvent) -> Result<KeyHandled> {
		use crossterm::event::KeyCode;

		// If in animation mode, handle navigation for carousel (non-splash) animations
		if self.app.bridge.chat_state.animation.animation_mode {
			let current_anim = self.app.bridge.chat_state.current_animation();

			// Splash navigation is handled by handle_screen_nav_key — only handle
			// carousel navigation when NOT on Splash.
			if current_anim == crate::AnimationType::Splash {
				return Ok(KeyHandled::Unconsumed);
			}

			let all_animations = crate::AnimationType::all();
			let carousel = crate::AnimationType::carousel_animations();

			match key.code {
				KeyCode::Esc if key.modifiers.is_empty() => {
					// Escape → back to Splash
					self.app.bridge.chat_state.animation.current_animation_index = 0;
					self.app.bridge.chat_state.play_sound(SoundCue::Navigate);
					self.app.bridge.chat_state.restart_current_animation();
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				KeyCode::Left if key.modifiers.is_empty() => {
					// Previous carousel animation
					if let Some(carousel_idx) = carousel.iter().position(|a| *a == current_anim) {
						let prev = if carousel_idx == 0 { carousel.len() - 1 } else { carousel_idx - 1 };
						if let Some(idx) = all_animations.iter().position(|a| *a == carousel[prev]) {
							self.app.bridge.chat_state.animation.current_animation_index = idx;
						}
					} else if current_anim == crate::AnimationType::FileBrowser {
						self.app.bridge.chat_state.animation.current_animation_index = 0;
					}
					self.app.bridge.chat_state.play_sound(SoundCue::Navigate);
					self.app.bridge.chat_state.restart_current_animation();
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				KeyCode::Right if key.modifiers.is_empty() => {
					// Next carousel animation (cycle forward, never back to splash)
					let carousel = crate::AnimationType::carousel_animations();
					let all = crate::AnimationType::all();
					if let Some(carousel_idx) = carousel.iter().position(|a| *a == current_anim) {
						let next = (carousel_idx + 1) % carousel.len();
						if let Some(idx) = all.iter().position(|a| *a == carousel[next]) {
							self.app.bridge.chat_state.animation.current_animation_index = idx;
						}
					}
					self.app.bridge.chat_state.play_sound(SoundCue::Navigate);
					self.app.bridge.chat_state.restart_current_animation();
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				KeyCode::Up => {
					if current_anim != crate::AnimationType::FileBrowser {
						let anim = &mut self.app.bridge.chat_state.animation;
						if anim.intro_animation == current_anim {
							anim.intro_animation = crate::AnimationType::Splash;
							self.app.bridge.chat_state.show_toast("Intro unset".to_string());
						} else {
							anim.intro_animation = current_anim;
							self.app.bridge.chat_state.show_toast("✓ Intro set".to_string());
						}
						self.app.bridge.chat_state.play_sound(SoundCue::Confirm);
						NEED_RENDER.store(1, Ordering::Relaxed);
					}
					return Ok(KeyHandled::Consumed);
				}
				KeyCode::Down => {
					if current_anim != crate::AnimationType::FileBrowser {
						let anim = &mut self.app.bridge.chat_state.animation;
						if anim.outro_animation == current_anim {
							anim.outro_animation = crate::AnimationType::Train;
							self.app.bridge.chat_state.show_toast("Outro unset".to_string());
						} else {
							anim.outro_animation = current_anim;
							self.app.bridge.chat_state.show_toast("✓ Outro set".to_string());
						}
						self.app.bridge.chat_state.play_sound(SoundCue::Confirm);
						NEED_RENDER.store(1, Ordering::Relaxed);
					}
					return Ok(KeyHandled::Consumed);
				}
				_ => {}
			}
		}

		Ok(KeyHandled::Unconsumed)
	}

	fn handle_session_key(&mut self, key: KeyEvent) -> Result<KeyHandled> {
		use crossterm::event::KeyCode;

		// Soft session-exit screen (summary + `dx continue`)
		if self.app.bridge.chat_state.session.show_session_screen {
			match key.code {
				KeyCode::Enter => {
					let cs = &mut self.app.bridge.chat_state;
					cs.session.show_session_screen = false;
					cs.animation.exit_after_outro = false;
					cs.session.session_exit_deadline = None;
					cs.session.quit_after_session_reveal = false;
					cs.session.force_clear_frames = 0;
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				KeyCode::Char('q') | KeyCode::Char('Q') => {
					let cs = &mut self.app.bridge.chat_state;
					if cs.animation.exit_after_outro {
						cs.finish_outro_exit();
					} else {
						cs.finish_soft_exit_public();
					}
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
					self.app.bridge.chat_state.request_exit();
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				KeyCode::Char('y') | KeyCode::Char('Y') => {
					let cmd = self.app.bridge.chat_state.continue_command_line();
					if cli_clipboard::set_contents(cmd.clone()).is_ok() {
						self.app.bridge.chat_state.show_toast(format!("Copied · {cmd}"));
					}
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				_ => {
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
			}
		}

		Ok(KeyHandled::Unconsumed)
	}

	fn handle_voice_key(&mut self, key: KeyEvent) -> Result<KeyHandled> {
		use crossterm::event::KeyCode;

		// Voice STT/TTS panel
		if self.app.bridge.chat_state.voice_state.panel.open {
			match key.code {
				KeyCode::Esc => {
					self.app.bridge.chat_state.voice_state.panel.close();
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				KeyCode::Tab => {
					self.app.bridge.chat_state.voice_state.panel.mode =
						self.app.bridge.chat_state.voice_state.panel.mode.toggle();
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				KeyCode::Up | KeyCode::Char('k') => {
					self.app.bridge.chat_state.voice_state.panel.cursor =
						self.app.bridge.chat_state.voice_state.panel.cursor.saturating_sub(1);
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				KeyCode::Down | KeyCode::Char('j') => {
					let max =
						self.app.bridge.chat_state.voice_state.panel.menu_rows().len().saturating_sub(1);
					self.app.bridge.chat_state.voice_state.panel.cursor =
						(self.app.bridge.chat_state.voice_state.panel.cursor + 1).min(max);
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				KeyCode::Enter => {
					let mode = self.app.bridge.chat_state.voice_state.panel.mode;
					let cursor = self.app.bridge.chat_state.voice_state.panel.cursor;
					match mode {
						crate::voice::VoiceMode::Stt => match cursor {
							1 => {
								let path = self.app.bridge.chat_state.voice_state.panel.input_path.clone();
								let handle = tokio::runtime::Handle::current();
								let tx = self.app.bridge.chat_state.agent_tx.clone();
								self.app.bridge.chat_state.voice_state.panel.status = "Transcribing…".into();
								handle.spawn(async move {
									match crate::voice::transcribe_file(&path).await {
										Ok(text) => {
											let _ = tx.send(format!("\n__VOICE_STT__\n{text}"));
										}
										Err(e) => {
											let _ = tx.send(format!("\n__VOICE_ERR__\n{e}"));
										}
									}
								});
							}
							2 => {
								let t = self.app.bridge.chat_state.voice_state.panel.last_transcript.clone();
								if !t.is_empty() {
									self.app.bridge.chat_state.input.replace_content(&t);
									self.app.bridge.chat_state.voice_state.panel.close();
									self.app.bridge.chat_state.show_toast("Transcript inserted".into());
								}
							}
							_ => {
								self.app.bridge.chat_state.voice_state.panel.mode =
									self.app.bridge.chat_state.voice_state.panel.mode.toggle();
							}
						},
						crate::voice::VoiceMode::Tts => match cursor {
							1 => {
								let mut text = self.app.bridge.chat_state.voice_state.panel.tts_text.clone();
								if text.is_empty() {
									text = self
										.app
										.bridge
										.chat_state
										.messages
										.iter()
										.rev()
										.find(|m| m.role == crate::components::MessageRole::Assistant)
										.map(|m| m.content.clone())
										.unwrap_or_default();
								}
								let handle = tokio::runtime::Handle::current();
								let tx = self.app.bridge.chat_state.agent_tx.clone();
								self.app.bridge.chat_state.voice_state.panel.status = "Synthesizing…".into();
								handle.spawn(async move {
									match crate::voice::synthesize_to_file(&text).await {
										Ok(path) => {
											let _ = tx.send(format!("\n__VOICE_TTS__\n{}", path.display()));
										}
										Err(e) => {
											let _ = tx.send(format!("\n__VOICE_ERR__\n{e}"));
										}
									}
								});
							}
							_ => {
								self.app.bridge.chat_state.voice_state.panel.mode =
									self.app.bridge.chat_state.voice_state.panel.mode.toggle();
							}
						},
					}
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				KeyCode::Char(c)
					if !key.modifiers.contains(KeyModifiers::CONTROL)
						&& self.app.bridge.chat_state.voice_state.panel.cursor == 1 =>
				{
					match self.app.bridge.chat_state.voice_state.panel.mode {
						crate::voice::VoiceMode::Stt => {
							self.app.bridge.chat_state.voice_state.panel.input_path.push(c);
						}
						crate::voice::VoiceMode::Tts => {
							self.app.bridge.chat_state.voice_state.panel.tts_text.push(c);
						}
					}
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				KeyCode::Backspace if self.app.bridge.chat_state.voice_state.panel.cursor == 1 => {
					match self.app.bridge.chat_state.voice_state.panel.mode {
						crate::voice::VoiceMode::Stt => {
							self.app.bridge.chat_state.voice_state.panel.input_path.pop();
						}
						crate::voice::VoiceMode::Tts => {
							self.app.bridge.chat_state.voice_state.panel.tts_text.pop();
						}
					}
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				_ => {
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
			}
		}

		Ok(KeyHandled::Unconsumed)
	}

	fn handle_dialog_key(&mut self, key: KeyEvent) -> Result<KeyHandled> {
		use crossterm::event::KeyCode;

		// Slash-command dialogs (sessions, help, themes, rename, ...)
		if self.app.bridge.chat_state.ui.dialog != CommandDialog::None {
			let dialog = self.app.bridge.chat_state.ui.dialog;
			let is_text = matches!(
				dialog,
				CommandDialog::Rename
					| CommandDialog::UserName
					| CommandDialog::Export
					| CommandDialog::Note
					| CommandDialog::Move
			);
			match key.code {
				KeyCode::Esc => {
					self.app.bridge.chat_state.close_dialog();
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				KeyCode::Enter => {
					self.app.bridge.chat_state.activate_dialog_selection();
					self.app.bridge.chat_state.play_sound(SoundCue::Confirm);
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				KeyCode::Up | KeyCode::Char('k') if !is_text => {
					self.app.bridge.chat_state.dialog_move(-1);
					self.app.bridge.chat_state.play_sound(SoundCue::Navigate);
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				KeyCode::Down | KeyCode::Char('j') if !is_text => {
					self.app.bridge.chat_state.dialog_move(1);
					self.app.bridge.chat_state.play_sound(SoundCue::Navigate);
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				KeyCode::Tab if dialog == CommandDialog::Export => {
					let cs = &mut self.app.bridge.chat_state;
					match (cs.ui.export_include_thinking, cs.ui.export_include_tools) {
						(true, true) => {
							cs.ui.export_include_thinking = true;
							cs.ui.export_include_tools = false;
						}
						(true, false) => {
							cs.ui.export_include_thinking = false;
							cs.ui.export_include_tools = true;
						}
						(false, true) => {
							cs.ui.export_include_thinking = false;
							cs.ui.export_include_tools = false;
						}
						(false, false) => {
							cs.ui.export_include_thinking = true;
							cs.ui.export_include_tools = true;
						}
					}
					cs.show_toast(format!(
						"Export options · thinking={} tools={}",
						cs.ui.export_include_thinking, cs.ui.export_include_tools
					));
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				KeyCode::Char('n') if dialog == CommandDialog::Sessions => {
					self.app.bridge.chat_state.cmd_new_session();
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				KeyCode::Backspace if is_text => {
					self.app.bridge.chat_state.ui.dialog_input.pop();
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				KeyCode::Char(c) if is_text && !key.modifiers.contains(KeyModifiers::CONTROL) => {
					self.app.bridge.chat_state.ui.dialog_input.push(c);
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				_ => {
					if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
						// fall through
					} else {
						NEED_RENDER.store(1, Ordering::Relaxed);
						return Ok(KeyHandled::Consumed);
					}
				}
			}
		}

		Ok(KeyHandled::Unconsumed)
	}

	fn handle_diff_key(&mut self, key: KeyEvent) -> Result<KeyHandled> {
		use crossterm::event::KeyCode;

		// Full-screen differ navigation — but never steal typing / slash suggestions
		// from the shared chat input chrome at the bottom.
		if self.app.bridge.chat_state.diff_state.open {
			let has_sugg = self.app.bridge.chat_state.input.has_suggestions();
			let input_busy = has_sugg || !self.app.bridge.chat_state.input.content.is_empty();
			let mods = key.modifiers;
			let plain = mods.is_empty() || mods == KeyModifiers::SHIFT;

			// Slash/suggestion UX and free typing always fall through to chat input.
			if has_sugg {
				match key.code {
					KeyCode::Up
					| KeyCode::Down
					| KeyCode::Tab
					| KeyCode::Enter
					| KeyCode::Esc
					| KeyCode::Char(_)
					| KeyCode::Backspace
					| KeyCode::Delete => return Ok(KeyHandled::Unconsumed),
					_ => {}
				}
			}
			if plain {
				match key.code {
					KeyCode::Char('/') | KeyCode::Char('@') => return Ok(KeyHandled::Unconsumed),
					KeyCode::Char(_) | KeyCode::Backspace | KeyCode::Delete => {
						return Ok(KeyHandled::Unconsumed);
					}
					// Enter submits chat when the input has content.
					KeyCode::Enter if input_busy => return Ok(KeyHandled::Unconsumed),
					_ => {}
				}
			}

			let cs = &mut self.app.bridge.chat_state;
			let viewport = cs.ui.chat_list_area.height.max(10) as usize;
			match key.code {
				KeyCode::Esc => {
					// Esc: clear suggestions first (handled above when open); else close diff.
					if !cs.input.content.is_empty() {
						// Prefer clearing input over closing when user was typing.
						return Ok(KeyHandled::Unconsumed);
					}
					cs.diff_state.close();
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				KeyCode::Char('q') if plain && !input_busy => {
					cs.diff_state.close();
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				KeyCode::Char('r') if plain && !input_busy => {
					cs.diff_state.refresh();
					cs.show_toast("Diff refreshed".into());
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				KeyCode::Tab if !has_sugg => {
					cs.diff_state.focus_tree = !cs.diff_state.focus_tree;
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				KeyCode::Up | KeyCode::Char('k') if plain && !has_sugg => {
					if cs.diff_state.focus_tree {
						cs.diff_state.move_tree_cursor(-1);
					} else {
						cs.diff_state.scroll_diff_by(-1, viewport);
					}
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				KeyCode::Down | KeyCode::Char('j') if plain && !has_sugg => {
					if cs.diff_state.focus_tree {
						cs.diff_state.move_tree_cursor(1);
					} else {
						cs.diff_state.scroll_diff_by(1, viewport);
					}
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				KeyCode::PageUp => {
					cs.diff_state.scroll_diff_by(-(viewport as i32 / 2).max(1), viewport);
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				KeyCode::PageDown => {
					cs.diff_state.scroll_diff_by((viewport as i32 / 2).max(1), viewport);
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				KeyCode::Enter | KeyCode::Char(' ') if plain && !input_busy => {
					cs.diff_state.activate_tree_cursor();
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				KeyCode::Left | KeyCode::Right if plain && !has_sugg => {
					cs.diff_state.focus_tree = key.code == KeyCode::Left;
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				// Everything else (typing, paste chords, etc.) → chat input.
				_ => return Ok(KeyHandled::Unconsumed),
			}
		}

		Ok(KeyHandled::Unconsumed)
	}

	fn handle_popup_key(&mut self, key: KeyEvent) -> Result<KeyHandled> {
		use crossterm::event::KeyCode;

		// Bottom-bar popup menus (mode / runtime / models / channels)
		if self.app.bridge.chat_state.ui.bottom_popup != BottomPopup::None {
			// PlanOptions uses the wizard UI (tabbed multi-step)
			if self.app.bridge.chat_state.ui.bottom_popup == BottomPopup::PlanOptions {
				return self.handle_plan_wizard_key(key);
			}
			match key.code {
				KeyCode::Esc => {
					self.app.bridge.chat_state.close_popup();
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				KeyCode::Up | KeyCode::Char('k') => {
					self.app.bridge.chat_state.popup_move(-1);
					self.app.bridge.chat_state.play_sound(SoundCue::Navigate);
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				KeyCode::Down | KeyCode::Char('j') => {
					self.app.bridge.chat_state.popup_move(1);
					self.app.bridge.chat_state.play_sound(SoundCue::Navigate);
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				KeyCode::Enter => {
					self.app.bridge.chat_state.activate_popup_selection();
					self.app.bridge.chat_state.play_sound(SoundCue::Confirm);
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				_ => {}
			}
		}

		Ok(KeyHandled::Unconsumed)
	}

	fn handle_plan_wizard_key(&mut self, key: KeyEvent) -> Result<KeyHandled> {
		use crossterm::event::KeyCode;
		let cs = &mut self.app.bridge.chat_state;
		match key.code {
			KeyCode::Esc => {
				cs.plan_wizard.active = false;
				cs.close_popup();
				cs.show_toast("Plan wizard dismissed".into());
				NEED_RENDER.store(1, Ordering::Relaxed);
				Ok(KeyHandled::Consumed)
			}
			KeyCode::Up | KeyCode::Char('k') => {
				cs.plan_wizard.move_selection(-1);
				cs.play_sound(SoundCue::Navigate);
				NEED_RENDER.store(1, Ordering::Relaxed);
				Ok(KeyHandled::Consumed)
			}
			KeyCode::Down | KeyCode::Char('j') => {
				cs.plan_wizard.move_selection(1);
				cs.play_sound(SoundCue::Navigate);
				NEED_RENDER.store(1, Ordering::Relaxed);
				Ok(KeyHandled::Consumed)
			}
			KeyCode::Left | KeyCode::Char('h') => {
				cs.plan_wizard.move_tab(-1);
				cs.play_sound(SoundCue::Navigate);
				NEED_RENDER.store(1, Ordering::Relaxed);
				Ok(KeyHandled::Consumed)
			}
			KeyCode::Right | KeyCode::Char('l') => {
				cs.plan_wizard.move_tab(1);
				cs.play_sound(SoundCue::Navigate);
				NEED_RENDER.store(1, Ordering::Relaxed);
				Ok(KeyHandled::Consumed)
			}
			KeyCode::Tab => {
				cs.plan_wizard.move_tab(1);
				cs.play_sound(SoundCue::Navigate);
				NEED_RENDER.store(1, Ordering::Relaxed);
				Ok(KeyHandled::Consumed)
			}
			KeyCode::BackTab => {
				cs.plan_wizard.move_tab(-1);
				cs.play_sound(SoundCue::Navigate);
				NEED_RENDER.store(1, Ordering::Relaxed);
				Ok(KeyHandled::Consumed)
			}
			KeyCode::Enter => {
				if cs.plan_wizard.is_confirm_tab() {
					cs.plan_options = cs.plan_wizard.to_plan_options();
					let report = crate::goal_runner::run_plan_tools(&cs.plan_options);
					cs.messages.push(crate::components::Message::assistant(format!(
						"<think>\n(plan tools)\n</think>\n```command name=\"plan-tools\"\n{report}\n```"
					)));
					cs.show_toast("Plan tools attached to chat".into());
				} else {
					let confirmed = cs.plan_wizard.select_current();
					if confirmed {
						cs.plan_options = cs.plan_wizard.to_plan_options();
						let report = crate::goal_runner::run_plan_tools(&cs.plan_options);
						cs.messages.push(crate::components::Message::assistant(format!(
							"<think>\n(plan tools)\n</think>\n```command name=\"plan-tools\"\n{report}\n```"
						)));
						cs.show_toast("Plan tools attached to chat".into());
					} else {
						cs.play_sound(SoundCue::Confirm);
						NEED_RENDER.store(1, Ordering::Relaxed);
						return Ok(KeyHandled::Consumed);
					}
				}
				cs.plan_wizard.active = false;
				cs.close_popup();
				NEED_RENDER.store(1, Ordering::Relaxed);
				Ok(KeyHandled::Consumed)
			}
			_ => Ok(KeyHandled::Unconsumed),
		}
	}

	fn handle_global_key(&mut self, key: KeyEvent) -> Result<KeyHandled> {
		use crate::menu::MenuAction;
		use crossterm::event::KeyCode;

		// Model switcher: Ctrl+M cycles; Ctrl+Shift+M opens model menu
		if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('m') {
			if key.modifiers.contains(KeyModifiers::SHIFT) {
				self.app.bridge.chat_state.open_popup(BottomPopup::Models);
			} else {
				self.app.bridge.chat_state.cycle_model();
			}
			self.app.bridge.chat_state.play_sound(SoundCue::Navigate);
			NEED_RENDER.store(1, Ordering::Relaxed);
			return Ok(KeyHandled::Consumed);
		}

		// Ctrl+P -> command palette
		if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('p') {
			self.app.bridge.chat_state.command_palette.toggle();
			self.app.bridge.chat_state.play_sound(SoundCue::MenuOpen);
			NEED_RENDER.store(1, Ordering::Relaxed);
			return Ok(KeyHandled::Consumed);
		}

		// Ctrl+D -> full-screen differ (also creates/refreshes if empty)
		if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('d') {
			self.app.bridge.chat_state.open_differ();
			self.app.bridge.chat_state.play_sound(SoundCue::Confirm);
			NEED_RENDER.store(1, Ordering::Relaxed);
			return Ok(KeyHandled::Consumed);
		}

		// Ctrl+Shift+A -> agent mode menu; Ctrl+A cycle Ask/Write/Plan/Goal
		if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('a') {
			if key.modifiers.contains(KeyModifiers::SHIFT) {
				self.app.bridge.chat_state.open_popup(BottomPopup::AgentMode);
			} else {
				self.app.bridge.chat_state.cycle_agent_mode();
			}
			self.app.bridge.chat_state.play_sound(SoundCue::Navigate);
			NEED_RENDER.store(1, Ordering::Relaxed);
			return Ok(KeyHandled::Consumed);
		}

		// Ctrl+L -> toggle Local/Remote; Ctrl+Shift+L runtime menu
		if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('l') {
			if key.modifiers.contains(KeyModifiers::SHIFT) {
				self.app.bridge.chat_state.open_popup(BottomPopup::Runtime);
			} else {
				self.app.bridge.chat_state.toggle_runtime_mode();
			}
			self.app.bridge.chat_state.play_sound(SoundCue::Navigate);
			NEED_RENDER.store(1, Ordering::Relaxed);
			return Ok(KeyHandled::Consumed);
		}

		// Ctrl+Shift+C -> channels menu (dx-agent)
		if key.modifiers.contains(KeyModifiers::CONTROL)
			&& key.modifiers.contains(KeyModifiers::SHIFT)
			&& key.code == KeyCode::Char('c')
		{
			self.app.bridge.chat_state.open_popup(BottomPopup::Channels);
			self.app.bridge.chat_state.play_sound(SoundCue::MenuOpen);
			NEED_RENDER.store(1, Ordering::Relaxed);
			return Ok(KeyHandled::Consumed);
		}

		// Ctrl+S -> real mic listen + STT (live wave bars on input right)
		if key.modifiers.contains(KeyModifiers::CONTROL)
			&& !key.modifiers.contains(KeyModifiers::SHIFT)
			&& matches!(key.code, KeyCode::Char('s') | KeyCode::Char('S'))
		{
			let cs = &mut self.app.bridge.chat_state;
			match cs.voice_state.panel.toggle_listening() {
				Ok(crate::voice::ListenToggle::Started) => {
					cs.play_sound(SoundCue::Toggle);
					cs.show_toast("● Recording mic · speak · Ctrl+S again to STT".into());
				}
				Ok(crate::voice::ListenToggle::Stopped { audio: Some((samples, rate)) }) => {
					cs.play_sound(SoundCue::Confirm);
					cs.show_toast(format!(
						"Transcribing {:.1}s audio…",
						samples.len() as f32 / rate.max(1) as f32
					));
					let tx = cs.agent_tx.clone();
					if let Ok(handle) = tokio::runtime::Handle::try_current() {
						handle.spawn(async move {
							match crate::voice::transcribe_samples(samples, rate).await {
								Ok(text) => {
									let _ = tx.send(format!("\n__VOICE_STT__\n{text}"));
								}
								Err(e) => {
									let _ = tx.send(format!("\n__VOICE_ERR__\n{e}"));
								}
							}
						});
					} else {
						cs.voice_state.panel.processing = false;
						cs.show_toast("STT needs async runtime".into());
					}
				}
				Ok(crate::voice::ListenToggle::Stopped { audio: None }) => {
					cs.play_sound(SoundCue::MenuClose);
					cs.voice_state.panel.processing = false;
					cs.show_toast("No speech captured · hold longer while talking".into());
				}
				Err(e) => {
					cs.play_sound(SoundCue::MenuClose);
					cs.show_toast(format!("Mic error: {e}"));
				}
			}
			NEED_RENDER.store(1, Ordering::Relaxed);
			return Ok(KeyHandled::Consumed);
		}

		// Ctrl+T -> speak selection (input/chat) or last assistant message via Kokoro TTS
		if key.modifiers.contains(KeyModifiers::CONTROL)
			&& !key.modifiers.contains(KeyModifiers::SHIFT)
			&& matches!(key.code, KeyCode::Char('t') | KeyCode::Char('T'))
		{
			let cs = &mut self.app.bridge.chat_state;
			let text = if let Some(sel) = cs.input.get_selected_text().filter(|s| !s.trim().is_empty()) {
				sel
			} else if let Some(sel) = cs
				.selected_chat_text_exact()
				.or_else(|| cs.selected_chat_text())
				.filter(|s| !s.trim().is_empty())
			{
				sel
			} else if let Some(msg) =
				cs.messages.iter().rev().find(|m| m.role == crate::components::MessageRole::Assistant)
			{
				// Prefer plain body without heavy tool fences for speech
				let raw = msg.copy_text();
				strip_for_speech(&raw)
			} else if !cs.input.content.trim().is_empty() {
				// Fallback: speak current input draft
				cs.input.content.clone()
			} else {
				String::new()
			};
			let text = text.trim().to_string();
			if text.is_empty() {
				cs.show_toast("Nothing to speak · select text or wait for an answer".into());
				NEED_RENDER.store(1, Ordering::Relaxed);
				return Ok(KeyHandled::Consumed);
			}
			// Cap very long replies for TTS latency
			let speak = if text.chars().count() > 1_200 {
				text.chars().take(1_200).collect::<String>() + "…"
			} else {
				text
			};
			cs.voice_state.panel.speaking = true;
			cs.voice_state.panel.status = "Speaking…".into();
			cs.show_toast(format!("TTS · Kokoro · {} chars…", speak.chars().count()));
			cs.play_sound(SoundCue::Confirm);
			let tx = cs.agent_tx.clone();
			if let Ok(handle) = tokio::runtime::Handle::try_current() {
				handle.spawn(async move {
					// speak_text synthesizes AND plays
					match crate::voice::speak_text(&speak).await {
						Ok(path) => {
							let _ = tx.send(format!("\n__VOICE_TTS__\n{}", path.display()));
						}
						Err(e) => {
							let _ = tx.send(format!("\n__VOICE_ERR__\n{e}"));
						}
					}
				});
			} else {
				cs.voice_state.panel.speaking = false;
				cs.show_toast("TTS needs async runtime".into());
			}
			NEED_RENDER.store(1, Ordering::Relaxed);
			return Ok(KeyHandled::Consumed);
		}

		// Expand/collapse last assistant blocks: Alt+T thinking, Alt+C commands, Alt+S subagents
		// File tab switching: Alt+1..9 to activate tab, Alt+W to close tab
		if key.modifiers.contains(KeyModifiers::ALT) {
			match key.code {
				KeyCode::Char('t') => {
					self.app.bridge.chat_state.toggle_last_assistant_block('t');
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				KeyCode::Char('c') => {
					self.app.bridge.chat_state.toggle_last_assistant_block('c');
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				KeyCode::Char('s') => {
					self.app.bridge.chat_state.toggle_last_assistant_block('s');
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				// Alt+1..9 -> activate file tab by index
				KeyCode::Char(d @ '1'..='9') => {
					let idx = d as usize - '1' as usize;
					let tab_count = self.app.bridge.chat_state.file_tabs.len();
					if idx < tab_count {
						self.app.bridge.chat_state.file_tabs.activate_index(idx);
						NEED_RENDER.store(1, Ordering::Relaxed);
					}
					return Ok(KeyHandled::Consumed);
				}
				// Alt+W -> close active file tab
				KeyCode::Char('w') => {
					self.app.bridge.chat_state.file_tabs.close_active_tab();
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				_ => {}
			}
		}

		// Production digit shortcuts — only at first-line-left (empty input).
		// Mid-text typing must treat 0-9 and Ctrl+0-9 as normal keys, never menus.
		//   0 -> Model / provider menu (Flow -> Zen -> catalog)
		//   1 -> Channels (dx-agent social)
		//   Ctrl+9 -> load Flow GGUF models + open model menu
		//   2-9 / Ctrl+0-8 -> tachyon command palette sections
		if let KeyCode::Char(d @ '0'..='9') = key.code {
			// Command position: empty buffer (first letter of first line).
			// Suggestions alone are not enough — Ctrl must not fire mid-message.
			let at_command_position = self.app.bridge.chat_state.input.content.is_empty();
			let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
			let alt = key.modifiers.contains(KeyModifiers::ALT);
			let shift = key.modifiers.contains(KeyModifiers::SHIFT);
			let bare = !ctrl && !alt && !shift;
			if at_command_position && (ctrl || bare) {
				// Ctrl+9 -> Flow model load / refresh + open Models (tachyon menu)
				if ctrl && d == '9' {
					if let Ok(mut flow) = self.app.bridge.chat_state.flow_backend.try_lock() {
						flow.refresh_models();
					}
					let n = crate::flow_backend::discover_local_models()
						.iter()
						.filter(|m| m.is_local && m.is_selectable_model() && m.available)
						.count();
					self.app.bridge.chat_state.refresh_model_catalog();
					self.app.bridge.chat_state.open_models_menu();
					self.app.bridge.chat_state.show_toast(format!(
						"Flow models loaded · {n} ready · {}",
						crate::flow_backend::flow_models_dir().display()
					));
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}

				// Bare 0 / 1 -- tachyon-style product menus (same chrome as Theme palette)
				if bare {
					match d {
						'0' => {
							self.app.bridge.chat_state.open_models_menu();
							NEED_RENDER.store(1, Ordering::Relaxed);
							return Ok(KeyHandled::Consumed);
						}
						'1' => {
							self.app.bridge.chat_state.open_channels_menu();
							NEED_RENDER.store(1, Ordering::Relaxed);
							return Ok(KeyHandled::Consumed);
						}
						_ => {}
					}
				}

				let digit = d.to_digit(10).unwrap_or(0) as usize;
				let index = if ctrl {
					let offset = if digit == 0 { 9 } else { digit - 1 };
					10 + offset
				} else if digit >= 2 {
					digit - 2
				} else {
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				};
				let cs = &mut self.app.bridge.chat_state;
				match cs.toggle_menu_by_index(index) {
					Some(true) => {
						let title = cs.menu.main_menu_title(index).unwrap_or("Menu");
						cs.show_toast(title.to_string());
						cs.play_sound(SoundCue::MenuOpen);
					}
					Some(false) => {
						cs.show_toast("Menu closed".into());
						cs.play_sound(SoundCue::MenuClose);
					}
					None => {
						cs.show_toast(format!("No menu at index {index}"));
					}
				}
				NEED_RENDER.store(1, Ordering::Relaxed);
				return Ok(KeyHandled::Consumed);
			}
		}

		// Tab cycles agent mode (Ask -> Write -> Plan -> Goal) when no suggestion list is open.
		if key.code == KeyCode::Tab
			&& !key.modifiers.contains(KeyModifiers::CONTROL)
			&& !self.app.bridge.chat_state.input.has_suggestions()
			&& self.app.bridge.chat_state.ui.dialog == CommandDialog::None
			&& self.app.bridge.chat_state.ui.bottom_popup == BottomPopup::None
			&& !self.app.bridge.chat_state.diff_state.open
		{
			self.app.bridge.chat_state.cycle_agent_mode();
			self.app.bridge.chat_state.play_sound(SoundCue::Navigate);
			NEED_RENDER.store(1, Ordering::Relaxed);
			return Ok(KeyHandled::Consumed);
		}

		// Global keyboard shortcuts - check if any registered shortcut matches
		let pressed_key = format_key_event(&key);
		let mappings = &self.app.bridge.chat_state.menu.keyboard_mappings;

		for action in MenuAction::all_actions() {
			let shortcut = mappings.get(action);

			let matches = if shortcut.contains(" or ") {
				shortcut.split(" or ").any(|s| s.trim() == pressed_key)
			} else {
				shortcut == pressed_key
			};

			if matches {
				let submenu_index = match action {
					MenuAction::ContextControlPanel => None,
					MenuAction::Theme => Some(0),
					MenuAction::KeyboardShortcuts => Some(1),
					MenuAction::Providers => Some(2),
					MenuAction::PluginsApps => Some(3),
					MenuAction::Skills => Some(4),
					MenuAction::Sandbox => Some(5),
					MenuAction::WebSearch => Some(6),
					MenuAction::McpServers => Some(7),
					MenuAction::MemoryHistory => Some(8),
					MenuAction::MultiAgent => Some(9),
					MenuAction::Notifications => Some(10),
					MenuAction::VoiceRealtime => Some(11),
					MenuAction::ImageVision => Some(12),
					MenuAction::Profiles => Some(13),
					MenuAction::Worktree => Some(14),
					MenuAction::Authentication => Some(15),
					MenuAction::NetworkProxy => Some(16),
					MenuAction::HooksEvents => Some(17),
					MenuAction::SessionResume => Some(18),
					MenuAction::ApprovalPolicy => Some(19),
					MenuAction::ShellEnvironment => Some(20),
					MenuAction::ExecutionRules => Some(21),
					MenuAction::ProjectTrust => Some(22),
					MenuAction::DeveloperInstructions => Some(23),
					MenuAction::FeatureFlags => Some(24),
					MenuAction::DxTools => Some(crate::menu::DX_TOOLS_SUBMENU_INDEX),
				};

				let is_same_submenu = if let Some(idx) = submenu_index {
					self.app.bridge.chat_state.show_tachyon_menu
						&& self.app.bridge.chat_state.menu.current_submenu == Some(idx)
						&& self.app.bridge.chat_state.menu.opened_directly
				} else {
					self.app.bridge.chat_state.show_tachyon_menu
						&& self.app.bridge.chat_state.menu.current_submenu.is_none()
				};

				if is_same_submenu {
					self.app.bridge.chat_state.menu_is_closing = true;
					self.app.bridge.chat_state.menu.pick_closing_effect();
					self.app.bridge.chat_state.show_tachyon_menu = false;
					self.app.bridge.chat_state.play_sound(SoundCue::MenuClose);
				} else {
					if !self.app.bridge.chat_state.show_tachyon_menu {
						self.app.bridge.chat_state.menu_is_closing = false;
						self.app.bridge.chat_state.show_tachyon_menu = true;
						self.app.bridge.chat_state.menu.pick_opening_effect();
						self.app.bridge.chat_state.play_sound(SoundCue::MenuOpen);
					}

					if let Some(idx) = submenu_index {
						self.app.bridge.chat_state.menu.enter_submenu_directly(idx);
					} else {
						self.app.bridge.chat_state.menu.go_back_to_main();
					}
					self.app.bridge.chat_state.play_sound(SoundCue::Confirm);
				}

				NEED_RENDER.store(1, Ordering::Relaxed);
				return Ok(KeyHandled::Consumed);
			}
		}

		Ok(KeyHandled::Unconsumed)
	}

	fn execute_command_action(&mut self, action: CommandAction) {
		let cs = &mut self.app.bridge.chat_state;
		match action {
			CommandAction::NewSession | CommandAction::ClearChat => {
				let _ = cs.handle_slash_command("/new");
			}
			CommandAction::ResumeSession | CommandAction::OpenSessions => {
				let _ = cs.handle_slash_command("/sessions");
			}
			CommandAction::RenameSession => {
				let _ = cs.handle_slash_command("/rename");
			}
			CommandAction::ExportSession => {
				let _ = cs.handle_slash_command("/export");
			}
			CommandAction::ShareSession => {
				let _ = cs.handle_slash_command("/share");
			}
			CommandAction::ForkSession => {
				let _ = cs.handle_slash_command("/fork");
			}
			CommandAction::OpenDiff => cs.open_differ(),
			CommandAction::ToggleSidebar => {
				cs.ui.show_sidebar = !cs.ui.show_sidebar;
				cs.show_toast(if cs.ui.show_sidebar {
					"Sidebar shown".into()
				} else {
					"Sidebar hidden".into()
				});
			}
			CommandAction::ToggleTimestamps => {
				let _ = cs.handle_slash_command("/timestamps");
			}
			CommandAction::ToggleThinking => {
				let _ = cs.handle_slash_command("/thinking");
			}
			CommandAction::ToggleTheme => cs.toggle_theme_mode(),
			CommandAction::CycleMode => cs.cycle_agent_mode(),
			CommandAction::CycleModel => cs.cycle_model(),
			CommandAction::ToggleRuntime => cs.toggle_runtime_mode(),
			CommandAction::OpenMenu(index) => {
				cs.open_menu_by_index(index as usize);
			}
			CommandAction::ToggleNotifications => {
				cs.open_menu_by_index(10);
			}
			CommandAction::OpenVoice => {
				let _ = cs.handle_slash_command("/voice");
			}
			CommandAction::OpenHelp => {
				let _ = cs.handle_slash_command("/help");
			}
			CommandAction::OpenStatus => {
				let _ = cs.handle_slash_command("/status");
			}
			CommandAction::TogglePerf => {
				cs.ui.show_perf_overlay = !cs.ui.show_perf_overlay;
			}
			CommandAction::CopyLastResponse => cs.copy_last_assistant_response(),
			CommandAction::Interrupt => cs.interrupt_generation(),
			CommandAction::RequestExit => cs.request_exit(),
			CommandAction::Custom(command) => {
				let _ = cs.handle_slash_command(&command);
			}
		}
	}

	fn handle_command_palette_key(&mut self, key: KeyEvent) -> Result<KeyHandled> {
		use crossterm::event::KeyCode;

		// Command palette input handling (grabs all keys when open)
		if self.app.bridge.chat_state.command_palette.open {
			match key.code {
				KeyCode::Esc => {
					self.app.bridge.chat_state.command_palette.close();
					self.app.bridge.chat_state.play_sound(SoundCue::MenuClose);
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
					self.app.bridge.chat_state.command_palette.push_char(c);
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				KeyCode::Backspace => {
					self.app.bridge.chat_state.command_palette.pop_char();
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				KeyCode::Up | KeyCode::Char('k') => {
					self.app.bridge.chat_state.command_palette.move_cursor(-1);
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				KeyCode::Down | KeyCode::Char('j') => {
					self.app.bridge.chat_state.command_palette.move_cursor(1);
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				KeyCode::Enter => {
					if let Some(action) = self
						.app
						.bridge
						.chat_state
						.command_palette
						.selected_command()
						.map(|command| command.action.clone())
					{
						self.app.bridge.chat_state.command_palette.close();
						self.execute_command_action(action);
						self.app.bridge.chat_state.play_sound(SoundCue::Confirm);
					}
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				_ => {
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
			}
		}

		Ok(KeyHandled::Unconsumed)
	}

	fn handle_vim_key(&mut self, key: KeyEvent) -> Result<KeyHandled> {
		// Vim mode: intercept keys when not in Insert mode
		if self.app.bridge.chat_state.vim_mode.enabled
			&& self.app.bridge.chat_state.vim_mode.mode != crate::vim_mode::VimMode::Insert
		{
			let action = self.app.bridge.chat_state.vim_mode.handle_key(key);
			match action {
				crate::vim_mode::VimAction::MoveUp => {
					self.app.bridge.chat_state.scroll_chat_by(-3);
					NEED_RENDER.store(1, Ordering::Relaxed);
				}
				crate::vim_mode::VimAction::MoveDown => {
					self.app.bridge.chat_state.scroll_chat_by(3);
					NEED_RENDER.store(1, Ordering::Relaxed);
				}
				crate::vim_mode::VimAction::MoveToTop => {
					self.app.bridge.chat_state.set_chat_scroll(0);
					NEED_RENDER.store(1, Ordering::Relaxed);
				}
				crate::vim_mode::VimAction::MoveToBottom => {
					let max = self.app.bridge.chat_state.max_chat_scroll();
					self.app.bridge.chat_state.set_chat_scroll(max);
					NEED_RENDER.store(1, Ordering::Relaxed);
				}
				crate::vim_mode::VimAction::EnterInsertMode
				| crate::vim_mode::VimAction::EnterInsertAtEnd
				| crate::vim_mode::VimAction::EnterInsertBeforeLine
				| crate::vim_mode::VimAction::EnterInsertAfterLine => {
					self.app.bridge.chat_state.play_sound(SoundCue::Toggle);
					NEED_RENDER.store(1, Ordering::Relaxed);
				}
				crate::vim_mode::VimAction::DeleteChar => {
					if !self.app.bridge.chat_state.input.content.is_empty() {
						self.app.bridge.chat_state.input.handle_key(key);
					}
					NEED_RENDER.store(1, Ordering::Relaxed);
				}
				crate::vim_mode::VimAction::YankLine => {
					if let Some(text) = self.app.bridge.chat_state.input.get_selected_text() {
						let _ = cli_clipboard::set_contents(text);
						self.app.bridge.chat_state.show_toast("Yanked".into());
					} else if !self.app.bridge.chat_state.input.content.is_empty() {
						let _ = cli_clipboard::set_contents(self.app.bridge.chat_state.input.content.clone());
						self.app.bridge.chat_state.show_toast("Yanked line".into());
					}
					NEED_RENDER.store(1, Ordering::Relaxed);
				}
				crate::vim_mode::VimAction::PasteAfter | crate::vim_mode::VimAction::PasteBefore => {
					if let Ok(text) = cli_clipboard::get_contents() {
						self.app.bridge.chat_state.input.paste_text(&text);
						self.app.bridge.chat_state.show_toast("Pasted".into());
					}
					NEED_RENDER.store(1, Ordering::Relaxed);
				}
				_ => {
					NEED_RENDER.store(1, Ordering::Relaxed);
				}
			}
			// Stay in command mode for `:` and `/` -- don't fall through to input.
			if self.app.bridge.chat_state.vim_mode.mode == crate::vim_mode::VimMode::Command {
				return Ok(KeyHandled::Consumed);
			}
		}

		Ok(KeyHandled::Unconsumed)
	}

	fn handle_chat_input_key(&mut self, key: KeyEvent) -> Result<KeyHandled> {
		use crate::input::InputAction;
		use crossterm::event::KeyCode;

		// Handle chat input when in Chat mode or FilePicker mode (chat input is visible)
		if self.app.bridge.mode == crate::bridge::AppMode::Chat
			|| self.app.bridge.mode == crate::bridge::AppMode::FilePicker
		{
			use crossterm::event::KeyEventKind;

			// Ignore releases for normal keys
			if key.kind == KeyEventKind::Release {
				return Ok(KeyHandled::Consumed);
			}

			// Handle Space key for voice mode - hybrid hold detection
			if key.code == KeyCode::Char(' ') && key.modifiers.is_empty() {
				let now = Instant::now();
				let is_repeat = key.kind == KeyEventKind::Repeat;

				// Check if this is a rapid repeat using timing (fallback for terminals without enhancement flags)
				let is_timing_repeat = if let Some(last_press) = self.app.bridge.chat_state.last_space_press
				{
					last_press.elapsed() < Duration::from_millis(100)
				} else {
					false
				};

				if is_repeat || is_timing_repeat {
					// Key is being held! Activate voice mode (spinner)
					if !self.app.bridge.chat_state.space_held {
						let old_cursor_pos = self.app.bridge.chat_state.input.cursor_position;

						self.app.bridge.chat_state.animation.cursor_revert_from_pos = old_cursor_pos;

						// Remove ALL trailing spaces that were typed during the hold detection
						let mut new_pos = old_cursor_pos;
						let mut spaces_removed = 0;

						while new_pos > 0 && spaces_removed < 2 {
							let content_before = &self.app.bridge.chat_state.input.content[..new_pos];
							if content_before.ends_with(' ') {
								new_pos -= 1;
								self.app.bridge.chat_state.input.content.remove(new_pos);
								spaces_removed += 1;
							} else {
								break;
							}
						}

						if spaces_removed > 0 {
							self.app.bridge.chat_state.input.cursor_position = new_pos;
						}

						// Start cursor revert animation
						self.app.bridge.chat_state.animation.cursor_revert_animation = true;
						self.app.bridge.chat_state.animation.cursor_revert_start = Some(now);

						// Activate voice mode
						self.app.bridge.chat_state.space_held = true;
						self.app.bridge.chat_state.space_hold_start = Some(now);
					}
					self.app.bridge.chat_state.last_space_press = Some(now);
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed); // Don't type spaces while holding
				} else if key.kind == KeyEventKind::Release {
					// Space released - deactivate voice mode; optional push-to-talk STT.
					if self.app.bridge.chat_state.space_held {
						let held_ms = self
							.app
							.bridge
							.chat_state
							.space_hold_start
							.map(|t| t.elapsed().as_millis())
							.unwrap_or(0);
						self.app.bridge.chat_state.space_held = false;
						self.app.bridge.chat_state.space_hold_start = None;
						self.app.bridge.chat_state.last_space_press = None;
						self.app.bridge.chat_state.animation.cursor_revert_animation = false;
						self.app.bridge.chat_state.animation.cursor_revert_start = None;
						if held_ms >= 400 {
							self.app.bridge.chat_state.on_push_to_talk_release();
						}
						NEED_RENDER.store(1, Ordering::Relaxed);
					}
					return Ok(KeyHandled::Consumed);
				} else {
					// First press - type the space normally
					self.app.bridge.chat_state.last_space_press = Some(now);
					let action = self.app.bridge.chat_state.input.handle_key(key);
					match action {
						InputAction::Changed => {
							self.app.bridge.chat_state.play_sound(sound_for_input_change(key));
							NEED_RENDER.store(1, Ordering::Relaxed);
							return Ok(KeyHandled::Consumed);
						}
						_ => return Ok(KeyHandled::Consumed),
					}
				}
			} else {
				// Any other key pressed - deactivate voice mode
				if self.app.bridge.chat_state.space_held {
					self.app.bridge.chat_state.space_held = false;
					self.app.bridge.chat_state.space_hold_start = None;
					self.app.bridge.chat_state.last_space_press = None;
					self.app.bridge.chat_state.animation.cursor_revert_animation = false;
					self.app.bridge.chat_state.animation.cursor_revert_start = None;
					NEED_RENDER.store(1, Ordering::Relaxed);
				}
			}

			// If voice mode is active (spinner showing), don't process any input - just return
			if self.app.bridge.chat_state.space_held {
				NEED_RENDER.store(1, Ordering::Relaxed);
				return Ok(KeyHandled::Consumed);
			}

			// Message-list scroll: PageUp/PageDown or Ctrl+Up/Down only.
			if !self.app.bridge.chat_state.messages.is_empty() {
				match (key.code, key.modifiers) {
					(KeyCode::PageUp, _) => {
						let page = self.app.bridge.chat_state.ui.chat_list_area.height.max(1) as i32;
						self.app.bridge.chat_state.scroll_chat_by(-page);
						self.app.bridge.chat_state.play_sound(SoundCue::Navigate);
						NEED_RENDER.store(1, Ordering::Relaxed);
						return Ok(KeyHandled::Consumed);
					}
					(KeyCode::PageDown, _) => {
						let page = self.app.bridge.chat_state.ui.chat_list_area.height.max(1) as i32;
						self.app.bridge.chat_state.scroll_chat_by(page);
						self.app.bridge.chat_state.play_sound(SoundCue::Navigate);
						NEED_RENDER.store(1, Ordering::Relaxed);
						return Ok(KeyHandled::Consumed);
					}
					(KeyCode::Up, m) if m.contains(KeyModifiers::CONTROL) => {
						self.app.bridge.chat_state.scroll_chat_by(-3);
						self.app.bridge.chat_state.play_sound(SoundCue::Navigate);
						NEED_RENDER.store(1, Ordering::Relaxed);
						return Ok(KeyHandled::Consumed);
					}
					(KeyCode::Down, m) if m.contains(KeyModifiers::CONTROL) => {
						self.app.bridge.chat_state.scroll_chat_by(3);
						self.app.bridge.chat_state.play_sound(SoundCue::Navigate);
						NEED_RENDER.store(1, Ordering::Relaxed);
						return Ok(KeyHandled::Consumed);
					}
					_ => {}
				}
			}

			// Ctrl+B toggles sidebar
			if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('b') {
				self.app.bridge.chat_state.ui.show_sidebar ^= true;
				NEED_RENDER.store(1, Ordering::Relaxed);
				return Ok(KeyHandled::Consumed);
			}

			// Clear stuck shift_held immediately for plain Enter
			if matches!(key.code, KeyCode::Enter) && !key.modifiers.contains(KeyModifiers::SHIFT) {
				self.app.bridge.chat_state.ui.shift_held = false;
			}

			// Newline: Alt+Enter (primary) or Ctrl+J. Plain Enter submits.
			let wants_newline = match key.code {
				KeyCode::Enter if key.modifiers.contains(KeyModifiers::ALT) => true,
				KeyCode::Char('j') if key.modifiers.contains(KeyModifiers::CONTROL) => true,
				_ => false,
			};
			if wants_newline {
				self.app.bridge.chat_state.input.insert_newline();
				self.app.bridge.chat_state.play_sound(SoundCue::TextInput);
				NEED_RENDER.store(1, Ordering::Relaxed);
				return Ok(KeyHandled::Consumed);
			}
			// Plain Enter -- allow submit
			if matches!(key.code, KeyCode::Enter) {
				self.app.bridge.chat_state.ui.shift_held = false;
			}

			// Ctrl+A: select all input text, or all chat messages when input is empty.
			if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('a') {
				if self.app.bridge.chat_state.input.content.is_empty() {
					self.app.bridge.chat_state.select_all_chat();
					self.app.bridge.chat_state.show_toast("Selected all messages".into());
				} else {
					self.app.bridge.chat_state.input.select_all();
					self.app.bridge.chat_state.clear_chat_selection();
				}
				NEED_RENDER.store(1, Ordering::Relaxed);
				return Ok(KeyHandled::Consumed);
			}

			// Ctrl+C is exit-only. Selections auto-copy on mouse-up (no Ctrl+C copy).

			// Route key to chat input
			let action = self.app.bridge.chat_state.input.handle_key(key);

			match action {
				InputAction::Submit(msg) => {
					// OpenCode-compatible slash commands (/sessions, /new, /help, ...)
					if let Some(result) = self.app.bridge.chat_state.try_handle_slash(&msg) {
						match result {
							SlashResult::Exit => {
								self.app.bridge.chat_state.request_exit();
							}
							SlashResult::Handled | SlashResult::Unknown(_) => {
								self.app.bridge.chat_state.play_sound(SoundCue::Confirm);
							}
							SlashResult::SwitchMode(mode) => {
								self.app.bridge.mode = mode;
								self.app.bridge.chat_state.play_sound(SoundCue::Confirm);
							}
						}
						NEED_RENDER.store(1, Ordering::Relaxed);
						return Ok(KeyHandled::Consumed);
					}
					// Normal chat message
					self.app.bridge.chat_state.add_user_message(msg);
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				InputAction::Exit => {
					self.app.bridge.chat_state.request_exit();
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				InputAction::Changed => {
					self.app.bridge.chat_state.play_sound(sound_for_input_change(key));
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				InputAction::PreviousHistory | InputAction::NextHistory => {
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				InputAction::Pasted { lines, chars } => {
					let toast = if lines >= 2 || chars >= 80 {
						format!("[pasted {lines} lines]")
					} else {
						format!("Pasted {chars} chars")
					};
					self.app.bridge.chat_state.show_toast(toast);
					self.app.bridge.chat_state.play_sound(sound_for_input_change(key));
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				InputAction::Copied { chars } => {
					self.app.bridge.chat_state.show_toast(format!("✓ Copied {chars} chars"));
					self.app.bridge.chat_state.play_sound(SoundCue::Confirm);
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				InputAction::Attached { name } => {
					self.app.bridge.chat_state.show_toast(format!("Attached {name}"));
					self.app.bridge.chat_state.play_sound(SoundCue::Confirm);
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
				InputAction::None => {
					// FilePicker / Diff: let navigation keys through when the input
					// did not handle them (arrows, enter on empty, etc.).
					let in_fb = self.app.bridge.mode == crate::bridge::AppMode::FilePicker;
					let in_diff = self.app.bridge.chat_state.diff_state.open;
					if (in_fb || in_diff) && !self.app.bridge.chat_state.input.has_suggestions() {
						NEED_RENDER.store(1, Ordering::Relaxed);
						return Ok(KeyHandled::Unconsumed);
					}
					NEED_RENDER.store(1, Ordering::Relaxed);
					return Ok(KeyHandled::Consumed);
				}
			}
		}

		Ok(KeyHandled::Unconsumed)
	}

	#[inline]
	fn dispatch_key(&mut self, key: KeyEvent) -> Result<Data> {
		// Track Shift hold for Shift+Enter (works even if Enter lacks SHIFT flag)
		// MUST run before animation mode early returns so shift release events are never missed
		{
			use crossterm::event::KeyEventKind;
			let is_shift_key = matches!(
				key.code,
				KeyCode::Modifier(
					crossterm::event::ModifierKeyCode::LeftShift
						| crossterm::event::ModifierKeyCode::RightShift
				)
			);
			if is_shift_key {
				self.app.bridge.chat_state.ui.shift_held = key.kind != KeyEventKind::Release;
				return Ok(Data::Nil);
			}
			if key.kind != KeyEventKind::Release && key.modifiers.contains(KeyModifiers::SHIFT) {
				self.app.bridge.chat_state.ui.shift_held = true;
			}
		}

		// Global: ignore key *releases* so Press handlers (digit menus, popups, toggles)
		// are not immediately undone by the matching Release event (Windows / enhanced keys).
		// Space release is handled later for hold-to-talk.
		{
			use crossterm::event::KeyEventKind;
			if key.kind == KeyEventKind::Release && key.code != KeyCode::Char(' ') {
				return Ok(Data::Nil);
			}
		}

		if self.handle_priority_key(key)? == KeyHandled::Consumed {
			return Ok(Data::Nil);
		}
		if self.handle_editor_key(key)? == KeyHandled::Consumed {
			return Ok(Data::Nil);
		}
		if self.handle_screen_nav_key(key)? == KeyHandled::Consumed {
			return Ok(Data::Nil);
		}
		if self.handle_menu_key(key)? == KeyHandled::Consumed {
			return Ok(Data::Nil);
		}
		if self.handle_animation_key(key)? == KeyHandled::Consumed {
			return Ok(Data::Nil);
		}
		if self.handle_session_key(key)? == KeyHandled::Consumed {
			return Ok(Data::Nil);
		}
		if self.handle_voice_key(key)? == KeyHandled::Consumed {
			return Ok(Data::Nil);
		}
		if self.handle_dialog_key(key)? == KeyHandled::Consumed {
			return Ok(Data::Nil);
		}
		if self.handle_diff_key(key)? == KeyHandled::Consumed {
			return Ok(Data::Nil);
		}
		if self.handle_popup_key(key)? == KeyHandled::Consumed {
			return Ok(Data::Nil);
		}
		if self.handle_global_key(key)? == KeyHandled::Consumed {
			return Ok(Data::Nil);
		}
		if self.handle_command_palette_key(key)? == KeyHandled::Consumed {
			return Ok(Data::Nil);
		}
		if self.handle_vim_key(key)? == KeyHandled::Consumed {
			return Ok(Data::Nil);
		}
		if self.handle_chat_input_key(key)? == KeyHandled::Consumed {
			return Ok(Data::Nil);
		}

		// Route to the embedded file browser's normal key handling.
		Router::new(self.app).route(Key::from(key))?;
		succ!();
	}
	#[inline]
	fn dispatch_mouse(&mut self, mouse: MouseEvent) -> Result<Data> {
		use crossterm::event::{MouseButton, MouseEventKind};

		// Handle menu mouse events globally when menu is visible
		if self.app.bridge.chat_state.show_tachyon_menu {
			if self.app.bridge.chat_state.pending_dx_tool_confirmation.is_some() {
				NEED_RENDER.store(1, Ordering::Relaxed);
				succ!()
			}

			match mouse.kind {
				MouseEventKind::Moved => {
					// Handle hover - always process and render if state changed
					if self.app.bridge.chat_state.menu.handle_mouse(mouse.column, mouse.row, false) {
						// Apply theme preview if hovering over a theme
						if let Some(theme_name) = self.app.bridge.chat_state.menu.get_hovered_theme_name() {
							self
								.app
								.bridge
								.chat_state
								.apply_theme(&theme_name, self.app.bridge.chat_state.theme_mode);
						}
					}
					NEED_RENDER.store(1, Ordering::Relaxed);
				}
				#[allow(clippy::collapsible_match)]
				MouseEventKind::Down(MouseButton::Left) => {
					// Handle click - select and potentially enter submenu
					if self.app.bridge.chat_state.menu.handle_mouse(mouse.column, mouse.row, true) {
						// Check if toggle mode button is clicked
						if self.app.bridge.chat_state.menu.is_toggle_mode_selected() {
							// Toggle the theme mode
							self.app.bridge.chat_state.toggle_theme_mode();
							self.app.bridge.chat_state.play_sound(SoundCue::Toggle);
							NEED_RENDER.store(1, Ordering::Relaxed);
							succ!()
						}

						// Check if toggle recording button is clicked
						if self.app.bridge.chat_state.menu.is_toggle_recording_selected() {
							// Toggle the recording mode
							self.app.bridge.chat_state.menu.toggle_recording_mode();
							self.app.bridge.chat_state.play_sound(SoundCue::Toggle);
							NEED_RENDER.store(1, Ordering::Relaxed);
							succ!()
						}

						if let Some(action) = self.app.bridge.chat_state.menu.selected_dx_tool_action() {
							return self.dispatch_dx_tool_action(action);
						}

						// Models / Channels product menus (key 0 / 1)
						if self.app.bridge.chat_state.menu.is_dynamic_models()
							|| self.app.bridge.chat_state.menu.is_dynamic_channels()
						{
							if self.app.bridge.chat_state.activate_dynamic_menu_selection() {
								self.app.bridge.chat_state.play_sound(SoundCue::Confirm);
							}
							NEED_RENDER.store(1, Ordering::Relaxed);
							succ!()
						}

						// Get the current theme name before selecting
						let theme_name = self.app.bridge.chat_state.menu.get_selected_theme_name();

						// Item was clicked - now select it (enter submenu or execute)
						let _should_close = !self.app.bridge.chat_state.menu.select_current_item();
						self.app.bridge.chat_state.play_sound(SoundCue::Confirm);

						// If we were in theme submenu and clicked a theme, just close the menu
						// (theme is already applied from hover)
						if theme_name.is_some() {
							self.app.bridge.chat_state.menu_is_closing = true;
							self.app.bridge.chat_state.menu.pick_closing_effect();
							self.app.bridge.chat_state.show_tachyon_menu = false;
							self.app.bridge.chat_state.play_sound(SoundCue::MenuClose);
						}

						NEED_RENDER.store(1, Ordering::Relaxed);
					}
				}
				MouseEventKind::ScrollUp => {
					// Scroll up (previous items)
					self.app.bridge.chat_state.menu.select_prev_menu_item();
					self.app.bridge.chat_state.play_sound(SoundCue::Navigate);
					NEED_RENDER.store(1, Ordering::Relaxed);
					succ!()
				}
				MouseEventKind::ScrollDown => {
					// Scroll down (next items)
					self.app.bridge.chat_state.menu.select_next_menu_item();
					self.app.bridge.chat_state.play_sound(SoundCue::Navigate);
					NEED_RENDER.store(1, Ordering::Relaxed);
					succ!()
				}
				_ => {}
			}
		} else {
			// Not in menu, handle normal UI clicks / hover / wheel / scrollbar drag
			let x = mouse.column;
			let y = mouse.row;
			let cs = &mut self.app.bridge.chat_state;

			// File browser is on screen in FilePicker mode (or the FileBrowser carousel
			// frame). Wheel/clicks over that surface must reach the FB mouse actor —
			// do not swallow them as chat-owned events.
			let fb_on_screen = matches!(self.app.bridge.mode, AppMode::FilePicker)
				|| (cs.animation.animation_mode
					&& cs.messages.is_empty()
					&& cs.current_animation() == crate::state::AnimationType::FileBrowser);
			if fb_on_screen {
				// These rectangles belong to the last chat frame. Clear them so
				// file-browser clicks cannot be swallowed by stale sidebar,
				// minimap, or message-list hit targets.
				cs.ui.sidebar_panel_area = ratatui::layout::Rect::default();
				cs.ui.sidebar_area = ratatui::layout::Rect::default();
				cs.ui.chat_list_area = ratatui::layout::Rect::default();
				cs.ui.minimap_area = ratatui::layout::Rect::default();
				cs.ui.minimap_top_indicator = ratatui::layout::Rect::default();
				cs.ui.minimap_bottom_indicator = ratatui::layout::Rect::default();
			}

			// Diff screen mouse handling (takes over all events when open).
			// Hit-test uses rects filled by the last render_diff_view call so clicks
			// align with the painted file tree / patch panes.
			if cs.diff_state.open {
				let tree_inner = cs.diff_state.tree_inner;
				let patch_inner = cs.diff_state.patch_inner;
				let tree_viewport = tree_inner.height.max(1) as usize;
				let patch_viewport = patch_inner.height.max(1) as usize;

				let in_tree = cs.diff_state.point_in_tree(x, y);
				let in_patch = cs.diff_state.point_in_patch(x, y);

				// Active scrollbar drag must win over pane clicks (pointer may leave the track).
				match cs.ui.scroll_drag {
					ScrollDrag::DiffTree { anchor_y, anchor_scroll } => match mouse.kind {
						MouseEventKind::Drag(MouseButton::Left) | MouseEventKind::Moved => {
							let max = cs.diff_state.visible_tree_rows().len().saturating_sub(tree_viewport);
							let off = crate::state::ChatState::scroll_offset_from_drag(
								y,
								anchor_y,
								anchor_scroll,
								tree_inner,
								max,
							);
							cs.diff_state.tree_scroll = off;
							NEED_RENDER.store(1, Ordering::Relaxed);
							succ!()
						}
						MouseEventKind::Up(_) => {
							cs.ui.scroll_drag = ScrollDrag::None;
							NEED_RENDER.store(1, Ordering::Relaxed);
							succ!()
						}
						_ => {}
					},
					ScrollDrag::DiffPatch { anchor_y, anchor_scroll } => match mouse.kind {
						MouseEventKind::Drag(MouseButton::Left) | MouseEventKind::Moved => {
							let max = cs.diff_state.max_diff_scroll(patch_viewport);
							let off = crate::state::ChatState::scroll_offset_from_drag(
								y,
								anchor_y,
								anchor_scroll,
								patch_inner,
								max,
							);
							cs.diff_state.diff_scroll = off;
							NEED_RENDER.store(1, Ordering::Relaxed);
							succ!()
						}
						MouseEventKind::Up(_) => {
							cs.ui.scroll_drag = ScrollDrag::None;
							NEED_RENDER.store(1, Ordering::Relaxed);
							succ!()
						}
						_ => {}
					},
					_ => {}
				}

				match mouse.kind {
					MouseEventKind::ScrollUp => {
						if in_tree || (cs.diff_state.focus_tree && !in_patch) {
							cs.diff_state.move_tree_cursor(-3);
						} else {
							cs.diff_state.scroll_diff_by(-3, patch_viewport);
						}
						NEED_RENDER.store(1, Ordering::Relaxed);
						succ!()
					}
					MouseEventKind::ScrollDown => {
						if in_tree || (cs.diff_state.focus_tree && !in_patch) {
							cs.diff_state.move_tree_cursor(3);
						} else {
							cs.diff_state.scroll_diff_by(3, patch_viewport);
						}
						NEED_RENDER.store(1, Ordering::Relaxed);
						succ!()
					}
					MouseEventKind::Up(_) => {
						cs.ui.scroll_drag = ScrollDrag::None;
						NEED_RENDER.store(1, Ordering::Relaxed);
						succ!()
					}
					MouseEventKind::Down(MouseButton::Left) => {
						// Wider hit target for scrollbar (last 2 cols of the pane).
						let tree_track_x = tree_inner.right().saturating_sub(2);
						let patch_track_x = patch_inner.right().saturating_sub(2);

						// Tree scrollbar drag start
						if tree_inner.height > 0
							&& x >= tree_track_x
							&& x < tree_inner.right().saturating_add(1)
							&& y >= tree_inner.y
							&& y < tree_inner.bottom()
						{
							let max = cs.diff_state.visible_tree_rows().len().saturating_sub(tree_viewport);
							let track_rect = ratatui::layout::Rect {
								x: tree_track_x,
								y: tree_inner.y,
								width: tree_inner.right().saturating_sub(tree_track_x).max(1),
								height: tree_inner.height,
							};
							let off = crate::state::ChatState::scroll_offset_from_track_y(y, track_rect, max);
							cs.diff_state.tree_scroll = off;
							cs.ui.scroll_drag = ScrollDrag::DiffTree { anchor_y: y, anchor_scroll: off };
							cs.diff_state.focus_tree = true;
							NEED_RENDER.store(1, Ordering::Relaxed);
							succ!()
						}

						// Patch scrollbar drag start
						if patch_inner.height > 0
							&& x >= patch_track_x
							&& x < patch_inner.right().saturating_add(1)
							&& y >= patch_inner.y
							&& y < patch_inner.bottom()
						{
							let max = cs.diff_state.max_diff_scroll(patch_viewport);
							let track_rect = ratatui::layout::Rect {
								x: patch_track_x,
								y: patch_inner.y,
								width: patch_inner.right().saturating_sub(patch_track_x).max(1),
								height: patch_inner.height,
							};
							let off = crate::state::ChatState::scroll_offset_from_track_y(y, track_rect, max);
							cs.diff_state.diff_scroll = off;
							cs.ui.scroll_drag = ScrollDrag::DiffPatch { anchor_y: y, anchor_scroll: off };
							cs.diff_state.focus_tree = false;
							NEED_RENDER.store(1, Ordering::Relaxed);
							succ!()
						}

						// File tree rows — full pane width is clickable
						if in_tree {
							cs.diff_state.focus_tree = true;
							if let Some(clicked_idx) = cs.diff_state.tree_row_at_y(y) {
								cs.diff_state.tree_cursor = clicked_idx;
								let tree_rows = cs.diff_state.visible_tree_rows();
								if let Some(row) = tree_rows.get(clicked_idx) {
									if row.is_dir {
										cs.diff_state.toggle_tree_at_cursor();
									} else if let Some(idx) = row.file_index {
										cs.diff_state.select_file(idx);
									}
								}
							}
							NEED_RENDER.store(1, Ordering::Relaxed);
							succ!()
						}

						if in_patch {
							cs.diff_state.focus_tree = false;
							NEED_RENDER.store(1, Ordering::Relaxed);
							succ!()
						}

						NEED_RENDER.store(1, Ordering::Relaxed);
						succ!()
					}

					// Hover highlight for diff scrollbars (editor-style track/thumb).
					MouseEventKind::Moved => {
						let tree_track = cs.diff_state.tree_inner;
						let patch_track = cs.diff_state.patch_inner;
						let on_tree = tree_track.width > 0
							&& x >= tree_track.right().saturating_sub(1)
							&& x < tree_track.right()
							&& y >= tree_track.y
							&& y < tree_track.bottom();
						let on_patch = patch_track.width > 0
							&& x >= patch_track.right().saturating_sub(1)
							&& x < patch_track.right()
							&& y >= patch_track.y
							&& y < patch_track.bottom();
						if cs.diff_state.tree_scrollbar_hovered != on_tree
							|| cs.diff_state.patch_scrollbar_hovered != on_patch
						{
							cs.diff_state.tree_scrollbar_hovered = on_tree;
							cs.diff_state.patch_scrollbar_hovered = on_patch;
							NEED_RENDER.store(1, Ordering::Relaxed);
						}
						succ!()
					}
					MouseEventKind::Drag(_) => {
						succ!()
					}
					_ => {
						succ!()
					}
				}
			}

			let minimap_area = cs.ui.minimap_area;
			let sidebar_panel = cs.ui.sidebar_panel_area;
			let chat_list_area = cs.ui.chat_list_area;
			let input_area = cs.ui.input_area;
			let input_text = cs.ui.input_text_area;
			let top_ind = cs.ui.minimap_top_indicator;
			let bot_ind = cs.ui.minimap_bottom_indicator;
			let chat_track = cs.chat_scrollbar_track();
			let side_track = cs.sidebar_scrollbar_track();

			let in_rect = |area: ratatui::layout::Rect| {
				area.width > 0
					&& area.height > 0
					&& x >= area.x
					&& x < area.x + area.width
					&& y >= area.y
					&& y < area.y + area.height
			};

			let in_minimap = in_rect(minimap_area);
			let in_minimap_zone = in_minimap || in_rect(top_ind) || in_rect(bot_ind);
			let in_sidebar = in_rect(sidebar_panel);
			let in_chat = in_rect(chat_list_area);
			let in_input = in_rect(input_area);
			let in_input_text = in_rect(input_text);
			let in_chat_track = in_rect(chat_track) && cs.max_chat_scroll() > 0;
			let in_side_track = in_rect(side_track) && cs.max_sidebar_scroll() > 0;

			let minimap_scroll = cs.ui.minimap_scroll as usize;
			let minimap_index_at = |rel_y: u16| -> usize { rel_y as usize + minimap_scroll };

			// Minimap: one marker per wheel tick. Chat/sidebar: snappier steps.
			const MINIMAP_WHEEL: i32 = 1;
			let sidebar_wheel = (cs.ui.sidebar_area.height.max(3) / 3).max(1) as i32;
			let chat_wheel = (cs.ui.chat_list_area.height.max(4) / 3).max(1) as i32;

			// Continue active scrollbar drag (pointer may leave the track)
			if cs.ui.scroll_drag != ScrollDrag::None {
				match mouse.kind {
					MouseEventKind::Drag(MouseButton::Left) | MouseEventKind::Moved => {
						match cs.ui.scroll_drag {
							ScrollDrag::Chat { anchor_y, anchor_scroll } => {
								let max = cs.max_chat_scroll();
								let off = crate::state::ChatState::scroll_offset_from_drag(
									y,
									anchor_y,
									anchor_scroll,
									chat_track,
									max,
								);
								cs.set_chat_scroll(off);
								NEED_RENDER.store(1, Ordering::Relaxed);
								succ!()
							}
							ScrollDrag::Sidebar { anchor_y, anchor_scroll } => {
								let max = cs.max_sidebar_scroll() as usize;
								let off = crate::state::ChatState::scroll_offset_from_drag(
									y,
									anchor_y,
									anchor_scroll,
									side_track,
									max,
								);
								cs.set_sidebar_scroll(off as u16);
								NEED_RENDER.store(1, Ordering::Relaxed);
								succ!()
							}
							ScrollDrag::FileBrowser { .. } => {}
							ScrollDrag::DiffTree { anchor_y, anchor_scroll } => {
								let viewport = cs.diff_state.tree_inner.height.max(1) as usize;
								let max = cs.diff_state.visible_tree_rows().len().saturating_sub(viewport);
								let track = cs.diff_state.tree_inner;
								let off = crate::state::ChatState::scroll_offset_from_drag(
									y,
									anchor_y,
									anchor_scroll,
									track,
									max,
								);
								cs.diff_state.tree_scroll = off;
								NEED_RENDER.store(1, Ordering::Relaxed);
								succ!()
							}
							ScrollDrag::DiffPatch { anchor_y, anchor_scroll } => {
								let viewport = cs.diff_state.patch_inner.height.max(1) as usize;
								let max = cs.diff_state.max_diff_scroll(viewport);
								let track = cs.diff_state.patch_inner;
								let off = crate::state::ChatState::scroll_offset_from_drag(
									y,
									anchor_y,
									anchor_scroll,
									track,
									max,
								);
								cs.diff_state.diff_scroll = off;
								NEED_RENDER.store(1, Ordering::Relaxed);
								succ!()
							}
							ScrollDrag::None => {}
						}
					}
					MouseEventKind::Up(MouseButton::Left) => {
						cs.ui.scroll_drag = ScrollDrag::None;
						NEED_RENDER.store(1, Ordering::Relaxed);
						succ!()
					}
					_ => {}
				}
			}

			// Mouse text selection inside the chat input (browser-like drag select)
			if cs.input.mouse_selecting {
				match mouse.kind {
					MouseEventKind::Drag(MouseButton::Left) | MouseEventKind::Moved => {
						let col = x.saturating_sub(input_text.x);
						let row = y.saturating_sub(input_text.y);
						cs.input.update_mouse_select(col, row);
						NEED_RENDER.store(1, Ordering::Relaxed);
						succ!()
					}
					MouseEventKind::Up(MouseButton::Left) => {
						cs.input.end_mouse_select();
						NEED_RENDER.store(1, Ordering::Relaxed);
						succ!()
					}
					_ => {}
				}
			}

			match mouse.kind {
				MouseEventKind::Moved => {
					// Bottom-bar center chip hover (permission / plan / goal)
					let prev_hover = cs.ui.center_chip_hover;
					cs.update_center_hover(x, y);
					if cs.ui.center_chip_hover != prev_hover {
						NEED_RENDER.store(1, Ordering::Relaxed);
					}

					// Scrollbar hover highlight (track + thumb lighten like the code editor).
					let chat_h = in_chat_track;
					let side_h = in_side_track;
					if cs.ui.chat_scrollbar_hovered != chat_h || cs.ui.sidebar_scrollbar_hovered != side_h {
						cs.ui.chat_scrollbar_hovered = chat_h;
						cs.ui.sidebar_scrollbar_hovered = side_h;
						NEED_RENDER.store(1, Ordering::Relaxed);
					}

					// Minimap markers OR user bubbles in the chat list share hover state.
					let next_hover = if in_minimap {
						let local_index = minimap_index_at(y - minimap_area.y);
						let user_indices = cs.user_message_indices();
						user_indices.get(local_index).copied()
					} else if in_chat {
						// Only light up rounded user bubbles (not assistant rows).
						cs.message_index_at_y(y).and_then(|idx| {
							let is_user = cs
								.messages
								.get(idx)
								.is_some_and(|m| m.role == crate::components::MessageRole::User && !m.hidden);
							if !is_user {
								return None;
							}
							// Require pointer to be inside the right-aligned bubble, not empty gutter.
							if cs.pointer_inside_user_bubble(x, y, idx) { Some(idx) } else { None }
						})
					} else {
						None
					};

					if cs.ui.hovered_message_index != next_hover {
						cs.ui.hovered_message_index = next_hover;
						cs.ui.hovered_message_since =
							if next_hover.is_some() { Some(Instant::now()) } else { None };
						NEED_RENDER.store(1, Ordering::Relaxed);
					}
				}
				MouseEventKind::Down(MouseButton::Left) => {
					// Click / start drag-select in the input text area
					if in_input_text {
						let col = x.saturating_sub(input_text.x);
						let row = y.saturating_sub(input_text.y);
						cs.input.begin_mouse_select(col, row);
						NEED_RENDER.store(1, Ordering::Relaxed);
						succ!()
					}

					// Scrollbar track: jump-to + start drag
					if in_chat_track {
						let max = cs.max_chat_scroll();
						let off = crate::state::ChatState::scroll_offset_from_track_y(y, chat_track, max);
						cs.set_chat_scroll(off);
						cs.ui.scroll_drag = ScrollDrag::Chat { anchor_y: y, anchor_scroll: off };
						NEED_RENDER.store(1, Ordering::Relaxed);
						succ!()
					}
					if in_side_track {
						let max = cs.max_sidebar_scroll() as usize;
						let off = crate::state::ChatState::scroll_offset_from_track_y(y, side_track, max);
						cs.set_sidebar_scroll(off as u16);
						cs.ui.scroll_drag = ScrollDrag::Sidebar { anchor_y: y, anchor_scroll: off };
						NEED_RENDER.store(1, Ordering::Relaxed);
						succ!()
					}

					// Minimap overflow chevrons: page by viewport
					if in_rect(top_ind) {
						let page = cs.ui.minimap_viewport.max(1) as i32;
						cs.scroll_minimap_by(-page);
						NEED_RENDER.store(1, Ordering::Relaxed);
						succ!()
					}
					if in_rect(bot_ind) {
						let page = cs.ui.minimap_viewport.max(1) as i32;
						cs.scroll_minimap_by(page);
						NEED_RENDER.store(1, Ordering::Relaxed);
						succ!()
					}

					if in_minimap {
						let local_index = minimap_index_at(y - minimap_area.y);
						let user_indices = cs.user_message_indices();
						if local_index < user_indices.len() {
							let real_index = user_indices[local_index];
							cs.scroll_to_message_index(real_index);
							cs.ui.hovered_message_index = Some(real_index);
							cs.ui.hovered_message_since = Some(Instant::now());
							NEED_RENDER.store(1, Ordering::Relaxed);
							succ!()
						}
					}

					// Sidebar: task rows (cycle/remove) · accordion headers — never fall through.
					if in_sidebar {
						// Click a task: ☐ → ◐ → ☑ → ☒ → remove
						for (task_idx, area) in cs.ui.sidebar_task_areas.iter() {
							if x >= area.x && x < area.x + area.width && y >= area.y && y < area.y + area.height {
								let before = cs.sidebar.snapshot().tasks.len();
								cs.sidebar.cycle_task(*task_idx);
								let after = cs.sidebar.snapshot().tasks.len();
								if after < before {
									cs.show_toast("Task removed".into());
								} else if let Some(t) = cs.sidebar.snapshot().tasks.get(*task_idx) {
									cs.show_toast(format!(
										"Task {} · {}",
										t.status.glyph(),
										t.content.chars().take(32).collect::<String>()
									));
								}
								cs.play_sound(SoundCue::Confirm);
								NEED_RENDER.store(1, Ordering::Relaxed);
								succ!()
							}
						}
						for (prompt_idx, area) in cs.ui.sidebar_prompt_areas.iter() {
							if x >= area.x && x < area.x + area.width && y >= area.y && y < area.y + area.height {
								cs.sidebar.remove_prompt(*prompt_idx);
								cs.show_toast("Prompt removed".into());
								cs.play_sound(SoundCue::Confirm);
								NEED_RENDER.store(1, Ordering::Relaxed);
								succ!()
							}
						}
						if let Some(area) = cs.ui.sidebar_note_area
							&& x >= area.x
							&& x < area.x + area.width
							&& y >= area.y
							&& y < area.y + area.height
						{
							cs.ui.dialog = CommandDialog::Note;
							cs.ui.dialog_input = cs.sidebar.snapshot().note;
							cs.ui.dialog_cursor = 0;
							cs.show_toast("Edit note · Enter save · Esc cancel".into());
							cs.play_sound(SoundCue::Confirm);
							NEED_RENDER.store(1, Ordering::Relaxed);
							succ!()
						}
						for (i, area) in cs.ui.sidebar_areas.iter().enumerate() {
							if x >= area.x && x < area.x + area.width && y >= area.y && y < area.y + area.height {
								cs.ui.accordion_open[i] = !cs.ui.accordion_open[i];
								NEED_RENDER.store(1, Ordering::Relaxed);
								succ!()
							}
						}
						// Click on sidebar body/title/footer: swallow (no folder nav).
						NEED_RENDER.store(1, Ordering::Relaxed);
						succ!()
					}

					// Bottom bar center chips (permission / plan / goal / profile)
					if let Some(action) = cs.center_chip_at(x, y) {
						cs.handle_center_action(action);
						cs.play_sound(SoundCue::Confirm);
						NEED_RENDER.store(1, Ordering::Relaxed);
						succ!()
					}

					// Bottom bar: mode · runtime · model · diffs
					let plan = cs.ui.plan_button_area;
					let local = cs.ui.local_button_area;
					let model = cs.ui.model_button_area;
					let diff = cs.ui.diff_button_area;
					if in_rect(plan) {
						cs.open_popup(BottomPopup::AgentMode);
						cs.play_sound(SoundCue::MenuOpen);
						NEED_RENDER.store(1, Ordering::Relaxed);
						succ!()
					}
					if in_rect(local) {
						cs.open_popup(BottomPopup::Runtime);
						cs.play_sound(SoundCue::MenuOpen);
						NEED_RENDER.store(1, Ordering::Relaxed);
						succ!()
					}
					if in_rect(model) {
						cs.open_popup(BottomPopup::Models);
						cs.play_sound(SoundCue::MenuOpen);
						NEED_RENDER.store(1, Ordering::Relaxed);
						succ!()
					}
					if in_rect(diff) {
						cs.open_differ();
						cs.play_sound(SoundCue::Confirm);
						NEED_RENDER.store(1, Ordering::Relaxed);
						succ!()
					}

					// Message list: Block toggle (Thought/Command/Subagent expand footer), else text selection.
					if in_chat && !cs.messages.is_empty() {
						if let Some((idx, kind)) = cs.interactive_block_at(x, y)
							&& let Some(msg) = cs.messages.get_mut(idx)
						{
							use crate::components::InteractiveBlock;
							match kind {
								InteractiveBlock::Thinking => {
									if msg.has_thinking() {
										msg.toggle_thinking();
									}
								}
								InteractiveBlock::Command { index } => {
									msg.toggle_command_at(index);
								}
								InteractiveBlock::Subagent { index } => {
									msg.toggle_subagent_at(index);
								}
								InteractiveBlock::Approval => {
									// Focus permission — y/a/n still decide.
									cs.show_toast("Permission · y once · a always · n deny".into());
								}
								InteractiveBlock::PermissionAction { action } => {
									use crate::tools::PermissionDecision;
									let d = match action {
										0 => PermissionDecision::AllowOnce,
										1 => PermissionDecision::AllowAlways,
										_ => PermissionDecision::Deny,
									};
									cs.reply_permission(d);
								}
								InteractiveBlock::QuestionOption { index } => {
									if let Some(q) = cs.question_hub.pending() {
										let cur = q.selected as i32;
										let delta = index as i32 - cur;
										if delta != 0 {
											cs.question_hub.move_selection(delta);
										}
										if let Some(ans) = cs.question_hub.confirm() {
											cs.show_toast(format!("Answered · {ans}"));
										}
									}
								}
								InteractiveBlock::QuestionConfirm => {
									if let Some(ans) = cs.question_hub.confirm() {
										cs.show_toast(format!("Answered · {ans}"));
									}
								}
								InteractiveBlock::DiffReview { index, action } => {
									cs.diff_review_action(index, action);
								}
								InteractiveBlock::OpenPath { index } => {
									cs.diff_review_action(index, 2);
								}
								InteractiveBlock::Plan | InteractiveBlock::PlanStep { .. } => {
									cs.show_toast("Plan · toggle steps in body".into());
								}
								InteractiveBlock::PtyAttach { session_id_hash } => {
									if let Some(id) = cs.resolve_pty_hash(session_id_hash) {
										if cs.pty_host.attached_id.as_deref() == Some(id.as_str()) {
											cs.pty_host.detach_all();
											cs.show_toast("Terminal detached".into());
										} else {
											cs.pty_host.attach(&id);
											cs.show_toast("Terminal attached · Esc detach".into());
										}
										cs.sync_pty_parts_into_messages();
									}
								}
								InteractiveBlock::PtyKill { session_id_hash } => {
									if let Some(id) = cs.resolve_pty_hash(session_id_hash) {
										cs.pty_host.kill(&id);
										cs.sync_pty_parts_into_messages();
										cs.show_toast("Terminal killed".into());
									}
								}
								InteractiveBlock::ContextGroup => {
									cs.show_toast("Context group · expand individual tools above".into());
								}
								InteractiveBlock::Regenerate => {
									let _ = cs.regenerate_last_assistant();
								}
								InteractiveBlock::BranchFromHere => {
									let _ = cs.branch_from_message(idx);
								}
							}
							NEED_RENDER.store(1, Ordering::Relaxed);
							succ!()
						}

						if let Some((idx, char_idx)) = cs.selection_char_at_pointer(x, y) {
							cs.ui.chat_select_anchor = Some(idx);
							cs.ui.chat_select_end = Some(idx);
							cs.ui.active_message_index = Some(idx);
							cs.ui.chat_mouse_selecting = true;
							cs.ui.chat_text_selection_start = Some((idx, char_idx));
							cs.ui.chat_text_selection_end = Some((idx, char_idx));
							cs.input.clear_selection();
							NEED_RENDER.store(1, Ordering::Relaxed);
							succ!()
						}
					}
				}
				MouseEventKind::Up(MouseButton::Left) => {
					if cs.ui.scroll_drag != ScrollDrag::None {
						cs.ui.scroll_drag = ScrollDrag::None;
						NEED_RENDER.store(1, Ordering::Relaxed);
						succ!()
					}
					if cs.input.mouse_selecting {
						cs.input.end_mouse_select();
						// Auto-copy input selection when non-empty.
						if let Some(text) = cs.copy_any_selection() {
							cs.show_toast(format!("✓ Copied {} chars", text.chars().count()));
							cs.play_sound(SoundCue::Confirm);
						}
						NEED_RENDER.store(1, Ordering::Relaxed);
						succ!()
					}
					// Finalize chat text selection: auto-copy when range is non-empty;
					// clear zero-width click so selection never gets stuck.
					if cs.ui.chat_mouse_selecting || cs.ui.chat_select_anchor.is_some() {
						cs.ui.chat_mouse_selecting = false;
						let empty = match (cs.ui.chat_text_selection_start, cs.ui.chat_text_selection_end) {
							(Some(a), Some(b)) => a == b,
							_ => true,
						};
						if empty {
							cs.clear_chat_selection();
						} else if let Some(text) = cs.copy_any_selection() {
							cs.show_toast(format!("✓ Copied {} chars", text.chars().count()));
							cs.play_sound(SoundCue::Confirm);
							// Keep highlight after copy so the user sees what was copied.
						}
						NEED_RENDER.store(1, Ordering::Relaxed);
						succ!()
					}
				}
				MouseEventKind::Drag(MouseButton::Left) => {
					// Input text drag-select
					if in_input_text || cs.input.mouse_selecting {
						if !cs.input.mouse_selecting {
							let col = x.saturating_sub(input_text.x);
							let row = y.saturating_sub(input_text.y);
							cs.input.begin_mouse_select(col, row);
						} else {
							let col = x.saturating_sub(input_text.x);
							let row = y.saturating_sub(input_text.y);
							cs.input.update_mouse_select(col, row);
						}
						cs.clear_chat_selection();
						NEED_RENDER.store(1, Ordering::Relaxed);
						succ!()
					}
					// Message list drag-select
					if in_chat
						&& (cs.ui.chat_mouse_selecting || cs.ui.chat_select_anchor.is_some())
						&& let Some((idx, char_idx)) = cs.selection_char_at_pointer(x, y)
					{
						cs.ui.chat_select_end = Some(idx);
						cs.ui.chat_mouse_selecting = true;
						cs.ui.chat_text_selection_end = Some((idx, char_idx));
						NEED_RENDER.store(1, Ordering::Relaxed);
						succ!()
					}
					// Chat scroll drag initiation
					if cs.ui.scroll_drag == ScrollDrag::None {
						if in_chat_track {
							let max = cs.max_chat_scroll();
							let off = crate::state::ChatState::scroll_offset_from_track_y(y, chat_track, max);
							cs.set_chat_scroll(off);
							cs.ui.scroll_drag = ScrollDrag::Chat { anchor_y: y, anchor_scroll: off };
						} else if in_side_track {
							let max = cs.max_sidebar_scroll() as usize;
							let off = crate::state::ChatState::scroll_offset_from_track_y(y, side_track, max);
							cs.set_sidebar_scroll(off as u16);
							cs.ui.scroll_drag = ScrollDrag::Sidebar { anchor_y: y, anchor_scroll: off };
						}
					}
					match cs.ui.scroll_drag {
						ScrollDrag::Chat { anchor_y, anchor_scroll } => {
							let max = cs.max_chat_scroll();
							let off = crate::state::ChatState::scroll_offset_from_drag(
								y,
								anchor_y,
								anchor_scroll,
								chat_track,
								max,
							);
							cs.set_chat_scroll(off);
							NEED_RENDER.store(1, Ordering::Relaxed);
							succ!()
						}
						ScrollDrag::Sidebar { anchor_y, anchor_scroll } => {
							let max = cs.max_sidebar_scroll() as usize;
							let off = crate::state::ChatState::scroll_offset_from_drag(
								y,
								anchor_y,
								anchor_scroll,
								side_track,
								max,
							);
							cs.set_sidebar_scroll(off as u16);
							NEED_RENDER.store(1, Ordering::Relaxed);
							succ!()
						}
						ScrollDrag::None => {}
						ScrollDrag::DiffTree { .. } | ScrollDrag::DiffPatch { .. } | ScrollDrag::FileBrowser { .. } => {}
					}
				}
				MouseEventKind::ScrollUp => {
					// Bottom input strip (shared in FilePicker) still owns its own scroll.
					if in_input {
						cs.input.vertical_scroll = cs.input.vertical_scroll.saturating_sub(1);
						NEED_RENDER.store(1, Ordering::Relaxed);
						succ!()
					}
					// When the file browser is visible, fall through so wheel scrolls files.
					if !fb_on_screen {
						if in_minimap_zone {
							cs.scroll_minimap_by(-MINIMAP_WHEEL);
							NEED_RENDER.store(1, Ordering::Relaxed);
							succ!()
						}
						if in_sidebar {
							cs.scroll_sidebar_by(-sidebar_wheel);
							NEED_RENDER.store(1, Ordering::Relaxed);
							succ!()
						}
						if in_chat {
							cs.scroll_chat_by(-chat_wheel);
							NEED_RENDER.store(1, Ordering::Relaxed);
							succ!()
						}
						// Chat session owns the wheel — never change folders underneath.
						NEED_RENDER.store(1, Ordering::Relaxed);
						succ!()
					}
				}
				MouseEventKind::ScrollDown => {
					if in_input {
						let max = cs.input.line_count_display().saturating_sub(1);
						cs.input.vertical_scroll = (cs.input.vertical_scroll + 1).min(max);
						NEED_RENDER.store(1, Ordering::Relaxed);
						succ!()
					}
					if !fb_on_screen {
						if in_minimap_zone {
							cs.scroll_minimap_by(MINIMAP_WHEEL);
							NEED_RENDER.store(1, Ordering::Relaxed);
							succ!()
						}
						if in_sidebar {
							cs.scroll_sidebar_by(sidebar_wheel);
							NEED_RENDER.store(1, Ordering::Relaxed);
							succ!()
						}
						if in_chat {
							cs.scroll_chat_by(chat_wheel);
							NEED_RENDER.store(1, Ordering::Relaxed);
							succ!()
						}
						// Chat session owns the wheel — never change folders underneath.
						NEED_RENDER.store(1, Ordering::Relaxed);
						succ!()
					}
				}
				_ => {
					// Swallow unhandled mouse over chat UI so the file browser
					// does not interpret clicks as directory navigation — but only
					// when the file browser is not the active surface.
					if !fb_on_screen {
						if in_sidebar || in_chat || in_input || in_minimap_zone {
							NEED_RENDER.store(1, Ordering::Relaxed);
							succ!()
						}
						NEED_RENDER.store(1, Ordering::Relaxed);
						succ!()
					} else if in_input {
						// Keep bottom input chrome from triggering FB navigation.
						NEED_RENDER.store(1, Ordering::Relaxed);
						succ!()
					}
				}
			}
		}

		// Editor mouse: forward to editor when in Editor mode
		if self.app.bridge.mode == AppMode::Editor && self.app.bridge.editor_adapter.is_initialized() {
			let needs_render =
				self.app.bridge.editor_adapter.handle_event(crossterm::event::Event::Mouse(mouse))?;
			if needs_render {
				NEED_RENDER.store(1, Ordering::Relaxed);
			}
			succ!()
		}

		// File-browser mouse only when the file browser is actually on screen.
		// Chat UI (messages, sidebar, bottom bar) must never change cwd/folders.
		let chat = &self.app.bridge.chat_state;
		let fb_on_screen = matches!(self.app.bridge.mode, crate::bridge::AppMode::FilePicker)
			|| (chat.animation.animation_mode
				&& chat.messages.is_empty()
				&& chat.current_animation() == crate::state::AnimationType::FileBrowser);

		if !fb_on_screen {
			succ!()
		}

		// File-browser scrollbar interaction (editor-style track/thumb drag)
		{
			let cs = &mut self.app.bridge.chat_state;
			let track = cs.ui.fb_scrollbar_area;
			if track.width > 0 && track.height > 0 {
				let scrollbar_col = track.x;
				let was_dragging = matches!(cs.ui.scroll_drag, ScrollDrag::FileBrowser { .. });
				if mouse.column == scrollbar_col || was_dragging {
					// Snapshot folder data before mutable core access
					let (fb_total, _fb_offset) = {
						let folder = &self.app.core.mgr.active().current;
						(folder.files.len(), folder.offset)
					};
					let visible = track.height.max(1) as usize;
					// Active scrollbar drag must win (pointer may leave the column).
					match cs.ui.scroll_drag {
						ScrollDrag::FileBrowser { anchor_y, anchor_scroll } => match mouse.kind {
							MouseEventKind::Drag(MouseButton::Left) | MouseEventKind::Moved => {
								let max_scroll = fb_total.saturating_sub(visible);
								let off = crate::state::ChatState::scroll_offset_from_drag(
									mouse.row,
									anchor_y,
									anchor_scroll,
									track,
									max_scroll,
								);
								let f = self.app.core.current_mut();
								f.offset = off.min(max_scroll);
								f.cursor = f.cursor.min(fb_total.saturating_sub(1));
								NEED_RENDER.store(1, Ordering::Relaxed);
								succ!()
							}
							MouseEventKind::Up(_) => {
								cs.ui.scroll_drag = ScrollDrag::None;
								NEED_RENDER.store(1, Ordering::Relaxed);
								succ!()
							}
							_ => {}
						},
						_ => {}
					}
					match mouse.kind {
						MouseEventKind::Down(MouseButton::Left) => {
							let max_scroll = fb_total.saturating_sub(visible);
							let off = crate::state::ChatState::scroll_offset_from_track_y(
								mouse.row,
								track,
								max_scroll,
							);
							let f = self.app.core.current_mut();
							f.offset = off.min(max_scroll);
							f.cursor = f.cursor.min(fb_total.saturating_sub(1));
							cs.ui.scroll_drag =
								ScrollDrag::FileBrowser { anchor_y: mouse.row, anchor_scroll: off };
							NEED_RENDER.store(1, Ordering::Relaxed);
							succ!()
						}
						// Hover highlight
						MouseEventKind::Moved => {
							let hovered = mouse.column == scrollbar_col
								&& mouse.row >= track.y
								&& mouse.row < track.bottom();
							if cs.ui.fb_scrollbar_hovered != hovered {
								cs.ui.fb_scrollbar_hovered = hovered;
								NEED_RENDER.store(1, Ordering::Relaxed);
							}
							succ!()
						}
						MouseEventKind::Up(_) => {
							cs.ui.scroll_drag = ScrollDrag::None;
							NEED_RENDER.store(1, Ordering::Relaxed);
							succ!()
						}
						_ => {}
					}
				}
			}
		}

		let cx = &mut Ctx::active(&mut self.app.core, &mut self.app.term);
		act!(app:mouse, cx, mouse)?;
		NEED_RENDER.store(1, Ordering::Relaxed);
		succ!()
	}

	#[inline]
	fn dispatch_resize(&mut self) -> Result<Data> {
		if self.app.bridge.mode == AppMode::Editor {
			if let Ok((w, h)) = crossterm::terminal::size() {
				let needs_render =
					self.app.bridge.editor_adapter.handle_event(crossterm::event::Event::Resize(w, h))?;
				if needs_render {
					NEED_RENDER.store(1, Ordering::Relaxed);
				}
			}
			return Ok(Data::Nil);
		}
		let cx = &mut Ctx::active(&mut self.app.core, &mut self.app.term);
		act!(app:resize, cx, crate::root::TerminalRoot::reflow as fn(_) -> _)
	}

	#[inline]
	fn dispatch_focus(&mut self) -> Result<Data> {
		let cx = &mut Ctx::active(&mut self.app.core, &mut self.app.term);
		act!(app:focus, cx)
	}

	#[inline]
	fn dispatch_paste(&mut self, str: String) -> Result<Data> {
		// Prefer chat input when not using the file-browser text field
		if !self.app.core.input.visible {
			use crate::input::InputAction;
			let action = self.app.bridge.chat_state.input.paste_text(&str);
			match action {
				InputAction::Pasted { lines, chars } => {
					let toast = if lines >= 2 || chars >= 80 {
						format!("[pasted {lines} lines]")
					} else {
						format!("Pasted {chars} chars")
					};
					self.app.bridge.chat_state.show_toast(toast);
				}
				InputAction::Attached { name } => {
					self.app.bridge.chat_state.show_toast(format!("Attached {name}"));
				}
				_ => {}
			}
			NEED_RENDER.store(1, Ordering::Relaxed);
			succ!();
		}

		if self.app.core.input.visible {
			let input = &mut self.app.core.input;
			if input.mode() == InputMode::Insert {
				input.type_str(&str)?;
			} else if input.mode() == InputMode::Replace {
				input.replace_str(&str)?;
			}
		}
		succ!();
	}

	#[inline]
	fn dispatch_timer(&mut self) -> Result<Data> {
		// Editor tick: pump async messages when editor is active
		if (self.app.bridge.mode == AppMode::Editor || self.app.bridge.editor_adapter.is_initialized())
			&& let Ok(needs_render) = self.app.bridge.editor_adapter.tick()
			&& needs_render
		{
			NEED_RENDER.store(1, Ordering::Relaxed);
		}

		// Timer tick for animations - just trigger a render
		// The effects are time-based and will automatically show updated colors

		// Voice frequency bars (Ctrl+S listen mode)
		self.app.bridge.chat_state.voice_state.panel.tick_waves();

		// Update chat state (process LLM responses)
		self.app.bridge.chat_state.update();
		if self.app.bridge.chat_state.take_pending_quit() {
			AppProxy::quit(QuitOpt::default());
		}

		// Update splash font cycling (every 3 seconds)
		if self.app.bridge.chat_state.animation.animation_mode
			&& self.app.bridge.chat_state.animation.last_font_change.elapsed() >= Duration::from_secs(3)
		{
			let current_anim = self.app.bridge.chat_state.current_animation();
			if current_anim == crate::AnimationType::Splash {
				let n = crate::splash::splash_font_count().max(1);
				self.app.bridge.chat_state.animation.splash_font_index =
					(self.app.bridge.chat_state.animation.splash_font_index + 1) % n;
				self.app.bridge.chat_state.animation.last_font_change = Instant::now();
			}
		}

		// Update Menu timing
		let elapsed = self.app.bridge.chat_state.last_frame_instant.elapsed();
		self.app.bridge.chat_state.menu.update(elapsed);
		self.app.bridge.chat_state.last_frame_instant = Instant::now();

		NEED_RENDER.store(1, Ordering::Relaxed);
		succ!();
	}
}
