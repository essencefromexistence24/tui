//! Multi-step plan wizard (OpenCode-style tabbed question UI).

#![allow(dead_code)]
//! Steps: folder → formatter → linter → LSP → VCS → shell → Confirm.

use ratatui::{
	buffer::Buffer,
	layout::{Constraint, Direction, Layout, Rect},
	style::{Color, Modifier, Style},
	text::{Line, Span},
	widgets::{Block, Borders, Paragraph, Widget},
};

use crate::goal_runner::PlanOptions;

const TAB_ACTIVE_BG: Color = Color::Rgb(0x33, 0x66, 0xcc);
const TAB_ACTIVE_FG: Color = Color::Rgb(0xff, 0xff, 0xff);
const TAB_INACTIVE_BG: Color = Color::Rgb(0x1a, 0x1a, 0x2a);
const TAB_INACTIVE_FG: Color = Color::Rgb(0x88, 0x88, 0xaa);
const TAB_DONE_BG: Color = Color::Rgb(0x1a, 0x3a, 0x2a);
const TAB_DONE_FG: Color = Color::Rgb(0x66, 0xcc, 0x88);
const OPTION_ACTIVE_BG: Color = Color::Rgb(0x2a, 0x2a, 0x44);
const OPTION_ACTIVE_FG: Color = Color::Rgb(0xff, 0xff, 0xff);
const OPTION_INACTIVE_FG: Color = Color::Rgb(0xcc, 0xcc, 0xdd);
const OPTION_DESC_FG: Color = Color::Rgb(0x88, 0x88, 0xaa);
const TIPS_FG: Color = Color::Rgb(0x66, 0x66, 0x88);
const HEADER_FG: Color = Color::Rgb(0xff, 0xcc, 0x66);

#[derive(Debug, Clone)]
pub struct PlanWizardOption {
	pub label: String,
	pub description: String,
}

#[derive(Debug, Clone)]
pub struct PlanWizardStep {
	pub header: &'static str,
	pub question: String,
	pub options: Vec<PlanWizardOption>,
}

#[derive(Debug, Clone)]
pub struct PlanWizard {
	pub active: bool,
	pub steps: Vec<PlanWizardStep>,
	pub tab: usize,
	pub selected: Vec<usize>,
	pub answers: Vec<String>,
}

impl PlanWizard {
	pub fn new(plan: &PlanOptions) -> Self {
		let folder = plan.target_folder.clone().unwrap_or_else(|| ".".into());
		let steps = vec![
			PlanWizardStep {
				header: "Folder",
				question: format!("Working folder: {folder}"),
				options: vec![
					PlanWizardOption {
						label: folder.clone(),
						description: "Use current working folder".into(),
					},
					PlanWizardOption {
						label: "Reset to cwd".into(),
						description: "Reset to current directory".into(),
					},
				],
			},
			PlanWizardStep {
				header: "Formatter",
				question: "Run code formatter before planning?".into(),
				options: vec![
					PlanWizardOption { label: "Yes".into(), description: "Check & format code style".into() },
					PlanWizardOption { label: "No".into(), description: "Skip formatting".into() },
				],
			},
			PlanWizardStep {
				header: "Linter",
				question: "Run static analysis before planning?".into(),
				options: vec![
					PlanWizardOption { label: "Yes".into(), description: "Check for lint errors".into() },
					PlanWizardOption { label: "No".into(), description: "Skip linting".into() },
				],
			},
			PlanWizardStep {
				header: "LSP",
				question: "Use LSP diagnostics before planning?".into(),
				options: vec![
					PlanWizardOption { label: "Yes".into(), description: "Get project diagnostics".into() },
					PlanWizardOption { label: "No".into(), description: "Skip diagnostics".into() },
				],
			},
			PlanWizardStep {
				header: "VCS",
				question: "Check VCS (git) status before planning?".into(),
				options: vec![
					PlanWizardOption { label: "Yes".into(), description: "Show git status & diff".into() },
					PlanWizardOption { label: "No".into(), description: "Skip VCS check".into() },
				],
			},
			PlanWizardStep {
				header: "Shell",
				question: "Allow shell tools during plan phase?".into(),
				options: vec![
					PlanWizardOption { label: "Yes".into(), description: "Permit shell execution".into() },
					PlanWizardOption { label: "No".into(), description: "Read-only plan phase".into() },
				],
			},
		];
		let n = steps.len();
		Self {
			active: false,
			selected: vec![0; n + 1], // +1 for confirm tab
			answers: vec![String::new(); n],
			tab: 0,
			steps,
		}
	}

	pub fn reset(&mut self, plan: &PlanOptions) {
		let folder = plan.target_folder.clone().unwrap_or_else(|| ".".into());
		if let Some(step) = self.steps.get_mut(0) {
			step.options[0].label = folder.clone();
			step.question = format!("Working folder: {folder}");
		}
		self.tab = 0;
		self.selected = vec![0; self.steps.len() + 1];
		self.answers = vec![String::new(); self.steps.len()];
	}

	pub fn is_confirm_tab(&self) -> bool {
		self.tab >= self.steps.len()
	}

	pub fn tab_count(&self) -> usize {
		self.steps.len() + 1 // + Confirm
	}

	pub fn tab_label(&self, i: usize) -> &str {
		if i < self.steps.len() { self.steps[i].header } else { "Confirm" }
	}

	pub fn is_done(&self, i: usize) -> bool {
		i < self.answers.len() && !self.answers[i].is_empty()
	}

	pub fn move_selection(&mut self, delta: i32) {
		let n = if self.is_confirm_tab() {
			1 // Confirm tab has no options
		} else {
			self.steps[self.tab].options.len()
		};
		if n <= 1 {
			return;
		}
		let cur = self.selected[self.tab] as i32;
		let next = (cur + delta).rem_euclid(n as i32);
		self.selected[self.tab] = next as usize;
	}

	pub fn move_tab(&mut self, delta: i32) {
		let n = self.tab_count() as i32;
		let next = (self.tab as i32 + delta).rem_euclid(n);
		self.tab = next as usize;
	}

	/// Select the current option (auto-advances to next tab).
	pub fn select_current(&mut self) -> bool {
		if self.is_confirm_tab() {
			return true; // confirm pressed
		}
		let step = &self.steps[self.tab];
		let opt = &step.options[self.selected[self.tab]];
		self.answers[self.tab] = opt.label.clone();
		if self.tab + 1 < self.tab_count() {
			self.tab += 1;
		}
		false
	}

	/// Build PlanOptions from wizard answers (does not apply defaults).
	pub fn to_plan_options(&self) -> PlanOptions {
		let mut p = PlanOptions::default();
		if let Some(ans) = self.answers.first()
			&& ans == "Reset to cwd"
			&& let Ok(cwd) = std::env::current_dir()
		{
			p.target_folder = Some(cwd.display().to_string());
		}
		if let Some(ans) = self.answers.get(1) {
			p.run_formatter = ans == "Yes";
		}
		if let Some(ans) = self.answers.get(2) {
			p.run_linter = ans == "Yes";
		}
		if let Some(ans) = self.answers.get(3) {
			p.use_lsp = ans == "Yes";
		}
		if let Some(ans) = self.answers.get(4) {
			p.use_vcs = ans == "Yes";
		}
		if let Some(ans) = self.answers.get(5) {
			p.allow_shell = ans == "Yes";
		}
		p
	}
}

/// Render the plan wizard as a centered overlay within `area`.
pub fn render_plan_wizard(area: Rect, buf: &mut Buffer, wiz: &PlanWizard) {
	let overlay_w = area.width.clamp(40, 68);
	let overlay_x = area.x + (area.width.saturating_sub(overlay_w)) / 2;
	let overlay_h = 20u16.min(area.height.saturating_sub(2));
	let overlay_y = area.y + 2.min(area.height.saturating_sub(overlay_h) / 3);

	let overlay = Rect { x: overlay_x, y: overlay_y, width: overlay_w, height: overlay_h };
	if overlay.width < 12 || overlay.height < 6 {
		return;
	}

	// Background block
	let block = Block::default()
		.borders(Borders::ALL)
		.border_style(Style::default().fg(Color::Rgb(0x55, 0x66, 0x88)))
		.style(Style::default().bg(Color::Rgb(0x0d, 0x0d, 0x1a)));
	block.clone().render(overlay, buf);
	let inner = block.inner(overlay);

	// ── Layout ──
	let chunks = Layout::default()
		.direction(Direction::Vertical)
		.constraints([
			Constraint::Length(2), // title
			Constraint::Length(1), // tab bar
			Constraint::Length(1), // gap
			Constraint::Length(1), // question header
			Constraint::Length(1), // gap
			Constraint::Min(2),    // options
			Constraint::Length(1), // gap
			Constraint::Length(1), // tips
		])
		.split(inner);

	let title_style = Style::default().fg(Color::Rgb(0xff, 0xcc, 0x66)).add_modifier(Modifier::BOLD);
	let title_text = "Plan Configuration".to_string();
	let title_line = Line::from(Span::styled(title_text, title_style));
	Paragraph::new(title_line).render(chunks[0], buf);

	// ── Tab bar ──
	render_tab_bar(chunks[1], buf, wiz);

	// ── Question / Confirm content ──
	if wiz.is_confirm_tab() {
		render_confirm_content(chunks[3], chunks[5], buf, wiz);
	} else {
		render_step_content(chunks[3], chunks[5], buf, wiz);
	}

	// ── Tips ──
	render_tips(chunks[7], buf, wiz);
}

fn render_tab_bar(area: Rect, buf: &mut Buffer, wiz: &PlanWizard) {
	if area.width < 12 {
		return;
	}
	let n = wiz.tab_count();
	let tab_w = area.width / n as u16;
	let tab_w = tab_w.max(8);

	for i in 0..n {
		let x = area.x + i as u16 * tab_w;
		let tab_area = Rect {
			x,
			y: area.y,
			width: tab_w.min(area.width.saturating_sub(x.saturating_sub(area.x))),
			height: area.height,
		};
		if tab_area.width < 2 {
			continue;
		}

		let is_active = i == wiz.tab;
		let is_done = wiz.is_done(i);

		let (bg, fg) = if is_active {
			(TAB_ACTIVE_BG, TAB_ACTIVE_FG)
		} else if is_done {
			(TAB_DONE_BG, TAB_DONE_FG)
		} else {
			(TAB_INACTIVE_BG, TAB_INACTIVE_FG)
		};

		let label = wiz.tab_label(i);
		let prefix = if is_done { "✓ " } else { "  " };
		let text = format!("{}{}", prefix, label);

		let mut style = Style::default().bg(bg).fg(fg);
		if is_active {
			style = style.add_modifier(Modifier::BOLD);
		}

		for x in tab_area.x..tab_area.right() {
			for y in tab_area.y..tab_area.bottom() {
				let cell = &mut buf[(x, y)];
				cell.set_style(style);
			}
		}
		let text_trimmed: String = text.chars().take(tab_area.width as usize - 1).collect();
		Paragraph::new(Line::from(Span::styled(text_trimmed, style))).render(tab_area, buf);
	}
}

fn render_step_content(
	question_area: Rect,
	options_area: Rect,
	buf: &mut Buffer,
	wiz: &PlanWizard,
) {
	if wiz.tab >= wiz.steps.len() {
		return;
	}
	let step = &wiz.steps[wiz.tab];

	// Question header
	let q_style = Style::default().fg(HEADER_FG).add_modifier(Modifier::BOLD);
	let q_text: String = step.question.chars().take(question_area.width as usize - 1).collect();
	Paragraph::new(Line::from(Span::styled(q_text, q_style))).render(question_area, buf);

	// Options
	let sel = wiz.selected[wiz.tab];
	let mut y = options_area.y;
	for (i, opt) in step.options.iter().enumerate() {
		if y >= options_area.bottom() {
			break;
		}
		let is_active = i == sel;
		let num = format!("{}.", i + 1);
		let mark = if is_active { "▸" } else { " " };

		let bg = if is_active { OPTION_ACTIVE_BG } else { Color::Reset };
		let fg = if is_active { OPTION_ACTIVE_FG } else { OPTION_INACTIVE_FG };

		let line = Line::from(vec![
			Span::styled(
				format!("{} ", mark),
				Style::default().fg(if is_active { TAB_ACTIVE_BG } else { Color::Rgb(0x55, 0x55, 0x77) }),
			),
			Span::styled(
				format!("{} ", num),
				Style::default().fg(Color::Rgb(0x88, 0x88, 0xaa)).add_modifier(Modifier::DIM),
			),
			Span::styled(
				opt.label.clone(),
				Style::default().fg(fg).add_modifier(if is_active {
					Modifier::BOLD
				} else {
					Modifier::empty()
				}),
			),
		]);

		if is_active {
			for x in options_area.x..options_area.right() {
				let cell = &mut buf[(x, y)];
				cell.set_bg(bg);
			}
		}
		Paragraph::new(line).render(
			Rect { x: options_area.x + 1, y, width: options_area.width.saturating_sub(2), height: 1 },
			buf,
		);
		y += 1;

		// Description (dimmed, below the option)
		if y < options_area.bottom() {
			let desc_style = Style::default().fg(OPTION_DESC_FG).add_modifier(Modifier::ITALIC);
			let desc: String = opt.description.chars().take(options_area.width as usize - 4).collect();
			let desc_line = Line::from(vec![Span::raw("    "), Span::styled(desc, desc_style)]);
			if is_active {
				for x in options_area.x..options_area.right() {
					let cell = &mut buf[(x, y)];
					cell.set_bg(bg);
				}
			}
			Paragraph::new(desc_line).render(
				Rect { x: options_area.x + 1, y, width: options_area.width.saturating_sub(2), height: 1 },
				buf,
			);
			y += 1;
		}
	}
}

fn render_confirm_content(header_area: Rect, body_area: Rect, buf: &mut Buffer, wiz: &PlanWizard) {
	let q_style = Style::default().fg(HEADER_FG).add_modifier(Modifier::BOLD);
	let q_text: String =
		"Review your plan settings".chars().take(header_area.width as usize - 1).collect();
	Paragraph::new(Line::from(Span::styled(q_text, q_style))).render(header_area, buf);

	// Summary
	let mut y = body_area.y;
	for (i, step) in wiz.steps.iter().enumerate() {
		if y >= body_area.bottom() {
			break;
		}
		let ans = wiz.answers.get(i).map(|a| a.as_str()).unwrap_or("—");
		let done = wiz.is_done(i);
		let fg = if done { Color::Rgb(0x66, 0xcc, 0x88) } else { Color::Rgb(0xff, 0x66, 0x66) };
		let glyph = if done { "✓" } else { "✗" };

		let line = Line::from(vec![
			Span::styled(format!("{} ", glyph), Style::default().fg(fg)),
			Span::styled(
				format!("{}: ", step.header),
				Style::default().fg(Color::Rgb(0xaa, 0xaa, 0xcc)).add_modifier(Modifier::BOLD),
			),
			Span::styled(ans.to_string(), Style::default().fg(Color::Rgb(0xcc, 0xcc, 0xee))),
		]);
		Paragraph::new(line).render(
			Rect { x: body_area.x + 1, y, width: body_area.width.saturating_sub(2), height: 1 },
			buf,
		);
		y += 1;
	}

	// Run button hint
	if y < body_area.bottom() {
		y += 1;
		if y < body_area.bottom() {
			let run_style =
				Style::default().fg(Color::Rgb(0x22, 0xc5, 0x5e)).add_modifier(Modifier::BOLD);
			let run_text = "▶  Press Enter to run plan tools & attach results";
			let run: String = run_text.chars().take(body_area.width as usize - 2).collect();
			Paragraph::new(Line::from(Span::styled(run, run_style))).render(
				Rect { x: body_area.x + 1, y, width: body_area.width.saturating_sub(2), height: 1 },
				buf,
			);
		}
	}
}

fn render_tips(area: Rect, buf: &mut Buffer, wiz: &PlanWizard) {
	let (tips, tip_fg) = if wiz.is_confirm_tab() {
		("↑↓ navigate  Enter run plan  Esc dismiss", Color::Rgb(0x22, 0xc5, 0x5e))
	} else {
		("← → tab  ↑↓ select  Enter pick & advance  Esc dismiss", Color::Rgb(0x66, 0x66, 0x88))
	};
	let style = Style::default().fg(tip_fg).add_modifier(Modifier::DIM);
	let tip: String = tips.chars().take(area.width as usize - 1).collect();
	Paragraph::new(Line::from(Span::styled(tip, style))).render(area, buf);
}
