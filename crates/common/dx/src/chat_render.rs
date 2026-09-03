use ratatui::{
	buffer::Buffer,
	layout::{Constraint, Direction, Layout, Rect},
	style::{Color, Modifier, Style},
	text::{Line, Span, Text},
	widgets::{Block, Borders, Paragraph, Widget},
};
use unicode_width::UnicodeWidthStr;

use super::{
	components::{self, MessageList},
	state::{AnimationType, ChatState},
};

fn truncate_start(value: &str, max_chars: usize) -> String {
	let char_count = value.chars().count();
	if char_count <= max_chars {
		return value.to_string();
	}

	let visible_chars = max_chars.saturating_sub(2);
	let suffix: String = value.chars().skip(char_count.saturating_sub(visible_chars)).collect();
	format!("..{suffix}")
}

/// Collapse assistant/user wire content into a single preview paragraph for the minimap card.
fn clean_hover_preview_text(content: &str) -> String {
	content
		.lines()
		.map(str::trim)
		.filter(|line| {
			!line.is_empty()
				&& !line.starts_with("<think")
				&& !line.starts_with("</think")
				&& !line.starts_with("<thinking")
				&& !line.starts_with("</thinking")
				&& !line.starts_with("TITLE:")
				&& !line.starts_with("```")
				&& !line.starts_with("__")
		})
		.map(|line| line.trim_start_matches(['#', '-', '*', '>', '•']).trim())
		.filter(|line| !line.is_empty())
		.take(12)
		.collect::<Vec<_>>()
		.join(" ")
}

/// Ellipsize a styled line to `max_cols` display columns, ending with `…`.
fn ellipsize_line_spans(line: &Line<'_>, max_cols: usize) -> Span<'static> {
	use unicode_width::UnicodeWidthChar;
	let plain: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
	if max_cols == 0 {
		return Span::raw(String::new());
	}
	if plain.width() <= max_cols {
		return Span::raw(plain);
	}
	if max_cols == 1 {
		return Span::raw("…");
	}
	let keep = max_cols.saturating_sub(1);
	let mut out = String::new();
	let mut cols = 0usize;
	for ch in plain.chars() {
		let w = UnicodeWidthChar::width(ch).unwrap_or(0);
		if cols + w > keep {
			break;
		}
		out.push(ch);
		cols += w;
	}
	out.push('…');
	Span::raw(out)
}

impl ChatState {
	pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
		self.poll_codex_events();
		self.ui.rendered_area = area;
		// Update tachyon effects timing
		let _elapsed = self.last_render.elapsed();
		self.set_last_animation_area_width(area.width);

		// Paint the full surface with the active theme background first so theme
		// changes always recolor the whole chat TUI (not only bordered widgets).
		let theme_bg = self.theme.bg;
		for y in area.top()..area.bottom() {
			for x in area.left()..area.right() {
				let cell = &mut buf[(x, y)];
				cell.reset();
				cell.set_bg(theme_bg);
			}
		}

		// Full clear frames (post-train / second Ctrl+C) before session continue text.
		if self.session.force_clear_frames > 0 {
			// Keep the theme plate so exit frames do not flash the terminal default.
			self.session.force_clear_frames = self.session.force_clear_frames.saturating_sub(1);
			return;
		}

		// Full-screen git differ (left tree / right patch) + shared chat input chrome.
		if self.diff_state.open {
			let suggest_h = self.input.suggestion_bar_height();
			let input_h = self.input.preferred_height();
			let bottom_h = suggest_h
				.saturating_add(input_h)
				.saturating_add(1) // bottom controls
				.max(2)
				.min(area.height.saturating_sub(6).max(2));
			let chunks = Layout::default()
				.direction(Direction::Vertical)
				.constraints([Constraint::Min(6), Constraint::Length(bottom_h)])
				.split(area);

			crate::diff_view::render_diff_view(&mut self.diff_state, &self.theme, chunks[0], buf);
			// chat_list_area doubles as diff viewport height for PageUp/Down.
			self.ui.chat_list_area = chunks[0];
			self.render_bottom_chat_chrome(chunks[1], buf);
			self.render_toast(area, buf);
			return;
		}

		// PRIORITY 0: Playing intro/outro transition animations
		if self.animation.playing_intro || self.animation.playing_outro {
			if self.animation.playing_outro {
				// Outro: full-screen animation only, no input/controls
				for y in area.top()..area.bottom() {
					for x in area.left()..area.right() {
						buf[(x, y)].reset();
						buf[(x, y)].set_bg(self.theme_bg_color());
					}
				}
				if self.animation.outro_animation == AnimationType::Train {
					self.render_train_animation_in_area(area, buf)
				}
				self.render_toast(area, buf);
				return;
			}

			// Intro: animation area + input box + controls
			let input_h = self.input.preferred_height();
			let chunks = Layout::default()
				.direction(Direction::Vertical)
				.constraints([Constraint::Min(9), Constraint::Length(input_h), Constraint::Length(1)])
				.split(area);

			self.ui.input_area = chunks[1];

			for y in chunks[0].top()..chunks[0].bottom() {
				for x in chunks[0].left()..chunks[0].right() {
					buf[(x, y)].reset();
					buf[(x, y)].set_bg(self.theme_bg_color());
				}
			}

			match self.animation.intro_animation {
				AnimationType::Train => self.render_train_animation_in_area(chunks[0], buf),
				AnimationType::Matrix => self.render_matrix_animation_in_area(chunks[0], buf),
				AnimationType::Confetti => self.render_confetti_animation_in_area(chunks[0], buf),
				AnimationType::GameOfLife => self.render_gameoflife_animation_in_area(chunks[0], buf),
				AnimationType::Starfield => self.render_starfield_animation_in_area(chunks[0], buf),
				AnimationType::Rain => self.render_rain_animation_in_area(chunks[0], buf),
				AnimationType::NyanCat => {}
				AnimationType::DVDLogo => self.render_dvdlogo_animation_in_area(chunks[0], buf),
				AnimationType::Fire => self.render_fire_animation_in_area(chunks[0], buf),
				AnimationType::Plasma => self.render_plasma_animation_in_area(chunks[0], buf),
				AnimationType::Waves => self.render_waves_animation_in_area(chunks[0], buf),
				AnimationType::Fireworks => self.render_fireworks_animation_in_area(chunks[0], buf),
				_ => {}
			}

			self.render_input_box(chunks[1], buf);
			let (plan_area, model_area, _token_area, local_area) =
				self.render_bottom_controls(chunks[2], buf, false);
			self.ui.plan_button_area = plan_area;
			self.ui.model_button_area = model_area;
			self.ui.local_button_area = local_area;

			self.render_toast(area, buf);
			return;
		}

		// Both animations show in full screen, no input or controls
		if self.animation.show_train_animation || self.animation.show_matrix_animation {
			// Clear the entire area first
			for y in area.top()..area.bottom() {
				for x in area.left()..area.right() {
					buf[(x, y)].reset();
					buf[(x, y)].set_bg(self.theme_bg_color());
				}
			}

			// Render appropriate animation in the full area
			if self.animation.show_train_animation {
				self.render_train_animation_in_area(area, buf);
			} else if self.animation.show_matrix_animation {
				self.render_matrix_animation_in_area(area, buf);
			}
			return;
		}

		// Animation carousel mode
		if self.animation.animation_mode {
			let current_anim = self.current_animation();

			// The file browser is rendered by the root widget so it can share the frame.
			if current_anim == AnimationType::FileBrowser {
				let suggest_h = self.input.suggestion_bar_height();
				let input_h = self.input.preferred_height();
				let chunks = Layout::default()
					.direction(Direction::Vertical)
					.constraints([
						Constraint::Min(9),
						Constraint::Length(suggest_h),
						Constraint::Length(input_h),
						Constraint::Length(1),
						Constraint::Length(1), // bottom margin
					])
					.split(area);

				self.ui.input_area = chunks[2];
				if suggest_h > 0 {
					self.render_suggestion_bar(chunks[1], buf);
				}
				self.render_input_box(chunks[2], buf);
				let (plan_area, model_area, _token_area, local_area) =
					self.render_bottom_controls(chunks[3], buf, false);
				self.ui.plan_button_area = plan_area;
				self.ui.model_button_area = model_area;
				self.ui.local_button_area = local_area;
				return;
			}

			// Matrix animation - show with input and controls
			if current_anim == AnimationType::Matrix {
				let suggest_h = self.input.suggestion_bar_height();
				let input_h = self.input.preferred_height();
				let chunks = Layout::default()
					.direction(Direction::Vertical)
					.constraints([
						Constraint::Min(9),
						Constraint::Length(suggest_h),
						Constraint::Length(input_h),
						Constraint::Length(1),
						Constraint::Length(1), // bottom margin
					])
					.split(area);

				self.ui.input_area = chunks[2];
				if suggest_h > 0 {
					self.render_suggestion_bar(chunks[1], buf);
				}

				// Clear the animation area first
				for y in chunks[0].top()..chunks[0].bottom() {
					for x in chunks[0].left()..chunks[0].right() {
						buf[(x, y)].reset();
						buf[(x, y)].set_bg(self.theme_bg_color());
					}
				}

				// Render animation in the main area
				self.render_matrix_animation_in_area(chunks[0], buf);

				// Render intro/outro indicators (top-left corner)
				self.render_animation_indicators(chunks[0], current_anim, buf);

				// Render input box and bottom controls
				self.render_input_box(chunks[2], buf);
				let (plan_area, model_area, _token_area, local_area) =
					self.render_bottom_controls(chunks[3], buf, false);
				self.ui.plan_button_area = plan_area;
				self.ui.model_button_area = model_area;
				self.ui.local_button_area = local_area;

				// Render menu overlay if visible
				if self.show_tachyon_menu || self.menu_is_closing {
					self.render_menu_in_area(area, buf);
				}

				// Render toast notification (on top of everything)
				self.render_toast(area, buf);
				return;
			}

			// Home / carousel: suggestion bar above input (same as chat)
			let suggest_h = self.input.suggestion_bar_height();
			let input_h = self.input.preferred_height();
			let chunks = Layout::default()
				.direction(Direction::Vertical)
				.constraints([
					Constraint::Min(9),
					Constraint::Length(suggest_h),
					Constraint::Length(input_h),
					Constraint::Length(1),
					Constraint::Length(1), // bottom margin
				])
				.split(area);

			self.ui.input_area = chunks[2];
			if suggest_h > 0 {
				self.render_suggestion_bar(chunks[1], buf);
			}

			// Render the current animation in the chat area
			match current_anim {
				AnimationType::Splash => {
					super::splash::render(
						chunks[0],
						buf,
						&self.theme,
						self.animation.splash_font_index,
						&self.animation.rainbow_animation,
					);
				}
				AnimationType::Train => {
					self.render_train_animation_in_area(chunks[0], buf);
				}
				AnimationType::Matrix => {
					// Already handled above
				}
				AnimationType::Confetti => {
					self.render_confetti_animation_in_area(chunks[0], buf);
				}
				AnimationType::GameOfLife => {
					self.render_gameoflife_animation_in_area(chunks[0], buf);
				}
				AnimationType::Starfield => {
					self.render_starfield_animation_in_area(chunks[0], buf);
				}
				AnimationType::Rain => {
					self.render_rain_animation_in_area(chunks[0], buf);
				}
				AnimationType::NyanCat => {}
				AnimationType::DVDLogo => {
					self.render_dvdlogo_animation_in_area(chunks[0], buf);
				}
				AnimationType::Fire => {
					self.render_fire_animation_in_area(chunks[0], buf);
				}
				AnimationType::Plasma => {
					self.render_plasma_animation_in_area(chunks[0], buf);
				}
				AnimationType::Waves => {
					self.render_waves_animation_in_area(chunks[0], buf);
				}
				AnimationType::Fireworks => {
					self.render_fireworks_animation_in_area(chunks[0], buf);
				}
				AnimationType::FileBrowser => {
					return;
				}
			}

			// Render intro/outro indicators (top-left corner)
			self.render_animation_indicators(chunks[0], current_anim, buf);

			// Render input box and bottom controls
			self.render_input_box(chunks[2], buf);

			let (plan_area, model_area, _token_area, local_area) =
				self.render_bottom_controls(chunks[3], buf, false);

			self.ui.plan_button_area = plan_area;
			self.ui.model_button_area = model_area;
			self.ui.local_button_area = local_area;

			// Slash-command dialogs + toast
			self.render_command_dialog(area, buf);

			// Render menu overlay if visible (on top of animations)
			if self.show_tachyon_menu || self.menu_is_closing {
				self.render_menu_in_area(area, buf);
			}

			// Render toast notification (on top of everything)
			self.render_toast(area, buf);
			return;
		}

		if self.ui.show_dx_splash {
			// Show DX splash screen
			super::splash::render(
				area,
				buf,
				&self.theme,
				self.animation.splash_font_index,
				&self.animation.rainbow_animation,
			);
			return;
		}

		// Session details screen (shown after train exit animation)
		if self.session.show_session_screen {
			self.render_session_screen(area, buf);
			return;
		}

		let is_low_width = area.width < 100;
		let effective_sidebar = self.ui.show_sidebar && !is_low_width;

		// Message-list screen: [minimap 2 | chat fills rest | sidebar] — zero extra gutters
		let (minimap_area, after_minimap) = {
			let row = Layout::default()
				.direction(Direction::Horizontal)
				.spacing(0)
				.constraints([Constraint::Length(2), Constraint::Min(0)])
				.split(area);
			(row[0], row[1])
		};
		// Full TUI height minimap
		self.render_left_minimap(minimap_area, buf);

		let chat_column = if effective_sidebar {
			let side = Layout::default()
				.direction(Direction::Horizontal)
				.spacing(0)
				// Chat takes all remaining width; sidebar fixed, no spacer margins
				.constraints([Constraint::Min(0), Constraint::Length(40)])
				.split(after_minimap);
			self.render_sidebar(side[1], buf);
			side[0]
		} else {
			self.ui.sidebar_panel_area = Rect::default();
			self.ui.sidebar_area = Rect::default();
			self.ui.sidebar_areas = [Rect::default(); crate::sidebar_data::SIDEBAR_SECTION_COUNT];
			after_minimap
		};

		// 1-line input by default; grows with multi-line content; @// suggestions above
		let suggest_h = self.input.suggestion_bar_height();
		let input_h = self.input.preferred_height();
		let chunks = Layout::default()
			.direction(Direction::Vertical)
			.constraints([
				Constraint::Min(5),
				Constraint::Length(suggest_h),
				Constraint::Length(input_h),
				Constraint::Length(2),
			])
			.split(chat_column);

		self.ui.input_area = chunks[2];
		if suggest_h > 0 {
			self.render_suggestion_bar(chunks[1], buf);
		}

		let (chat_area, inline_sidebar_area) = if self.ui.show_sidebar && is_low_width {
			let c = Layout::default()
				.direction(Direction::Vertical)
				.constraints([Constraint::Length(3), Constraint::Min(5)])
				.split(chunks[0]);
			(c[1], Some(c[0]))
		} else {
			(chunks[0], None)
		};

		if let Some(inline_area) = inline_sidebar_area {
			self.render_inline_sidebar(inline_area, buf);
		}

		// Show splash when no messages, otherwise show message list
		if self.messages.is_empty() {
			self.ui.chat_list_area = chat_area;
			super::splash::render(
				chat_area,
				buf,
				&self.theme,
				self.animation.splash_font_index,
				&self.animation.rainbow_animation,
			);
		} else {
			// 2-col branch rail + message list
			let rail_w = if crate::msg_ui::list_branches(&self.messages, &self.active_branch_id).len() > 1
			{
				2u16
			} else {
				0u16
			};
			let list_area = if rail_w > 0 && chat_area.width > rail_w + 10 {
				let rail = ratatui::layout::Rect {
					x: chat_area.x,
					y: chat_area.y,
					width: rail_w,
					height: chat_area.height,
				};
				crate::msg_ui::render_branch_rail(
					rail,
					buf,
					&self.messages,
					&self.active_branch_id,
					&self.theme,
				);
				ratatui::layout::Rect {
					x: chat_area.x + rail_w,
					y: chat_area.y,
					width: chat_area.width.saturating_sub(rail_w),
					height: chat_area.height,
				}
			} else {
				chat_area
			};
			// Hit-test / selection must use the same rect the list paints into (not the rail).
			self.ui.chat_list_area = list_area;
			// Keep scroll offset in range as the viewport resizes
			let max_chat = self.max_chat_scroll();
			if self.ui.chat_scroll_offset > max_chat {
				self.ui.chat_scroll_offset = max_chat;
			}
			MessageList::with_effects(
				&self.messages,
				&self.theme,
				self.ui.chat_scroll_offset,
				&self.animation.shimmer,
				&self.typing_indicator,
			)
			.show_timestamps(self.ui.show_timestamps)
			.scrollbar_hovered(self.ui.chat_scrollbar_hovered)
			.selection(self.chat_selection_range())
			.text_selection(self.ui.chat_text_selection_start, self.ui.chat_text_selection_end)
			.user_label(&self.user_display_name)
			.streaming(self.is_loading)
			.hovered_message_index(self.ui.hovered_message_index)
			.render(list_area, buf);

			// Branch picker modal (centered over chat)
			crate::msg_ui::render_branch_picker(
				chat_area,
				buf,
				&self.messages,
				&self.active_branch_id,
				&self.branch_picker,
				&self.theme,
			);
		}

		self.render_input_box(chunks[2], buf);

		let (plan_area, model_area, _token_area, local_area) =
			self.render_bottom_controls(chunks[3], buf, true);

		self.ui.plan_button_area = plan_area;
		self.ui.model_button_area = model_area;
		self.ui.local_button_area = local_area;

		// Render performance overlay if enabled
		self.render_perf_overlay(area, buf);

		// Bottom-bar popup menus (modes / models / channels)
		self.render_bottom_popup(area, buf);

		// Slash-command dialogs
		self.render_command_dialog(area, buf);

		// Render menu overlay globally if visible (on top of everything)
		if self.show_tachyon_menu || self.menu_is_closing {
			self.render_menu_in_area(area, buf);
		}

		// Permission / question / plan / goal live in the bottom-bar **center**
		// (OpenCode footer surface) — no floating docks.

		// Render toast notification (on top of everything)
		self.render_minimap_hover_card(area, buf);
		self.render_toast(area, buf);
	}

	pub fn render_dimmed(&mut self, area: Rect, full_area: Rect, buf: &mut Buffer) {
		// FilePicker / compact chrome: suggestion bar + input + bottom controls.
		self.render_bottom_chat_chrome(area, buf);

		// Render menu overlay globally if visible (on top of everything)
		// Use full_area to center menu in the entire terminal, not just the chat area
		if self.show_tachyon_menu || self.menu_is_closing {
			self.render_menu_in_area(full_area, buf);
		}

		// Render toast notification (on top of everything)
		self.render_toast(full_area, buf);
	}

	/// Shared bottom chrome used by FilePicker, Diff, and other compact modes:
	/// optional `/` `@` suggestion list, multi-line input, and bottom controls.
	pub fn render_bottom_chat_chrome(&mut self, area: Rect, buf: &mut Buffer) {
		if area.height == 0 || area.width == 0 {
			return;
		}
		// Paint plate so suggestion rows never sit on garbage glyphs.
		for y in area.top()..area.bottom() {
			for x in area.left()..area.right() {
				let cell = &mut buf[(x, y)];
				cell.reset();
				cell.set_bg(self.theme.bg);
			}
		}

		let suggest_h = self.input.suggestion_bar_height().min(area.height.saturating_sub(2));
		let input_h = self
			.input
			.preferred_height()
			.min(area.height.saturating_sub(suggest_h).saturating_sub(1).max(1));
		let control_h = if area.height > suggest_h.saturating_add(input_h) { 1 } else { 0 };

		let chunks = Layout::default()
			.direction(Direction::Vertical)
			.constraints([
				Constraint::Length(suggest_h),
				Constraint::Length(input_h),
				Constraint::Length(control_h),
			])
			.split(area);

		if suggest_h > 0 {
			self.render_suggestion_bar(chunks[0], buf);
		}
		self.ui.input_area = chunks[1];
		self.render_input_box(chunks[1], buf);
		if control_h > 0 {
			let (plan_area, model_area, _token_area, local_area) =
				self.render_bottom_controls(chunks[2], buf, false);
			self.ui.plan_button_area = plan_area;
			self.ui.model_button_area = model_area;
			self.ui.local_button_area = local_area;
		}
	}
}

// Input rendering methods
impl ChatState {
	/// `/` and `@` suggestion menu — full width, compact rows, theme colors only.
	pub fn render_suggestion_bar(&self, area: Rect, buf: &mut Buffer) {
		if area.height == 0 || area.width == 0 || self.input.suggestions.is_empty() {
			return;
		}

		// Full-width theme card surface (no hard-coded brand colors)
		for y in area.top()..area.bottom() {
			for x in area.left()..area.right() {
				buf[(x, y)].reset();
				buf[(x, y)].set_bg(self.theme.card);
			}
		}

		let kind = match self.input.suggestion_kind {
			Some(crate::input::SuggestKind::Slash) => "/ commands",
			Some(crate::input::SuggestKind::Mention) => "@ mentions",
			None => "suggestions",
		};
		let max_rows = area.height as usize;
		// Compact: no dedicated header row when height is tight — put kind in selected line only
		let list_rows = max_rows;
		let list_y0 = area.y;

		let start = self.input.suggestion_index.saturating_sub(list_rows.saturating_sub(1));
		let slice = &self.input.suggestions[start..];
		let visible: Vec<_> = slice.iter().take(list_rows).enumerate().collect();

		// First pass: max label width across the full list (not just the
		// visible slice) so descriptions stay column-aligned while scrolling.
		// Prefix is always one column (`›` or ` `).
		const LABEL_DESC_GAP: u16 = 2;
		let max_label_w: u16 = self
			.input
			.suggestions
			.iter()
			.map(|item| 1u16 + item.label.width() as u16)
			.max()
			.unwrap_or(0);

		for (row, item) in &visible {
			let idx = start + row;
			let selected = *idx == self.input.suggestion_index;
			let y = list_y0 + *row as u16;

			// Full-width row fill
			for x in area.left()..area.right() {
				let cell = &mut buf[(x, y)];
				if selected {
					cell.set_bg(self.theme.accent);
				} else {
					cell.set_bg(self.theme.card);
				}
			}

			let style = if selected {
				Style::default().fg(self.theme.bg).bg(self.theme.accent).add_modifier(Modifier::BOLD)
			} else {
				Style::default().fg(self.theme.fg).bg(self.theme.card)
			};
			let desc_style = if selected {
				Style::default().fg(self.theme.bg).bg(self.theme.accent)
			} else {
				Style::default().fg(self.theme.muted_fg).bg(self.theme.card)
			};
			let prefix = if selected { "›" } else { " " };
			let label = format!("{prefix}{}", item.label);
			let label_w = label.width() as u16;
			let pad = max_label_w.saturating_sub(label_w) + LABEL_DESC_GAP;
			let rest = area.width.saturating_sub(label_w + pad);
			let desc = if rest > 3 && !item.description.is_empty() {
				let d: String = item.description.chars().take(rest as usize).collect();
				format!("{}{d}", " ".repeat(pad as usize))
			} else {
				String::new()
			};
			// Show kind on first row when at top of list
			let extra =
				if *row == 0 && start == 0 && rest > 20 { format!("  · {kind}") } else { String::new() };
			let line = Line::from(vec![
				Span::styled(label, style),
				Span::styled(desc, desc_style),
				Span::styled(extra, desc_style),
			]);
			Paragraph::new(line).render(Rect { x: area.x, y, width: area.width, height: 1 }, buf);
		}
	}

	pub fn render_input_box(&mut self, area: Rect, buf: &mut Buffer) {
		// Start timing input render
		self.perf_monitor.start_timing();

		let block = Block::default()
			.borders(Borders::ALL)
			.border_style(Style::default().fg(self.theme.border))
			.border_type(ratatui::widgets::BorderType::Rounded)
			.style(Style::default());

		let inner = block.inner(area);
		block.render(area, buf);

		// Attachment chips on the first row when present
		let has_atts = !self.input.attachments.is_empty();
		let attach_h: u16 = if has_atts { 1 } else { 0 };

		// Reserve 1 col for scrollbar when multi-line content overflows
		let display = self.input.display_content();
		let line_count = if display.is_empty() { 1 } else { display.lines().count().max(1) };
		let text_viewport_h = inner.height.saturating_sub(attach_h);
		let needs_scroll = line_count > text_viewport_h as usize && text_viewport_h > 0;
		let bar_w: u16 = if needs_scroll { components::SCROLLBAR_TRACK_WIDTH } else { 0 };
		// Right padding so text + loader never hug the border / scrollbar.
		let right_pad = components::INPUT_BOX_RIGHT_PAD;
		// Spinner (space-hold / load) sits in a reserved column left of the right pad.
		let show_spinner = self.space_held || self.is_loading;
		let spinner_reserve: u16 = if show_spinner { components::INPUT_SPINNER_RESERVE } else { 0 };
		// Ctrl+S voice listen / STT processing: frequency bars on the right of the input box.
		let show_voice_wave = self.voice_state.panel.listening || self.voice_state.panel.processing;
		// Use a wide professional meter; scale with available width.
		let wave_reserve: u16 = if show_voice_wave {
			let max_w = components::INPUT_VOICE_WAVE_RESERVE;
			let avail = inner.width.saturating_sub(12);
			// Prefer ~40% of input width, clamped to [22, max_w]
			((avail * 2) / 5).clamp(22, max_w)
		} else {
			0
		};

		// Content area: left-flush, right inset for wave + spinner + pad + optional scrollbar.
		let content_w = inner
			.width
			.saturating_sub(bar_w)
			.saturating_sub(right_pad)
			.saturating_sub(spinner_reserve)
			.saturating_sub(wave_reserve);
		let padded_inner =
			Rect { x: inner.x, y: inner.y + attach_h, width: content_w, height: text_viewport_h };
		// Expose for mouse drag-select hit testing
		self.ui.input_text_area = padded_inner;

		if has_atts {
			let chips: String =
				self.input.attachments.iter().map(|a| a.label()).collect::<Vec<_>>().join("  ");
			let chip_area = Rect { x: inner.x, y: inner.y, width: content_w, height: 1 };
			Paragraph::new(Span::styled(
				chips,
				Style::default().fg(self.theme.accent).add_modifier(Modifier::BOLD),
			))
			.render(chip_area, buf);
		}

		self.input.ensure_cursor_visible(padded_inner.height as usize);
		self.render_input_text(padded_inner, buf);
		self.render_input_cursor(padded_inner, buf);

		if show_voice_wave && wave_reserve > 0 && text_viewport_h > 0 {
			let wave_area = Rect {
				x: inner.x + content_w + spinner_reserve,
				y: padded_inner.y,
				width: wave_reserve.min(inner.width.saturating_sub(content_w)),
				height: padded_inner.height,
			};
			self.render_input_voice_wave(wave_area, buf);
		}

		if needs_scroll {
			let bar_area = Rect {
				x: inner.x + inner.width.saturating_sub(bar_w),
				y: padded_inner.y,
				width: bar_w,
				height: padded_inner.height,
			};
			components::render_scrollbar_track_hover(
				bar_area,
				buf,
				line_count,
				self.input.vertical_scroll,
				false,
				components::SCROLLBAR_TRACK_WIDTH,
			);
		}

		// Input spinner removed — only bottom bar spinner is used

		self.last_input_render_time = self.perf_monitor.record_input_render();
	}

	/// Professional multi-row frequency meter (Ctrl+S mic / STT processing).
	fn render_input_voice_wave(&self, area: Rect, buf: &mut Buffer) {
		if area.width == 0 || area.height == 0 {
			return;
		}
		let levels = &self.voice_state.panel.wave_levels;
		let peaks = &self.voice_state.panel.wave_peaks;
		let listening = self.voice_state.panel.listening;
		let processing = self.voice_state.panel.processing;

		// Clear wave area with slightly elevated bg for a "meter rack" look
		let rack_bg = ratatui::style::Color::Rgb(0x12, 0x14, 0x18);
		for row in 0..area.height {
			for col in 0..area.width {
				let cell = &mut buf[(area.x + col, area.y + row)];
				cell.reset();
				cell.set_bg(rack_bg);
				cell.set_char(' ');
			}
		}

		// Left status glyph + gap
		let live = listening || processing;
		let label_w: u16 = 2;
		{
			let glyph = if listening {
				'●'
			} else if processing {
				'…'
			} else {
				'○'
			};
			let color = if listening {
				ratatui::style::Color::Rgb(0xff, 0x3b, 0x3b)
			} else if processing {
				ratatui::style::Color::Rgb(0xff, 0xc1, 0x07)
			} else {
				self.theme.border
			};
			let mid_y = area.y + area.height / 2;
			let cell = &mut buf[(area.x, mid_y)];
			cell.set_char(glyph);
			cell.set_fg(color);
			cell.set_bg(rack_bg);
		}

		let meter_x = area.x + label_w;
		let meter_w = area.width.saturating_sub(label_w);
		if meter_w == 0 {
			return;
		}

		// One band per column (or average if more bands than width)
		let bands = levels.len();
		let cols = meter_w as usize;
		let h = area.height as usize;

		for col in 0..cols {
			// Map column → band index (spread full spectrum across width)
			let band = if bands <= 1 { 0 } else { col * (bands - 1) / cols.max(1) };
			let level = levels.get(band).copied().unwrap_or(0.1).clamp(0.0, 1.0);
			let peak = peaks.get(band).copied().unwrap_or(level).clamp(0.0, 1.0);

			// Filled cells from bottom
			let fill = ((level * h as f32).ceil() as usize).min(h);
			let peak_row = h.saturating_sub(1).saturating_sub(
				((peak * (h.saturating_sub(1)) as f32).round() as usize).min(h.saturating_sub(1)),
			);

			for row in 0..h {
				let y = area.y + row as u16;
				let x = meter_x + col as u16;
				let from_bottom = h - 1 - row;
				let cell = &mut buf[(x, y)];
				cell.set_bg(rack_bg);

				if from_bottom < fill {
					// Gradient: green (low) → yellow → red (hot)
					let t = (from_bottom as f32 + 1.0) / h.max(1) as f32;
					let color = if t > 0.82 {
						ratatui::style::Color::Rgb(0xff, 0x4d, 0x4d)
					} else if t > 0.55 {
						ratatui::style::Color::Rgb(0xff, 0xd1, 0x66)
					} else if live {
						ratatui::style::Color::Rgb(0x3d, 0xdc, 0x97)
					} else {
						self.theme.accent
					};
					// Solid block for professional meter
					cell.set_char('█');
					cell.set_fg(color);
				} else if row == peak_row && peak > 0.12 {
					// Peak hold tick
					cell.set_char('▬');
					cell.set_fg(ratatui::style::Color::Rgb(0xe8, 0xea, 0xed));
				} else if from_bottom == 0 {
					// Baseline
					cell.set_char('▁');
					cell.set_fg(ratatui::style::Color::Rgb(0x2a, 0x2e, 0x36));
				}
			}
		}
	}

	fn render_input_text(&self, area: Rect, buf: &mut Buffer) {
		if area.width == 0 || area.height == 0 {
			return;
		}

		let placeholder = " The Development Experience You Deserve...";
		let display = self.input.display_content();

		// Always use cell-by-cell paint so mouse select & Ctrl+A share one path
		if display.is_empty() && self.input.attachments.is_empty() {
			// Soft but readable (theme.border is near-black; muted_fg is still dull —
			// use a lighter gray so the placeholder is easy to see).
			let placeholder_fg = self.theme.muted_fg;
			Paragraph::new(Text::from(Line::from(Span::styled(
				placeholder,
				Style::default().fg(placeholder_fg).add_modifier(Modifier::DIM),
			))))
			.render(area, buf);
			return;
		}

		self.render_input_content_with_selection(area, buf);
	}

	/// Paint input text with optional selection (Ctrl+A and mouse drag share this).
	/// Selected cells: `bg(theme.fg) + fg(theme.bg)` — same invert as Ctrl+A.
	fn render_input_content_with_selection(&self, area: Rect, buf: &mut Buffer) {
		let content = self.input.display_content();
		let selected_style = Style::default().bg(self.theme.fg).fg(self.theme.bg);
		let normal_style = Style::default().bg(self.theme.bg).fg(self.theme.fg);

		let (sel_start, sel_end) = if self.input.has_selection() {
			match (self.input.selection_start, self.input.selection_end) {
				(Some(a), Some(b)) => {
					if a < b {
						(a, b)
					} else {
						(b, a)
					}
				}
				_ => (0, 0),
			}
		} else {
			(0, 0)
		};
		let has_sel = sel_start != sel_end;

		// Clear area with theme bg so selection has a solid invert fill
		for row in 0..area.height {
			for col in 0..area.width {
				let cell = &mut buf[(area.x + col, area.y + row)];
				cell.reset();
				cell.set_bg(self.theme.bg);
			}
		}

		// Line list with trailing empty line after final `\n`
		let mut lines: Vec<(usize, &str)> = Vec::new();
		let mut offset = 0usize;
		if content.is_empty() {
			lines.push((0, ""));
		} else {
			for (i, line) in content.split('\n').enumerate() {
				if i > 0 {
					offset += 1;
				}
				lines.push((offset, line));
				offset += line.len();
			}
			if content.ends_with('\n') {
				lines.push((content.len(), ""));
			}
		}

		let start_line = self.input.vertical_scroll.min(lines.len().saturating_sub(1));
		let end_line = (start_line + area.height as usize).min(lines.len());

		// Selection indices are on raw `input.content`; map via same split on raw
		let raw = &self.input.content;
		let mut raw_starts: Vec<usize> = Vec::new();
		{
			let mut off = 0usize;
			if raw.is_empty() {
				raw_starts.push(0);
			} else {
				for (i, line) in raw.split('\n').enumerate() {
					if i > 0 {
						off += 1;
					}
					raw_starts.push(off);
					off += line.len();
				}
				if raw.ends_with('\n') {
					raw_starts.push(raw.len());
				}
			}
		}

		for (li, &(_disp_off, line)) in lines[start_line..end_line].iter().enumerate() {
			let li = start_line + li;
			let y = area.y + (li - start_line) as u16;
			if y >= area.bottom() {
				break;
			}
			let line_byte_start = raw_starts.get(li).copied().unwrap_or(0);
			let mut x = area.x;
			let mut col_byte = line_byte_start;

			if line.is_empty() {
				let is_selected = has_sel && col_byte >= sel_start && col_byte < sel_end;
				if is_selected && x < area.right() {
					let cell = &mut buf[(x, y)];
					cell.set_char(' ');
					cell.set_style(selected_style);
				}
				continue;
			}

			for ch in line.chars() {
				if x >= area.right() {
					break;
				}
				let ch_len = ch.len_utf8();
				let is_selected = has_sel && col_byte >= sel_start && col_byte < sel_end;
				let cell = &mut buf[(x, y)];
				cell.set_char(ch);
				cell.set_style(if is_selected { selected_style } else { normal_style });
				x = x.saturating_add(1);
				col_byte += ch_len;
			}
		}
	}

	fn render_input_cursor(&self, area: Rect, buf: &mut Buffer) {
		if area.width == 0 || area.height == 0 {
			return;
		}

		if self.cursor_visible {
			// Check if cursor revert animation is active
			if self.animation.cursor_revert_animation
				&& let Some(start_time) = self.animation.cursor_revert_start
			{
				let elapsed = start_time.elapsed().as_millis() as f32;
				let animation_duration = 300.0; // 300ms animation

				if elapsed < animation_duration {
					// Calculate interpolation progress (0.0 to 1.0)
					let progress = elapsed / animation_duration;
					// Use ease-out cubic for smooth deceleration
					let eased_progress = 1.0 - (1.0 - progress).powi(3);

					// Interpolate between old position and new position
					let from_pos = self.animation.cursor_revert_from_pos as f32;
					let to_pos = self.input.cursor_position as f32;
					let animated_pos = from_pos + (to_pos - from_pos) * eased_progress;

					// Render animated cursor with trail effect
					let position = animated_pos.max(0.0) as usize;
					let width = usize::from(area.width);
					let row = position / width;
					let column = position % width;
					if row >= usize::from(area.height) {
						return;
					}
					let cursor_x = area.x.saturating_add(column as u16);
					let cursor_y = area.y.saturating_add(row as u16);

					if cursor_x < area.right() && cursor_y < area.bottom() {
						let cell = &mut buf[(cursor_x, cursor_y)];
						let rainbow_color = self.animation.rainbow_cursor.current_color();

						// Pulsing effect during animation
						let pulse_char = if (elapsed as u32 / 50).is_multiple_of(2) { '◆' } else { '◇' };
						cell.set_char(pulse_char);
						cell.set_style(Style::default().fg(rainbow_color));
					}

					return;
				}
			}

			// Normal cursor: map byte offset → (line, col) on display content
			let display = self.input.display_content();
			if display.is_empty() {
				let cell = &mut buf[(area.x, area.y)];
				cell.set_char('▎');
				cell.set_style(Style::default().fg(self.animation.rainbow_cursor.current_color()));
				return;
			}
			// Approximate: use raw content line/col relative to vertical_scroll
			let before = &self.input.content[..self.input.cursor_position.min(self.input.content.len())];
			let line = before.chars().filter(|&c| c == '\n').count();
			let col = match before.rfind('\n') {
				Some(nl) => before[nl + 1..].chars().count(),
				None => before.chars().count(),
			};
			let view_line = line.saturating_sub(self.input.vertical_scroll);
			if view_line >= usize::from(area.height) {
				return;
			}
			let cursor_x = area.x.saturating_add((col as u16).min(area.width.saturating_sub(1)));
			let cursor_y = area.y.saturating_add(view_line as u16);

			if cursor_x < area.right() && cursor_y < area.bottom() {
				let cell = &mut buf[(cursor_x, cursor_y)];
				let existing_char = cell.symbol().chars().next().unwrap_or(' ');
				let rainbow_color = self.animation.rainbow_cursor.current_color();

				if existing_char == ' ' || self.input.content.is_empty() {
					cell.set_char('▎');
					cell.set_style(Style::default().fg(rainbow_color));
				} else {
					cell.set_style(Style::default().bg(rainbow_color).fg(self.theme.bg));
				}
			}
		}
	}

	/// Bottom bar (left → right):
	///   Ask · Remote · Big Pickle, OpenCode Zen   | **center** |   42 (0%) · $0.00 · +11, -42
	///
	/// Center is context-aware (OpenCode footer-shaped):
	/// permission chips · question chips · Goal timer · Plan toggles · profile actions · tips.
	///
	/// Returns (mode_area, model_area, token_area, runtime_area).
	pub fn render_bottom_controls(
		&mut self,
		area: Rect,
		buf: &mut Buffer,
		show_actions: bool,
	) -> (Rect, Rect, Rect, Rect) {
		use crate::bottom_center::build_center;

		let mode_text = self.agent_mode.label().to_string();
		let runtime_text = self.runtime_mode.label().to_string();
		let _reasoning_text = self.reasoning_effort.label().to_string();
		let (model_name, provider_name) = self.resolved_model_labels();
		self.provider.model_display_name = model_name.clone();
		self.provider.model_provider_name = provider_name.clone();

		let token_info = self.token_usage_label();
		let cost_info = self.cost_label();
		let add_n = self.diff_state.total_additions;
		let del_n = self.diff_state.total_deletions;
		let add_text = format!("+{add_n}");
		let del_text = format!("-{del_n}");
		let diff_width = (add_text.chars().count() + 2 + del_text.chars().count()) as u16;

		let model_part_w = model_name.chars().count() as u16;
		let provider_part_w = provider_name.chars().count() as u16;
		let model_cluster_w = model_part_w + 2 + provider_part_w;

		let mode_width = mode_text.chars().count() as u16;
		let runtime_width = runtime_text.chars().count() as u16;
		let token_width = token_info.chars().count() as u16;
		let cost_width = cost_info.chars().count() as u16;

		let total_tokens = self.total_tokens_estimate();
		let context_limit = self.context_limit();
		let token_ratio = if context_limit > 0 {
			(total_tokens as f32 / context_limit as f32 * 100.0) as u32
		} else {
			0
		};

		const SEP_W: u16 = 3;
		const RIGHT_SEP_W: u16 = 2; // Gap between diff and spinner
		const LEFT_PAD: u16 = 1;
		// Match visual gap beside the right sidebar so the spinner isn't flush
		const RIGHT_PAD: u16 = 1;
		const SPINNER_W: u16 = 1;

		let fixed_left = LEFT_PAD + mode_width + SEP_W + runtime_width + SEP_W + model_cluster_w;
		let fixed_right =
			token_width + SEP_W + cost_width + SEP_W + diff_width + RIGHT_SEP_W + SPINNER_W + RIGHT_PAD;
		let center_space = area.width.saturating_sub(fixed_left + fixed_right);
		let show_center = center_space >= 12;

		// Build center content (permission > question > goal timer > plan > profile actions > tip)
		let perm = self.permission_hub.pending();
		let question = self.question_hub.pending();
		let goal_timer = if self.agent_mode == crate::modes::AgentMode::Goal && self.goal.active {
			Some(self.goal.bar_timer_line())
		} else {
			None
		};
		let tip = self.current_tip();
		let center = build_center(
			self.agent_mode,
			self.goal.active,
			goal_timer,
			self.goal.paused,
			&self.plan_options,
			perm.as_ref(),
			question.as_ref(),
			tip,
			show_actions,
			&self.input.paste_blocks,
			&self.input.attachments,
		);

		let constraints = vec![
			Constraint::Length(LEFT_PAD),
			Constraint::Length(mode_width),
			Constraint::Length(SEP_W),
			Constraint::Length(runtime_width),
			Constraint::Length(SEP_W),
			Constraint::Length(model_cluster_w),
			Constraint::Min(if show_center { 8 } else { 0 }),
			Constraint::Length(token_width),
			Constraint::Length(SEP_W),
			Constraint::Length(cost_width),
			Constraint::Length(SEP_W),
			Constraint::Length(diff_width),
			Constraint::Length(RIGHT_SEP_W),
			Constraint::Length(SPINNER_W),
			Constraint::Length(RIGHT_PAD),
		];
		// bottom_chunks layout:
		//   0=pad  1=mode  2=sep  3=runtime  4=sep  5=model  6=center
		//   7=tokens  8=sep  9=cost  10=sep  11=diffs  12=sep  13=spinner  14=pad

		let bottom_chunks =
			Layout::default().direction(Direction::Horizontal).constraints(constraints).split(area);

		let sep =
			Span::styled("•", Style::default().fg(self.theme.muted_fg).add_modifier(Modifier::BOLD));

		let mode_color = self.agent_mode.color(&self.theme);
		Paragraph::new(Span::styled(
			&mode_text,
			Style::default().fg(mode_color).add_modifier(Modifier::BOLD),
		))
		.render(bottom_chunks[1], buf);
		Paragraph::new(sep.clone())
			.alignment(ratatui::layout::Alignment::Center)
			.render(bottom_chunks[2], buf);

		Paragraph::new(Span::styled(&runtime_text, Style::default().fg(self.theme.muted_fg)))
			.render(bottom_chunks[3], buf);
		Paragraph::new(sep.clone())
			.alignment(ratatui::layout::Alignment::Center)
			.render(bottom_chunks[4], buf);

		let model_line = Line::from(vec![
			Span::styled(&model_name, Style::default().fg(self.theme.muted_fg)),
			Span::styled(", ", Style::default().fg(self.theme.border)),
			Span::styled(
				&provider_name,
				Style::default().fg({
					match self.theme.muted_fg {
						Color::Rgb(r, g, b) => {
							Color::Rgb(r.saturating_sub(25), g.saturating_sub(25), b.saturating_sub(25))
						}
						c => c,
					}
				}),
			),
		]);
		// ── Center surface ──────────────────────────────────────────────
		Paragraph::new(model_line).render(bottom_chunks[5], buf);

		// ── Center surface ──────────────────────────────────────────────
		self.ui.center_chip_areas.clear();
		self.ui.center_bar_area = bottom_chunks[6];
		if show_center && bottom_chunks[6].width > 0 {
			self.render_center_bar(bottom_chunks[6], buf, &center, show_actions);
		}

		let token_color = if token_ratio > 80 {
			self.theme.danger()
		} else if token_ratio > 60 {
			self.theme.warning()
		} else {
			self.theme.fg
		};
		Paragraph::new(Span::styled(&token_info, Style::default().fg(token_color)))
			.render(bottom_chunks[7], buf);
		Paragraph::new(sep.clone())
			.alignment(ratatui::layout::Alignment::Center)
			.render(bottom_chunks[8], buf);

		Paragraph::new(Span::styled(&cost_info, Style::default().fg(self.theme.muted_fg)))
			.render(bottom_chunks[9], buf);
		Paragraph::new(sep.clone())
			.alignment(ratatui::layout::Alignment::Center)
			.render(bottom_chunks[10], buf);

		let diff_line = Line::from(vec![
			Span::styled(
				add_text,
				Style::default().fg(self.theme.success()).add_modifier(Modifier::BOLD),
			),
			Span::styled(", ", Style::default().fg(self.theme.muted_fg)),
			Span::styled(del_text, Style::default().fg(self.theme.danger()).add_modifier(Modifier::BOLD)),
		]);
		Paragraph::new(diff_line).render(bottom_chunks[11], buf);

		// Bottom bar spinner: braille frames, spinning rainbow when loading, static when idle
		{
			const SPINNER_FRAMES: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
			let (spin_char, spin_color) = if self.is_loading {
				let frame_idx = ((self.animation.rainbow_animation.elapsed() * 1000.0 / 80.0) as usize)
					% SPINNER_FRAMES.len();
				let c = self.animation.rainbow_animation.rgb_color_at(frame_idx);
				(SPINNER_FRAMES[frame_idx], Color::Rgb(c.r, c.g, c.b))
			} else {
				('⠿', self.theme.muted_fg)
			};
			Paragraph::new(Span::styled(
				spin_char.to_string(),
				Style::default().fg(spin_color).add_modifier(Modifier::BOLD),
			))
			.render(bottom_chunks[13], buf);
		}

		self.ui.token_button_area = bottom_chunks[7];
		self.ui.diff_button_area = bottom_chunks[11];

		(bottom_chunks[1], bottom_chunks[5], bottom_chunks[7], bottom_chunks[3])
	}

	/// Render OpenCode-style center chips / timer with hover highlight.
	/// Shows ↑ / ↓ scroll chips only on the message screen (show_scroll=true).
	fn render_center_bar(
		&mut self,
		area: Rect,
		buf: &mut Buffer,
		content: &crate::bottom_center::CenterContent,
		show_scroll: bool,
	) {
		use crate::bottom_center::{CenterAction, CenterChip, CenterContent};

		// Pure text (rotating tip) — render centered, clipped to width.
		if let CenterContent::Text { text, .. } = content {
			let display: String = text.chars().take(area.width.max(1) as usize).collect();
			ratatui::widgets::Paragraph::new(ratatui::text::Span::styled(
				display,
				ratatui::style::Style::default().fg(self.theme.muted_fg),
			))
			.alignment(ratatui::layout::Alignment::Center)
			.render(area, buf);
			return;
		}

		// Chips content: merge scroll nav (message screen only) with profile/context chips.
		let scroll_chips =
			if show_scroll { crate::bottom_center::scroll_nav_chips() } else { Vec::new() };
		let (label, chips): (Option<String>, Vec<CenterChip>) = match content {
			CenterContent::Text { .. } => unreachable!(),
			CenterContent::Chips { label, chips } => {
				let mut all = scroll_chips;
				all.extend(chips.iter().cloned());
				(label.clone(), all)
			}
		};

		// Layout: [↑] [↓] · [label?] · [chip] …
		let mut total_w = 0u16;
		// parts: (Some(chip_idx) | None for separators/labels, text, width)
		let mut parts: Vec<(Option<usize>, String, u16)> = Vec::new();

		for (i, chip) in chips.iter().enumerate() {
			if i > 0 {
				// No bullet between the two scroll arrows; bullets before later chips
				let sep = if i == 1 { " " } else { "•" };
				let sep_w = if i == 1 { 1 } else { 2 };
				parts.push((None, sep.into(), sep_w));
				total_w = total_w.saturating_add(sep_w);
			}
			// After ↓, inject the optional label before other chips
			if i == 2
				&& let Some(ref lab) = label
			{
				let s: String = lab.chars().take(28).collect();
				if !s.is_empty() {
					let w = s.chars().count() as u16;
					parts.push((None, s, w));
					total_w = total_w.saturating_add(w).saturating_add(1);
					parts.push((None, "•".into(), 2));
					total_w = total_w.saturating_add(2);
				}
			}
			let text = chip.label.clone();
			let w = text.chars().count() as u16 + 1;
			parts.push((Some(i), text, w));
			total_w = total_w.saturating_add(w);
		}
		// If only scroll chips (no extra chips), still show label after them
		if chips.len() <= 2
			&& let Some(ref lab) = label
		{
			let s: String = lab.chars().take(36).collect();
			if !s.is_empty() {
				parts.push((None, " ".into(), 1));
				total_w = total_w.saturating_add(1);
				let w = s.chars().count() as u16;
				parts.push((None, s, w));
				total_w = total_w.saturating_add(w);
			}
		}

		// Clip if needed (drop from the end — keep ↑↓)
		while total_w > area.width && parts.len() > 3 {
			if let Some((_, _, w)) = parts.pop() {
				total_w = total_w.saturating_sub(w);
			}
		}

		let mut x = area.x + area.width.saturating_sub(total_w) / 2;
		for (idx_opt, text, w) in parts {
			let chip_w = w.saturating_sub(1).max(1);
			let rect =
				Rect { x, y: area.y, width: chip_w.min(area.right().saturating_sub(x)), height: 1 };
			if rect.width == 0 {
				break;
			}
			let hovered = idx_opt.is_some_and(|i| self.ui.center_chip_hover == Some(i));
			let chip_action = idx_opt.and_then(|i| chips.get(i).map(|c| &c.action));
			let is_scroll = chip_action
				.is_some_and(|a| matches!(a, CenterAction::ScrollChatTop | CenterAction::ScrollChatBottom));
			let is_perm = chip_action.is_some_and(|a| {
				matches!(a, CenterAction::PermOnce | CenterAction::PermAlways | CenterAction::PermDeny)
			});
			let is_plan_go = chip_action.is_some_and(|a| matches!(a, CenterAction::PlanApproveWrite));
			let is_deny = chip_action.is_some_and(|a| matches!(a, CenterAction::PermDeny));
			let (fg, bg) = if hovered {
				if is_deny {
					(self.theme.bg, self.theme.danger())
				} else {
					(self.theme.bg, self.theme.accent)
				}
			} else if is_scroll {
				(self.theme.accent, Color::Reset)
			} else if is_plan_go {
				(self.theme.success(), Color::Reset)
			} else if is_deny {
				(self.theme.danger(), Color::Reset)
			} else if is_perm {
				(self.theme.primary, Color::Reset)
			} else if self.agent_mode == crate::modes::AgentMode::Goal {
				(self.theme.warning(), Color::Reset)
			} else if self.agent_mode == crate::modes::AgentMode::Plan {
				(self.theme.accent, Color::Reset)
			} else {
				(self.theme.muted_fg, Color::Reset)
			};
			let style = if hovered {
				Style::default().fg(fg).bg(bg).add_modifier(Modifier::BOLD)
			} else {
				Style::default().fg(fg).add_modifier(Modifier::BOLD)
			};
			if let Some(i) = idx_opt {
				if let Some(chip) = chips.get(i) {
					self.ui.center_chip_areas.push((chip.action.clone(), rect));
				}
				Paragraph::new(Span::styled(text, style)).render(rect, buf);
			} else {
				Paragraph::new(Span::styled(
					text,
					Style::default().fg(self.theme.fg).add_modifier(Modifier::BOLD),
				))
				.render(rect, buf);
			}
			x = x.saturating_add(w);
		}
		let _ = chips; // used above
	}

	pub fn render_command_dialog(&self, area: Rect, buf: &mut Buffer) {
		use crate::state::CommandDialog;

		if self.ui.dialog == CommandDialog::None {
			return;
		}

		let title = match self.ui.dialog {
			CommandDialog::Sessions => " Sessions  ·  Enter switch  ·  n new  ·  Esc ",
			CommandDialog::Rename => " Rename session ",
			CommandDialog::UserName => " Your display name ",
			CommandDialog::Timeline => " Timeline  ·  Enter jump ",
			CommandDialog::Fork => " Fork from message ",
			CommandDialog::Export => " Export transcript ",
			CommandDialog::Note => " Edit session note ",
			CommandDialog::Move => " Move session project ",
			CommandDialog::Help => " Help — slash commands ",
			CommandDialog::Status => " Status ",
			CommandDialog::Debug => " Debug ",
			CommandDialog::Themes => " Themes ",
			CommandDialog::Skills => " Skills ",
			CommandDialog::Connect => " Connect provider ",
			CommandDialog::Mcps => " MCP servers ",
			CommandDialog::Workspaces => " Workspaces ",
			CommandDialog::None => return,
		};

		let is_text = matches!(
			self.ui.dialog,
			CommandDialog::Rename
				| CommandDialog::UserName
				| CommandDialog::Export
				| CommandDialog::Note
				| CommandDialog::Move
		);

		let items = self.dialog_list_items();
		let body_lines = if is_text {
			3u16 + if self.ui.dialog == CommandDialog::Export { 2 } else { 0 }
		} else {
			(items.len() as u16 + 2).min(area.height.saturating_sub(4)).max(3)
		};

		let width = area.width.clamp(28, 72);
		let height = body_lines.min(area.height.saturating_sub(2)).max(5);
		let popup_area = Rect {
			x: area.x + area.width.saturating_sub(width) / 2,
			y: area.y + area.height.saturating_sub(height) / 2,
			width,
			height,
		};

		for y in popup_area.top()..popup_area.bottom() {
			for x in popup_area.left()..popup_area.right() {
				buf[(x, y)].reset();
				buf[(x, y)].set_bg(self.theme.bg);
			}
		}

		let block = Block::default()
			.borders(Borders::ALL)
			.border_type(ratatui::widgets::BorderType::Rounded)
			.border_style(Style::default().fg(self.theme.accent))
			.title(Span::styled(
				title,
				Style::default().fg(self.theme.accent).add_modifier(Modifier::BOLD),
			));
		let inner = block.inner(popup_area);
		block.render(popup_area, buf);

		if is_text {
			let label = match self.ui.dialog {
				CommandDialog::Rename => "Session:",
				CommandDialog::UserName => "You are:",
				CommandDialog::Export => "File:",
				CommandDialog::Note => "Note:",
				CommandDialog::Move => "Directory:",
				_ => "Input:",
			};
			Paragraph::new(Span::styled(label, Style::default().fg(self.theme.border)))
				.render(Rect { x: inner.x, y: inner.y, width: inner.width, height: 1 }, buf);
			let field = format!("{}_", self.ui.dialog_input);
			Paragraph::new(Span::styled(field, Style::default().fg(self.theme.fg)))
				.render(Rect { x: inner.x, y: inner.y + 1, width: inner.width, height: 1 }, buf);
			if self.ui.dialog == CommandDialog::Export {
				let opts = format!(
					"thinking={}  tools={}  (Tab cycles)",
					self.ui.export_include_thinking, self.ui.export_include_tools
				);
				Paragraph::new(Span::styled(opts, Style::default().fg(self.theme.border)))
					.render(Rect { x: inner.x, y: inner.y + 3, width: inner.width, height: 1 }, buf);
			}
			return;
		}

		for (i, (label, detail)) in items.iter().enumerate().take(inner.height as usize) {
			let y = inner.y + i as u16;
			let selected = i == self.ui.dialog_cursor;
			let style = if selected {
				Style::default().fg(self.theme.bg).bg(self.theme.accent).add_modifier(Modifier::BOLD)
			} else {
				Style::default().fg(self.theme.fg)
			};
			let text = if detail.is_empty() { label.clone() } else { format!("{label}  —  {detail}") };
			let display: String = text.chars().take(inner.width as usize).collect();
			Paragraph::new(Span::styled(display, style))
				.render(Rect { x: inner.x, y, width: inner.width, height: 1 }, buf);
		}
	}

	pub fn render_bottom_popup(&self, area: Rect, buf: &mut Buffer) {
		use crate::state::BottomPopup;

		if self.ui.bottom_popup == BottomPopup::None {
			return;
		}

		#[allow(dead_code)]
		fn on_off(v: bool) -> &'static str {
			if v { "on" } else { "off" }
		}

		// PlanOptions renders the wizard overlay instead of item list
		if self.ui.bottom_popup == BottomPopup::PlanOptions {
			crate::plan_wizard::render_plan_wizard(area, buf, &self.plan_wizard);
			return;
		}

		let (title, items): (String, Vec<(String, String)>) = match self.ui.bottom_popup {
			BottomPopup::AgentMode => (
				"Agent mode".into(),
				crate::modes::AgentMode::ALL
					.iter()
					.map(|m| (m.label().to_string(), String::new()))
					.collect(),
			),
			BottomPopup::Runtime => (
				"Runtime".into(),
				vec![
					("Local".into(), "on-device models".into()),
					("Remote".into(), "cloud providers".into()),
				],
			),
			BottomPopup::Models => {
				if self.agent_mode == crate::modes::AgentMode::Codex {
					(
						"Models · Codex mode".into(),
						vec![("Codex — managed by app-server".into(), String::new())],
					)
				} else {
					let flow_n = self
						.provider
						.model_catalog
						.iter()
						.filter(|m| m.is_local && m.is_selectable_model() && m.available)
						.count();
					let title = format!("Models · {} · Flow {flow_n} · key 0", self.runtime_mode.label());
					(
						title,
						self
							.provider
							.model_catalog
							.iter()
							.map(|m| {
								if m.is_section() {
									(m.display_name.clone(), String::new())
								} else if m.is_action() {
									(m.display_name.clone(), m.provider.clone())
								} else if m.provider.is_empty() {
									(m.display_name.clone(), m.status_label().to_string())
								} else {
										(format!("{} · {}", m.display_name, m.provider_badge()), m.status_label().to_string())
								}
							})
							.collect(),
					)
				}
			}
			BottomPopup::Channels => {
				let mut items: Vec<(String, String)> = crate::channel_actions::channels_menu_action_rows()
					.iter()
					.map(|(a, b)| ((*a).to_string(), (*b).to_string()))
					.collect();
				items.extend(self.provider.channels.iter().map(|c| {
					(
						format!("{} {}", c.status_glyph(), c.name),
						format!("{} · {}", c.status_label(), c.description),
					)
				}));
				(
					format!(
						"Channels · dx-agent · key 1 · {}",
						crate::channels::connection_summary().chars().take(40).collect::<String>()
					),
					items,
				)
			}
			BottomPopup::Connect => {
				// Not-connected first for easier onboarding
				let mut providers: Vec<_> = self.provider.models_catalog.providers.iter().collect();
				providers.sort_by(|a, b| {
					let ac = self.provider.provider_store.providers.iter().any(|c| c.id == a.id && c.enabled);
					let bc = self.provider.provider_store.providers.iter().any(|c| c.id == b.id && c.enabled);
					ac.cmp(&bc).then_with(|| a.name.cmp(&b.name))
				});
				(
					format!(
						"Providers · {} providers · /providers",
						self.provider.models_catalog.provider_count()
					),
					providers
						.iter()
						.map(|p| {
							let connected =
								self.provider.provider_store.providers.iter().any(|c| c.id == p.id && c.enabled);
							let env = p.env.first().map(|e| format!(" · {e}")).unwrap_or_default();
							(
								format!("{} {}", if connected { "●" } else { "○" }, p.name),
								format!("{} models{env}", p.models.len()),
							)
						})
						.collect(),
				)
			}
			BottomPopup::PlanOptions => unreachable!("PlanOptions handled above"),
			BottomPopup::ShareChannel => (
				"Share session".into(),
				crate::channel_actions::sendable_channels(&self.provider.channels)
					.iter()
					.map(|c| (format!("{} {}", c.status_glyph(), c.name), c.type_key.clone()))
					.collect(),
			),
			BottomPopup::PastePreview => {
				let mut items: Vec<(String, String)> = Vec::new();
				for block in &self.input.paste_blocks {
					let short =
						block.content.lines().next().unwrap_or("").chars().take(50).collect::<String>();
					let detail = if short.len() >= 50 { format!("{short}…") } else { short };
					items.push((format!("📋 [{} lines · {} chars]", block.lines, block.chars), detail));
				}
				for att in &self.input.attachments {
					let sym = match att.kind {
						crate::input::AttachmentKind::File => "📄",
						crate::input::AttachmentKind::Folder => "📁",
						crate::input::AttachmentKind::Image => "🖼",
					};
					let path_str = att.path.display().to_string();
					items.push((format!("{sym} {}", att.label()), path_str));
				}
				(format!("Paste preview · {} items", items.len()), items)
			}
			BottomPopup::None => return,
		};

		// Larger popup for model/channel catalogs
		let max_h = area.height.saturating_sub(3).max(5);
		let height = (items.len() as u16 + 2).min(max_h).max(5);
		let width = area.width.clamp(28, 78);
		let popup_area =
			Rect { x: area.x + 1, y: area.bottom().saturating_sub(height + 1), width, height };

		// Clear + border
		for y in popup_area.top()..popup_area.bottom() {
			for x in popup_area.left()..popup_area.right() {
				buf[(x, y)].reset();
				buf[(x, y)].set_bg(self.theme.bg);
			}
		}

		let block = Block::default()
			.borders(Borders::ALL)
			.border_type(ratatui::widgets::BorderType::Rounded)
			.border_style(Style::default().fg(self.theme.accent))
			.title(Span::styled(
				format!(" {title} "),
				Style::default().fg(self.theme.accent).add_modifier(Modifier::BOLD),
			));
		let inner = block.inner(popup_area);
		block.render(popup_area, buf);

		// Scroll window so cursor stays visible
		let visible = inner.height as usize;
		let scroll =
			if self.ui.popup_cursor >= visible { self.ui.popup_cursor + 1 - visible } else { 0 };

		for row in 0..visible {
			let i = scroll + row;
			if i >= items.len() {
				break;
			}
			let (label, detail) = &items[i];
			let y = inner.y + row as u16;
			let selected = i == self.ui.popup_cursor;
			let is_section = label.starts_with("──");
			let style = if selected && !is_section {
				Style::default().fg(self.theme.bg).bg(self.theme.accent).add_modifier(Modifier::BOLD)
			} else if is_section {
				Style::default().fg(self.theme.muted_fg).add_modifier(Modifier::BOLD)
			} else {
				Style::default().fg(self.theme.fg)
			};
			let text =
				if detail.is_empty() || is_section { label.clone() } else { format!("{label}  {detail}") };
			let max = inner.width as usize;
			let display: String = text.chars().take(max).collect();
			Paragraph::new(Span::styled(display, style))
				.render(Rect { x: inner.x, y, width: inner.width, height: 1 }, buf);
		}
	}

	pub fn render_sidebar(&mut self, area: Rect, buf: &mut Buffer) {
		// Sidebar plate follows the active theme (card surface when available).
		let bg = if self.theme.card != self.theme.bg { self.theme.card } else { self.theme.bg };
		let fg = self.theme.fg;
		// Soft mute (readable) — theme.border is near-black and looks "too muted".
		let muted = self.theme.muted_fg;
		let border = self.theme.border;

		self.ui.sidebar_panel_area = area;

		// Clear sidebar background
		for y in area.top()..area.bottom() {
			for x in area.left()..area.right() {
				buf[(x, y)].reset();
				buf[(x, y)].set_bg(bg);
			}
		}

		// Left edge border for the right sidebar panel.
		const BORDER_W: u16 = 1;
		for y in area.top()..area.bottom() {
			let cell = &mut buf[(area.x, y)];
			cell.set_char('│');
			cell.set_fg(border);
			cell.set_bg(bg);
		}

		// Content sits inside the left border.
		let inner = Rect {
			x: area.x.saturating_add(BORDER_W),
			y: area.y,
			width: area.width.saturating_sub(BORDER_W),
			height: area.height,
		};

		// Reserve dedicated footer rows so path + version never get eaten by sections.
		// Extra bottom margin keeps footer clear of the terminal edge.
		// Title block: left-padded multi-line name + session id row underneath.
		let show_generating = !self.session.title_from_ai
			&& crate::session_meta::should_show_generating_title(
				&self.session.session_name,
				self.session.title_from_ai,
				self.is_loading,
			);

		const TITLE_LEFT_PAD: u16 = 1;
		const SESSION_ID_ROWS: u16 = 1;
		let name_width = inner.width.saturating_sub(TITLE_LEFT_PAD).max(1);
		let name_rows = if show_generating {
			// Keep three shimmer lines so the header height matches a real long title.
			3
		} else {
			// Always reserve ≥3 rows so long chat names look intentionally big.
			(self.session.session_name.chars().count() as u16).div_ceil(name_width).clamp(3, 8)
		};
		let title_lines = name_rows.saturating_add(SESSION_ID_ROWS);

		let chunks = Layout::default()
			.direction(Direction::Vertical)
			.constraints([
				Constraint::Length(1),           // top padding
				Constraint::Length(title_lines), // name + session id
				Constraint::Length(1),           // one line gap
				Constraint::Min(3),              // sections (scrollable accordion content)
				Constraint::Length(1),           // path
				Constraint::Length(1),           // version
				Constraint::Length(1),           // bottom margin
			])
			.split(inner);

		// Title — primary color; every wrapped line shares the same left inset.
		let title_area = chunks[1];
		let name_area = Rect {
			x: title_area.x.saturating_add(TITLE_LEFT_PAD),
			y: title_area.y,
			width: title_area.width.saturating_sub(TITLE_LEFT_PAD),
			height: title_area.height.saturating_sub(SESSION_ID_ROWS).max(1),
		};
		let id_area = Rect {
			x: title_area.x.saturating_add(TITLE_LEFT_PAD),
			y: title_area.bottom().saturating_sub(SESSION_ID_ROWS),
			width: title_area.width.saturating_sub(TITLE_LEFT_PAD),
			height: SESSION_ID_ROWS,
		};

		if show_generating {
			let label = "Generating...";
			let mut x = name_area.x;
			let y = name_area.y;
			let len = label.chars().count().max(1);
			for (i, ch) in label.chars().enumerate() {
				if x >= name_area.right() {
					break;
				}
				let pos = i as f32 / len as f32;
				let color = self.animation.shimmer.shimmer_color_at(pos);
				let cell = &mut buf[(x, y)];
				cell.set_char(ch);
				cell.set_fg(color);
				cell.set_style(
					ratatui::style::Style::default().fg(color).add_modifier(ratatui::style::Modifier::BOLD),
				);
				x = x.saturating_add(1);
			}
		} else {
			// Inset rect so wrap continuations keep the same left margin as line 1.
			ratatui::widgets::Paragraph::new(ratatui::text::Text::from(ratatui::text::Span::styled(
				self.session.session_name.clone(),
				ratatui::style::Style::default()
					.fg(self.theme.primary)
					.add_modifier(ratatui::style::Modifier::BOLD),
			)))
			.wrap(ratatui::widgets::Wrap { trim: true })
			.render(name_area, buf);
		}

		// Session id under the chat name (short, muted).
		let short_id = crate::session_store::short_session_id(&self.session.chat_id);
		if id_area.width > 0 && id_area.height > 0 && !short_id.is_empty() {
			let id_label = format!("#{short_id}");
			ratatui::widgets::Paragraph::new(ratatui::text::Span::styled(
				id_label,
				ratatui::style::Style::default().fg(muted).add_modifier(ratatui::style::Modifier::DIM),
			))
			.render(id_area, buf);
		}

		// Sections viewport (scrollable)
		let sections_area = chunks[3];
		self.ui.sidebar_area = sections_area;

		// Tasks · Subagents · LSPs · Plugins · MCPs · Prompts · Notes
		self.sidebar.refresh_if_stale(std::time::Duration::from_secs(45));
		self.sidebar.sync_subagents(&self.messages);
		let sections = self.sidebar.section_lines();

		// Dynamic height: header + body lines when open
		let section_heights: Vec<u16> = sections
			.iter()
			.enumerate()
			.map(
				|(i, (_, body))| {
					if self.ui.accordion_open[i] { 1 + body.len().max(1) as u16 } else { 1 }
				},
			)
			.collect();
		let content_height: u16 = section_heights.iter().sum();
		let viewport_h = sections_area.height;
		let max_scroll = content_height.saturating_sub(viewport_h);
		self.ui.sidebar_scroll = self.ui.sidebar_scroll.min(max_scroll);

		// Reset header hit areas; only visible headers get real rects.
		self.ui.sidebar_areas = [Rect::default(); crate::sidebar_data::SIDEBAR_SECTION_COUNT];
		// Task row hit targets: (task_index, rect) for click-to-cycle/remove
		self.ui.sidebar_task_areas.clear();
		self.ui.sidebar_prompt_areas.clear();
		self.ui.sidebar_note_area = None;

		let mut content_y: u16 = 0;
		for (i, ((name, body), section_h)) in sections.iter().zip(section_heights.iter()).enumerate() {
			let is_open = self.ui.accordion_open[i];
			let section_top = content_y;
			content_y = content_y.saturating_add(*section_h);

			if content_y <= self.ui.sidebar_scroll {
				continue;
			}
			if section_top >= self.ui.sidebar_scroll.saturating_add(viewport_h) {
				break;
			}

			let header_style =
				ratatui::style::Style::default().fg(muted).add_modifier(ratatui::style::Modifier::BOLD);
			let body_style = ratatui::style::Style::default().fg(fg);
			let empty_style =
				ratatui::style::Style::default().fg(muted).add_modifier(ratatui::style::Modifier::ITALIC);
			let done_style = ratatui::style::Style::default()
				.fg(self.theme.success())
				.add_modifier(ratatui::style::Modifier::DIM);
			let active_style = ratatui::style::Style::default()
				.fg(self.theme.primary)
				.add_modifier(ratatui::style::Modifier::BOLD);
			let cancelled_style = ratatui::style::Style::default()
				.fg(self.theme.muted_fg)
				.add_modifier(ratatui::style::Modifier::CROSSED_OUT);

			for row in 0..*section_h {
				let abs_row = section_top + row;
				if abs_row < self.ui.sidebar_scroll {
					continue;
				}
				let screen_row = abs_row - self.ui.sidebar_scroll;
				if screen_row >= viewport_h {
					break;
				}
				let row_area = Rect {
					x: sections_area.x + 1,
					y: sections_area.y + screen_row,
					width: sections_area.width.saturating_sub(crate::components::SCROLLBAR_TRACK_WIDTH + 1),
					height: 1,
				};

				if row == 0 {
					self.ui.sidebar_areas[i] = row_area;
					let chev = if is_open { "▼" } else { "▶" };
					// Tasks header shows count when non-empty
					let label = if i == 0 {
						let n = self.sidebar.snapshot().tasks.len();
						if n == 0 { format!("{chev} {name}") } else { format!("{chev} {name} · {n}") }
					} else {
						format!("{chev} {name}")
					};
					ratatui::widgets::Paragraph::new(ratatui::text::Span::styled(label, header_style))
						.render(row_area, buf);
				} else if is_open {
					let body_idx = (row - 1) as usize;
					let line = body.get(body_idx).map(|s| format!("  {s}")).unwrap_or_default();
					let is_empty = line.contains("No Tasks Yet")
						|| line.contains("No Prompts Yet")
						|| line.contains("No Notes Yet")
						|| line.contains("none configured")
						|| line.trim() == "—"
						|| line.contains("—");
					let style = if is_empty {
						empty_style
					} else if i == 0 && line.contains("[done]") {
						done_style
					} else if i == 0 && line.contains("[active]") {
						active_style
					} else if i == 0 && line.contains("[cancelled]") {
						cancelled_style
					} else {
						body_style
					};
					// Record task row hits (section 0 = Tasks, skip empty placeholder)
					if i == 0 && !is_empty {
						self.ui.sidebar_task_areas.push((body_idx, row_area));
					}
					// Record prompt row hits (section 1 = Prompts, skip placeholder)
					if i == 1 && !is_empty {
						self.ui.sidebar_prompt_areas.push((body_idx, row_area));
					}
					// Record note area (section 2 = Notes, first body row)
					if i == 2 && body_idx == 0 {
						self.ui.sidebar_note_area = Some(Rect {
							x: row_area.x,
							y: row_area.y,
							width: row_area.width,
							height: section_h.saturating_sub(1).min(sections_area.bottom() - row_area.y),
						});
					}

					if i == 2 {
						// Notes section: bordered rect
						let _n_rows = section_h.saturating_sub(1) as usize;
						let last_row = (*section_h as usize).saturating_sub(1);
						let border = Style::default().fg(self.theme.border);
						let body_w = row_area.width.saturating_sub(2) as usize;
						if row == 1 {
							let sep = "─".repeat(body_w);
							ratatui::widgets::Paragraph::new(ratatui::text::Line::from(vec![
								Span::styled("┌", border),
								Span::styled(sep, border),
								Span::styled("┐", border),
							]))
							.render(row_area, buf);
						} else if row as usize == last_row {
							let sep = "─".repeat(body_w);
							ratatui::widgets::Paragraph::new(ratatui::text::Line::from(vec![
								Span::styled("└", border),
								Span::styled(sep, border),
								Span::styled("┘", border),
							]))
							.render(row_area, buf);
						} else {
							let display =
								crate::session_meta::ellipsize_one_line(line.trim_end(), body_w.saturating_sub(1));
							let pad = body_w.saturating_sub(display.width()).min(body_w);
							ratatui::widgets::Paragraph::new(ratatui::text::Line::from(vec![
								Span::styled("│", border),
								Span::styled(display, style),
								Span::styled(" ".repeat(pad) + "│", border),
							]))
							.render(row_area, buf);
						}
					} else {
						// Single line — ellipsize by display width
						let display =
							crate::session_meta::ellipsize_one_line(line.trim_end(), row_area.width as usize);
						ratatui::widgets::Paragraph::new(ratatui::text::Span::styled(display, style))
							.render(row_area, buf);
					}
				}
			}
		}

		// Same custom full-range scrollbar as the message list (1-col, flush bottom)
		if content_height > viewport_h && viewport_h > 0 {
			crate::components::render_scrollbar_track_hover(
				sections_area,
				buf,
				content_height as usize,
				self.ui.sidebar_scroll as usize,
				self.ui.sidebar_scrollbar_hovered,
				crate::components::SCROLLBAR_TRACK_WIDTH,
			);
		}

		// Footer: cwd path + version (dedicated rows, always visible)
		let cwd = std::env::current_dir()
			.ok()
			.and_then(|p| p.to_str().map(|s| s.to_string()))
			.unwrap_or_default();
		let path_max = chunks[4].width.saturating_sub(1) as usize;
		let path_display = if path_max == 0 {
			String::new()
		} else {
			format!(" {}", truncate_start(&cwd, path_max.saturating_sub(1)))
		};
		ratatui::widgets::Paragraph::new(ratatui::text::Span::styled(
			path_display,
			ratatui::style::Style::default().fg(fg),
		))
		.render(chunks[4], buf);

		ratatui::widgets::Paragraph::new(ratatui::text::Span::styled(
			" Dx v1.0.0",
			ratatui::style::Style::default().fg(muted),
		))
		.render(chunks[5], buf);
		// chunks[6] is bottom margin (intentionally empty)
	}

	pub fn render_session_screen(&self, area: Rect, buf: &mut Buffer) {
		// Soft clear — no harsh black plate.
		for y in area.top()..area.bottom() {
			for x in area.left()..area.right() {
				buf[(x, y)].reset();
			}
		}

		let elapsed = self.session.session_start_time.elapsed();
		let hours = elapsed.as_secs() / 3600;
		let mins = (elapsed.as_secs() % 3600) / 60;
		let secs = elapsed.as_secs() % 60;
		let (model_name, _provider) = self.resolved_model_labels();
		let cont = self.continue_command_line();
		let short_id = crate::session_store::short_session_id(&self.session.chat_id);

		// Soft mute labels (readable at the top of the cleared screen).
		let muted = Style::default().fg(self.theme.muted_fg);
		let fg = Style::default().fg(self.theme.fg);
		let accent = Style::default().fg(self.theme.accent).add_modifier(Modifier::BOLD);
		let cmd_style = Style::default().fg(self.theme.success()).add_modifier(Modifier::BOLD);

		let lines = vec![
			Line::from(Span::styled("  Session saved", accent)),
			Line::from(""),
			Line::from(vec![
				Span::styled("  Name      ", muted),
				Span::styled(self.session.session_name.clone(), fg),
			]),
			Line::from(vec![Span::styled("  ID        ", muted), Span::styled(short_id, fg)]),
			Line::from(vec![
				Span::styled("  Messages  ", muted),
				Span::styled(self.messages.len().to_string(), fg),
			]),
			Line::from(vec![Span::styled("  Model     ", muted), Span::styled(model_name, fg)]),
			Line::from(vec![
				Span::styled("  Duration  ", muted),
				Span::styled(format!("{hours:02}:{mins:02}:{secs:02}"), fg),
			]),
			Line::from(""),
			Line::from(Span::styled("  Resume this session:", muted)),
			Line::from(vec![Span::raw("    "), Span::styled(cont, cmd_style)]),
			Line::from(""),
			Line::from(Span::styled("  [Enter]  stay in session", muted)),
			Line::from(Span::styled("  [q] / [Ctrl+C]  quit quietly", muted)),
		];

		// Always pin summary to the top of the viewport.
		let top = Rect { x: area.x, y: area.y, width: area.width, height: area.height };
		Paragraph::new(Text::from(lines)).render(top, buf);
	}

	pub fn render_left_minimap(&mut self, area: Rect, buf: &mut Buffer) {
		// Transparent rail: only clear glyphs, keep screen bg (no solid fill)
		for y in area.top()..area.bottom() {
			for x in area.left()..area.right() {
				let cell = &mut buf[(x, y)];
				cell.set_char(' ');
				// keep existing bg from root clear
			}
		}

		self.ui.minimap_top_indicator = Rect::default();
		self.ui.minimap_bottom_indicator = Rect::default();

		let user_indices = self.user_message_indices();
		let total = user_indices.len();
		if total == 0 || area.height == 0 || area.width == 0 {
			self.ui.minimap_area = area;
			self.ui.minimap_viewport = 0;
			self.ui.minimap_scroll = 0;
			return;
		}

		// Prefer explicit active (clicked) message; fall back to last user message.
		let selected_idx = self
			.ui
			.active_message_index
			.and_then(|active| user_indices.iter().position(|&i| i == active))
			.unwrap_or_else(|| total.saturating_sub(1));

		// When content overflows the rail, reserve 1 row top + bottom for ▴N / ▾N
		// so infinite lists stay navigable with a clear remainder count.
		// Need at least 3 rows to host top ind + marker + bottom ind.
		let overflows = (total as u16) > area.height && area.height >= 3;
		let content_h = if overflows {
			area.height.saturating_sub(2).max(1)
		} else {
			(total as u16).min(area.height)
		};

		self.ui.minimap_viewport = content_h;
		let max_scroll = (total as u16).saturating_sub(content_h);
		// Free user scroll — no per-frame re-centering (that capped scroll to ~half).
		self.ui.minimap_scroll = self.ui.minimap_scroll.min(max_scroll);

		let scroll = self.ui.minimap_scroll as usize;
		let view_h = content_h as usize;
		let above = scroll;
		let below = total.saturating_sub(scroll + view_h);

		let fmt_count = |n: usize| -> String { if n > 99 { "99+".to_string() } else { n.to_string() } };

		let (content_area, top_ind, bot_ind) = if overflows {
			// [top indicator][markers...][bottom indicator]
			let top = Rect { x: area.x, y: area.y, width: area.width, height: 1 };
			let mid = Rect { x: area.x, y: area.y + 1, width: area.width, height: content_h };
			let bot = Rect { x: area.x, y: area.y + 1 + content_h, width: area.width, height: 1 };
			(mid, Some(top), Some(bot))
		} else {
			// Center when everything fits
			let pad = area.height.saturating_sub(content_h) / 2;
			let mid = Rect { x: area.x, y: area.y + pad, width: area.width, height: content_h };
			(mid, None, None)
		};

		let muted = self.theme.border;
		let accent = self.theme.accent;
		let ind_style = |active: bool| {
			if active {
				ratatui::style::Style::default().fg(accent).add_modifier(ratatui::style::Modifier::BOLD)
			} else {
				ratatui::style::Style::default().fg(muted)
			}
		};

		// Left-align glyphs (shorter markers share the same left edge).
		let mut paint_left = |area: Rect, text: &str, style: ratatui::style::Style| {
			if area.width == 0 || area.height == 0 || text.is_empty() {
				return;
			}
			for (i, ch) in text.chars().enumerate() {
				let x = area.x + i as u16;
				if x >= area.x + area.width {
					break;
				}
				let cell = &mut buf[(x, area.y)];
				cell.set_char(ch);
				cell.set_style(style);
			}
		};

		if let Some(top) = top_ind {
			self.ui.minimap_top_indicator = top;
			let label = if above > 0 { format!("▴{}", fmt_count(above)) } else { "▴".to_string() };
			paint_left(top, &label, ind_style(above > 0));
		}

		if let Some(bot) = bot_ind {
			self.ui.minimap_bottom_indicator = bot;
			let label = if below > 0 { format!("▾{}", fmt_count(below)) } else { "▾".to_string() };
			paint_left(bot, &label, ind_style(below > 0));
		}

		// Draw only visible markers — virtualized for large lists; LEFT-aligned
		for row in 0..view_h {
			let i = scroll + row;
			if i >= total {
				break;
			}
			let real_index = user_indices[i];
			let is_active = Some(real_index) == self.ui.active_message_index
				|| (self.ui.active_message_index.is_none() && i == selected_idx);
			let is_hovered = Some(real_index) == self.ui.hovered_message_index;

			let color = if is_active {
				self.theme.accent
			} else if is_hovered {
				self.theme.fg
			} else {
				self.theme.border
			};

			// Taper length but keep left edge fixed (no right-align / center)
			let dist_edge = row.min(view_h.saturating_sub(1).saturating_sub(row));
			let symbol = if dist_edge == 0 {
				"━"
			} else if dist_edge == 1 && view_h > 3 {
				"━━"
			} else {
				"━━━"
			};

			let style = if is_active {
				ratatui::style::Style::default().fg(color).add_modifier(ratatui::style::Modifier::BOLD)
			} else {
				ratatui::style::Style::default().fg(color)
			};

			let row_area = Rect {
				x: content_area.x,
				y: content_area.y + row as u16,
				width: content_area.width,
				height: 1,
			};
			paint_left(row_area, symbol, style);
		}

		// Hit-test area matches the drawn markers exactly
		self.ui.minimap_area = content_area;
	}

	fn render_minimap_hover_card(&self, area: Rect, buf: &mut Buffer) {
		let Some(hovered_at) = self.ui.hovered_message_since else {
			return;
		};
		// Slight delay so the card doesn't flash while scanning the rail.
		if hovered_at.elapsed() < std::time::Duration::from_millis(280) {
			return;
		}
		let Some(user_index) = self.ui.hovered_message_index else {
			return;
		};
		let Some(user_msg) = self.messages.get(user_index) else {
			return;
		};
		let answer = self
			.messages
			.iter()
			.skip(user_index + 1)
			.take_while(|message| message.role != crate::components::MessageRole::User)
			.find(|message| {
				message.role == crate::components::MessageRole::Assistant
					&& !message.content.trim().is_empty()
			});

		// Prefer a clean assistant preview; fall back to the user prompt.
		let body_src = answer.map(|m| m.content.as_str()).unwrap_or(user_msg.content.as_str());
		let summary = clean_hover_preview_text(body_src);
		if summary.is_empty() {
			return;
		}

		const MAX_BODY_LINES: usize = 3;
		let width = area.width.saturating_sub(6).clamp(32, 52);
		let text_width = width.saturating_sub(4).max(8) as usize; // inner padding + borders
		let height = (MAX_BODY_LINES as u16).saturating_add(2); // borders

		// Anchor next to the minimap; keep fully on-screen.
		let x = self.ui.minimap_area.right().saturating_add(1).min(area.right().saturating_sub(width));
		// Prefer near the top of the rail; clamp if near bottom.
		let y = self.ui.minimap_area.y.min(area.bottom().saturating_sub(height));
		let card = Rect { x, y, width, height };

		// Word-wrap then hard-cap rows; ellipsize the last line when clipped.
		let mut wrapped = components::clip_lines_to_width(vec![Line::from(summary)], text_width);
		let truncated = wrapped.len() > MAX_BODY_LINES;
		wrapped.truncate(MAX_BODY_LINES);
		if truncated && let Some(last) = wrapped.last_mut() {
			*last = Line::from(ellipsize_line_spans(last, text_width));
		}

		let tokens = answer
			.map(|a| a.tokens_out.map_or(a.token_count, |t| t as usize))
			.unwrap_or(user_msg.token_count);
		let turn_n =
			self.user_message_indices().iter().position(|&i| i == user_index).map(|i| i + 1).unwrap_or(1);
		let title = if answer.is_some() {
			format!(" Assistant · turn {turn_n} · {tokens} tok ")
		} else {
			format!(" You · turn {turn_n} ")
		};

		// Theme-aware plate (never hard-coded dark) so light themes stay readable.
		let card_bg = if self.theme.card != self.theme.bg { self.theme.card } else { self.theme.bg };
		let card_fg = self.theme.fg;
		let card_border = self.theme.primary;
		// Fill plate first so no terminal default bleeds through.
		for y in card.top()..card.bottom() {
			for x in card.left()..card.right() {
				let cell = &mut buf[(x, y)];
				cell.reset();
				cell.set_bg(card_bg);
			}
		}
		// Soft card: theme surface + primary border.
		let body: Vec<Line<'static>> = wrapped
			.into_iter()
			.map(|line| {
				let spans: Vec<Span<'static>> = line
					.spans
					.into_iter()
					.map(|s| Span::styled(s.content.to_string(), Style::default().fg(card_fg).bg(card_bg)))
					.collect();
				Line::from(spans)
			})
			.collect();
		Paragraph::new(Text::from(body))
			.block(
				Block::default()
					.borders(Borders::ALL)
					.border_style(Style::default().fg(card_border).bg(card_bg))
					.title(Span::styled(
						title,
						Style::default().fg(card_border).bg(card_bg).add_modifier(Modifier::BOLD),
					))
					.style(Style::default().bg(card_bg).fg(card_fg)),
			)
			.render(card, buf);
	}

	pub fn render_inline_sidebar(&mut self, area: Rect, buf: &mut Buffer) {
		let chunks = Layout::default()
			.direction(Direction::Vertical)
			.constraints([Constraint::Length(1), Constraint::Length(1)])
			.split(area);

		// Title — primary color, same left pad as full sidebar
		let title = format!(" {}", self.session.session_name);
		ratatui::widgets::Paragraph::new(ratatui::text::Span::styled(
			&title,
			ratatui::style::Style::default()
				.fg(self.theme.primary)
				.add_modifier(ratatui::style::Modifier::BOLD),
		))
		.render(chunks[0], buf);

		let short_id = crate::session_store::short_session_id(&self.session.chat_id);
		if !short_id.is_empty() {
			ratatui::widgets::Paragraph::new(ratatui::text::Span::styled(
				format!(" #{short_id}"),
				ratatui::style::Style::default()
					.fg(components::SOFT_MUTED_FG)
					.add_modifier(ratatui::style::Modifier::DIM),
			))
			.render(chunks[1], buf);
		}
	}

	pub fn render_perf_overlay(&self, area: Rect, buf: &mut Buffer) {
		if !self.ui.show_perf_overlay {
			return;
		}

		let stats = self.perf_monitor.get_stats();

		// Create overlay area (top-right corner, 50 chars wide, 10 lines tall)
		let overlay_width = 52.min(area.width);
		let overlay_height = 10.min(area.height);
		let overlay_area = Rect {
			x: area.width.saturating_sub(overlay_width),
			y: 0,
			width: overlay_width,
			height: overlay_height,
		};

		let status_color = if self.perf_monitor.is_meeting_targets() {
			ratatui::style::Color::Green
		} else if stats.avg_input_render_ms < 33.0 {
			ratatui::style::Color::Yellow
		} else {
			ratatui::style::Color::Red
		};

		// Build content lines
		let lines = vec![
			Line::from(vec![
				Span::styled("⚡ ", Style::default().fg(ratatui::style::Color::Yellow)),
				Span::styled(
					"Performance Monitor",
					Style::default().fg(ratatui::style::Color::Cyan).add_modifier(Modifier::BOLD),
				),
			]),
			Line::from(""),
			Line::from(vec![
				Span::raw("Input:    "),
				Span::styled(
					format!("{:.2}ms", stats.avg_input_render_ms),
					Style::default().fg(if stats.avg_input_render_ms < 16.0 {
						ratatui::style::Color::Green
					} else {
						ratatui::style::Color::Yellow
					}),
				),
			]),
			Line::from(vec![
				Span::raw("Status:  "),
				Span::styled(
					if self.perf_monitor.is_meeting_targets() { "✓ EXCELLENT" } else { "○ GOOD" },
					Style::default().fg(status_color).add_modifier(Modifier::BOLD),
				),
			]),
		];

		let block = Block::default()
			.borders(Borders::ALL)
			.border_style(Style::default().fg(status_color))
			.border_type(ratatui::widgets::BorderType::Rounded)
			.style(Style::default().bg(self.theme.bg));

		let paragraph = Paragraph::new(lines).block(block).style(Style::default().fg(self.theme.fg));

		paragraph.render(overlay_area, buf);
	}
}

impl ChatState {
	/// Render toast notification in top-right corner
	pub fn render_toast(&self, area: Rect, buf: &mut Buffer) {
		if let Some(ref message) = self.ui.toast_message {
			if area.width == 0 || area.height == 0 {
				return;
			}

			// Toast dimensions
			let toast_width =
				(unicode_width::UnicodeWidthStr::width(message.as_str()) as u16 + 4).min(area.width);
			let toast_height = 3.min(area.height);
			if toast_width == 0 || toast_height == 0 {
				return;
			}

			// Position in top-right corner
			let toast_x = area.width.saturating_sub(toast_width);
			let toast_y = 0;

			let toast_area = Rect { x: toast_x, y: toast_y, width: toast_width, height: toast_height };

			// Create toast with border
			let block = Block::default()
				.borders(Borders::ALL)
				.border_style(Style::default().fg(self.theme.accent))
				.border_type(ratatui::widgets::BorderType::Rounded)
				.style(Style::default().bg(self.theme.bg));

			let inner = block.inner(toast_area);
			block.render(toast_area, buf);

			// Render message text
			let text = Paragraph::new(message.as_str())
				.style(Style::default().fg(self.theme.fg))
				.alignment(ratatui::layout::Alignment::Center);

			text.render(inner, buf);
		}
	}

	/// Render intro/outro indicators in top-left corner (for carousel screens)
	pub fn render_animation_indicators(
		&self,
		area: Rect,
		current_anim: AnimationType,
		buf: &mut Buffer,
	) {
		if current_anim == AnimationType::Splash || current_anim == AnimationType::FileBrowser {
			return;
		}

		let mut lines = Vec::new();

		// Show intro indicator
		if self.animation.intro_animation == current_anim {
			lines.push(Line::from(vec![
				Span::styled("▲ ", Style::default().fg(self.theme.accent)),
				Span::styled("INTRO", Style::default().fg(self.theme.fg)),
			]));
		}

		// Show outro indicator
		if self.animation.outro_animation == current_anim {
			lines.push(Line::from(vec![
				Span::styled("▼ ", Style::default().fg(self.theme.accent)),
				Span::styled("OUTRO", Style::default().fg(self.theme.fg)),
			]));
		}

		if lines.is_empty() {
			return;
		}

		// Calculate dimensions
		let indicator_height = lines.len() as u16 + 2; // +2 for border
		let indicator_width = 12; // Fixed width for "▼ OUTRO" + padding

		let indicator_area = Rect {
			x: area.x,
			y: area.y,
			width: indicator_width.min(area.width),
			height: indicator_height.min(area.height),
		};

		// Create indicator box with border
		let block = Block::default()
			.borders(Borders::ALL)
			.border_style(Style::default().fg(self.theme.accent))
			.border_type(ratatui::widgets::BorderType::Rounded)
			.style(Style::default().bg(self.theme.bg));

		let inner = block.inner(indicator_area);
		block.render(indicator_area, buf);

		// Render indicator text
		let text = Paragraph::new(lines).style(Style::default().fg(self.theme.fg));

		text.render(inner, buf);
	}
}

#[cfg(test)]
mod tests {
	use super::{ChatState, truncate_start};
	use ratatui::{buffer::Buffer, layout::Rect};

	/// Convert a Buffer to a snapshot-friendly string showing visible content.
	/// Newlines separate rows. Trailing spaces are trimmed per row, and
	/// trailing empty rows are omitted.
	fn buffer_to_snapshot(buf: &Buffer) -> String {
		let area = buf.area;
		let mut lines: Vec<String> = Vec::new();
		for y in area.top()..area.bottom() {
			let mut line = String::new();
			let mut trailing_spaces = 0u16;
			for x in area.left()..area.right() {
				let cell = &buf[(x, y)];
				let ch = cell.symbol().chars().next().unwrap_or(' ');
				if ch == ' ' {
					trailing_spaces += 1;
				} else {
					for _ in 0..trailing_spaces {
						line.push(' ');
					}
					trailing_spaces = 0;
					line.push(ch);
				}
			}
			if !line.is_empty() {
				lines.push(line);
			} else if !lines.is_empty() {
				lines.push(String::new());
			}
		}
		while lines.last().map_or(false, |s| s.is_empty()) {
			lines.pop();
		}
		lines.join("\n")
	}

	/// Assert rendering produces a known snapshot stored under
	/// `tests/snapshots/chat_render/{name}.snap`.  Delete the file to
	/// regenerate.
	fn check_render_snapshot(name: &str, area: Rect, render: impl FnOnce(&mut Buffer)) {
		let mut buf = Buffer::empty(area);
		render(&mut buf);
		let got = buffer_to_snapshot(&buf);

		let mut snap_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
		snap_path.push("tests");
		snap_path.push("snapshots");
		snap_path.push("chat_render");
		std::fs::create_dir_all(&snap_path).ok();
		snap_path.push(name);
		snap_path.set_extension("snap");

		let snap_str = format!("{got}\n");
		match std::fs::read_to_string(&snap_path) {
			Ok(existing) if existing == snap_str => {}
			Ok(existing) => {
				let mut new_path = snap_path.clone();
				new_path.set_extension("snap.new");
				std::fs::write(&new_path, &snap_str).ok();
				panic!(
					"snapshot mismatch for `{name}`\n\
					 new content written to `{}`\n\
					 expected:\n{existing}\n\
					 ——— got:\n{got}\n———",
					new_path.display()
				);
			}
			Err(_) => {
				std::fs::write(&snap_path, &snap_str)
					.unwrap_or_else(|e| panic!("cannot write snapshot {snap_path:?}: {e}"));
			}
		}
	}

	// ── truncate_start ────────────────────────────────────────────────

	#[test]
	fn truncate_start_preserves_utf8_boundaries() {
		assert_eq!(truncate_start("/αβγδεζη", 5), "..εζη");
	}

	// ── toast snapshot tests ──────────────────────────────────────────

	#[test]
	fn toast_render_clamps_to_tiny_area() {
		let mut state = ChatState::new();
		state.show_toast("hello".to_string());
		let area = Rect { x: 0, y: 0, width: 1, height: 1 };
		let mut buffer = Buffer::empty(area);
		state.render_toast(area, &mut buffer);
	}

	#[test]
	fn toast_render_normal() {
		let mut state = ChatState::new();
		state.ui.toast_message = Some("Connected".to_string());
		check_render_snapshot("toast_normal", Rect::new(0, 0, 30, 3), |buf| {
			state.render_toast(Rect::new(0, 0, 30, 24), buf);
		});
	}

	#[test]
	fn toast_render_empty_when_no_message() {
		let state = ChatState::new();
		check_render_snapshot("toast_empty", Rect::new(0, 0, 30, 3), |buf| {
			state.render_toast(Rect::new(0, 0, 30, 24), buf);
		});
	}

	#[test]
	fn toast_render_zero_area() {
		let mut state = ChatState::new();
		state.ui.toast_message = Some("hello".to_string());
		check_render_snapshot("toast_zero", Rect::new(0, 0, 0, 0), |buf| {
			state.render_toast(Rect::new(0, 0, 0, 0), buf);
		});
	}

	// ── suggestion bar snapshot tests ─────────────────────────────────

	fn state_with_suggestions() -> ChatState {
		let mut state = ChatState::new();
		state.input.suggestions = vec![
			crate::input::SuggestItem {
				value: "/sessions".into(),
				label: "/sessions".into(),
				description: "Switch session".into(),
			},
			crate::input::SuggestItem {
				value: "/rename".into(),
				label: "/rename".into(),
				description: "Rename session".into(),
			},
			crate::input::SuggestItem {
				value: "/help".into(),
				label: "/help".into(),
				description: "Show help".into(),
			},
		];
		state.input.suggestion_kind = Some(crate::input::SuggestKind::Slash);
		state
	}

	#[test]
	fn suggestion_bar_render_empty() {
		let state = ChatState::new();
		check_render_snapshot("suggest_bar_empty", Rect::new(0, 0, 60, 3), |buf| {
			state.render_suggestion_bar(Rect::new(0, 0, 60, 3), buf);
		});
	}

	#[test]
	fn suggestion_bar_render_three_items() {
		let state = state_with_suggestions();
		check_render_snapshot("suggest_bar_three", Rect::new(0, 0, 60, 3), |buf| {
			state.render_suggestion_bar(Rect::new(0, 0, 60, 3), buf);
		});
	}

	#[test]
	fn suggestion_bar_render_zero_height() {
		let state = state_with_suggestions();
		check_render_snapshot("suggest_bar_zero_h", Rect::new(0, 0, 60, 0), |buf| {
			state.render_suggestion_bar(Rect::new(0, 0, 60, 0), buf);
		});
	}

	// ── input box snapshot tests ──────────────────────────────────────

	#[test]
	fn input_box_render_empty() {
		let mut state = ChatState::new();
		check_render_snapshot("input_box_empty", Rect::new(0, 0, 60, 3), |buf| {
			state.render_input_box(Rect::new(0, 0, 60, 3), buf);
		});
	}

	#[test]
	fn input_box_render_with_text() {
		let mut state = ChatState::new();
		state.input.content = "hello world".to_string();
		state.input.cursor_position = 11;
		check_render_snapshot("input_box_text", Rect::new(0, 0, 60, 3), |buf| {
			state.render_input_box(Rect::new(0, 0, 60, 3), buf);
		});
	}

	#[test]
	fn input_box_render_multiline() {
		let mut state = ChatState::new();
		state.input.content = "line one\nline two\nline three".to_string();
		state.input.cursor_position = 26;
		check_render_snapshot("input_box_multiline", Rect::new(0, 0, 60, 5), |buf| {
			state.render_input_box(Rect::new(0, 0, 60, 5), buf);
		});
	}

	#[test]
	fn input_box_render_zero_width() {
		let mut state = ChatState::new();
		state.input.content = "hello".to_string();
		check_render_snapshot("input_box_zero_w", Rect::new(0, 0, 0, 3), |buf| {
			state.render_input_box(Rect::new(0, 0, 0, 3), buf);
		});
	}

	// ── bottom controls snapshot tests ────────────────────────────────

	#[test]
	fn bottom_controls_render_narrow() {
		let mut state = ChatState::new();
		check_render_snapshot("bottom_controls_narrow", Rect::new(0, 0, 40, 1), |buf| {
			state.render_bottom_controls(Rect::new(0, 0, 40, 1), buf, false);
		});
	}

	#[test]
	fn bottom_controls_render_wide() {
		let mut state = ChatState::new();
		check_render_snapshot("bottom_controls_wide", Rect::new(0, 0, 120, 1), |buf| {
			state.render_bottom_controls(Rect::new(0, 0, 120, 1), buf, false);
		});
	}

	#[test]
	fn bottom_controls_render_zero_width() {
		let mut state = ChatState::new();
		check_render_snapshot("bottom_controls_zero", Rect::new(0, 0, 0, 1), |buf| {
			state.render_bottom_controls(Rect::new(0, 0, 0, 1), buf, false);
		});
	}

	// ── no-panic edge-case tests ──────────────────────────────────────

	#[test]
	fn cursor_render_ignores_zero_width_input_area() {
		let state = ChatState::new();
		let mut buffer = Buffer::empty(Rect { x: 0, y: 0, width: 1, height: 1 });
		state.render_input_cursor(Rect { x: 0, y: 0, width: 0, height: 1 }, &mut buffer);
	}
}
