//! Context-aware bottom-bar **center** (OpenCode footer-shaped).

#![allow(dead_code)]
//!
//! Priority (highest first):
//! 1. Pending tool **permission** → [once] [always] [deny] hover buttons
//! 2. Pending **question** → option chips
//! 3. **Goal** mode (active) → live timer + pause/resume/extend
//! 4. **Plan** mode → plan tool toggles + approve→Write
//! 5. Profile actions (Write/Agent/Ask) instead of generic tips
//! 6. Rotating tips (fallback)

use crate::{
	goal_runner::PlanOptions, modes::AgentMode, permission_hub::PendingPermission,
	question_hub::PendingQuestion, tools::PermissionDecision,
};

/// Clickable / hoverable center chip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CenterAction {
	// Permissions
	PermOnce,
	PermAlways,
	PermDeny,
	// Questions — select option by index, or confirm/dismiss
	QuestionPick(usize),
	QuestionConfirm,
	QuestionDismiss,
	// Goal
	GoalPause,
	GoalResume,
	GoalExtend,
	// Plan options
	PlanToggleFmt,
	PlanToggleLint,
	PlanToggleLsp,
	PlanToggleVcs,
	PlanToggleShell,
	/// Switch Plan → Write and execute the plan (OpenCode plan_exit / build).
	PlanApproveWrite,
	/// Show paste/attachment preview popup
	ShowPastePreview,
	ActionOpenPlan,
	ActionStartGoal,
	/// Jump message list to top (always available on center bar).
	ScrollChatTop,
	/// Jump message list to bottom (always available on center bar).
	ScrollChatBottom,
}

/// Always-on scroll chips shown on every profile in the bottom-bar center.
pub fn scroll_nav_chips() -> Vec<CenterChip> {
	vec![
		CenterChip { action: CenterAction::ScrollChatTop, label: "↑".into(), hint: None },
		CenterChip { action: CenterAction::ScrollChatBottom, label: "↓".into(), hint: None },
	]
}

#[derive(Debug, Clone)]
pub struct CenterChip {
	pub action: CenterAction,
	pub label: String,
	/// Optional keyboard digit/letter shown in the chip.
	pub hint: Option<char>,
}

#[derive(Debug, Clone)]
pub enum CenterContent {
	/// Static centered text (timer / tip).
	Text {
		text: String,
		/// Optional accent style key: "goal" | "warn" | "muted" | "plan"
		style: CenterTextStyle,
	},
	/// Row of hoverable chips + optional leading label.
	Chips { label: Option<String>, chips: Vec<CenterChip> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CenterTextStyle {
	Muted,
}

/// Build what the bottom-bar center should show right now.
#[allow(clippy::too_many_arguments)]
pub fn build_center(
	mode: AgentMode,
	goal_active: bool,
	goal_timer_line: Option<String>,
	goal_paused: bool,
	plan: &PlanOptions,
	perm: Option<&PendingPermission>,
	question: Option<&PendingQuestion>,
	fallback_tip: &str,
	show_actions: bool,
	paste_blocks: &[crate::input::PasteBlock],
	attachments: &[crate::input::Attachment],
) -> CenterContent {
	// 1) Permission (highest priority — OpenCode footer)
	if let Some(p) = perm {
		let preview: String = p.preview.chars().take(28).collect();
		let label = if preview.is_empty() {
			format!("△ {}", p.tool)
		} else {
			format!("△ {} · {preview}", p.tool)
		};
		return CenterContent::Chips {
			label: Some(label),
			chips: vec![
				CenterChip { action: CenterAction::PermOnce, label: "once".into(), hint: Some('y') },
				CenterChip { action: CenterAction::PermAlways, label: "always".into(), hint: Some('a') },
				CenterChip { action: CenterAction::PermDeny, label: "deny".into(), hint: Some('n') },
			],
		};
	}

	// 2) Question dock chips
	if let Some(q) = question {
		let mut chips = Vec::new();
		for (i, opt) in q.options.iter().enumerate().take(6) {
			let selected = i == q.selected;
			let mark = if selected { "›" } else { " " };
			let short: String = opt.chars().take(16).collect();
			chips.push(CenterChip {
				action: CenterAction::QuestionPick(i),
				label: format!("{mark}{short}"),
				hint: None,
			});
		}
		chips.push(CenterChip {
			action: CenterAction::QuestionConfirm,
			label: "ok".into(),
			hint: Some('↵'),
		});
		chips.push(CenterChip {
			action: CenterAction::QuestionDismiss,
			label: "esc".into(),
			hint: None,
		});
		let prompt: String = q.prompt.chars().take(24).collect();
		return CenterContent::Chips { label: Some(format!("? {prompt}")), chips };
	}

	// 3) Goal timer (center of bar — replaces tips)
	if mode == AgentMode::Goal
		&& goal_active
		&& let Some(timer) = goal_timer_line
	{
		let mut chips = Vec::new();
		if goal_paused {
			chips.push(CenterChip {
				action: CenterAction::GoalResume,
				label: "resume".into(),
				hint: Some('r'),
			});
		} else {
			chips.push(CenterChip {
				action: CenterAction::GoalPause,
				label: "pause".into(),
				hint: Some('p'),
			});
		}
		chips.push(CenterChip {
			action: CenterAction::GoalExtend,
			label: "+15m".into(),
			hint: Some('+'),
		});
		return CenterContent::Chips { label: Some(timer), chips };
	}

	// 4) Plan mode options (OpenCode-shaped checklist in center)
	if mode == AgentMode::Plan {
		let on = |b: bool| if b { "✓" } else { "·" };
		return CenterContent::Chips {
			label: Some("Plan".into()),
			chips: vec![
				CenterChip {
					action: CenterAction::PlanToggleFmt,
					label: format!("fmt{}", on(plan.run_formatter)),
					hint: Some('1'),
				},
				CenterChip {
					action: CenterAction::PlanToggleLint,
					label: format!("lint{}", on(plan.run_linter)),
					hint: Some('2'),
				},
				CenterChip {
					action: CenterAction::PlanToggleLsp,
					label: format!("lsp{}", on(plan.use_lsp)),
					hint: Some('3'),
				},
				CenterChip {
					action: CenterAction::PlanToggleVcs,
					label: format!("vcs{}", on(plan.use_vcs)),
					hint: Some('4'),
				},
				CenterChip {
					action: CenterAction::PlanToggleShell,
					label: format!("sh{}", on(plan.allow_shell)),
					hint: Some('5'),
				},
				CenterChip {
					action: CenterAction::PlanApproveWrite,
					label: "→ Write".into(),
					hint: Some('↵'),
				},
			],
		};
	}

	// 5) Profile-specific actions (only on the message screen)
	if show_actions {
		match mode {
			AgentMode::Write | AgentMode::Agent => {
				// Show paste/attachment chips instead of Run/Fmt/Lint
				let mut chips = Vec::new();
				for block in paste_blocks {
					let label = format!("📋{}", block.lines);
					chips.push(CenterChip { action: CenterAction::ShowPastePreview, label, hint: None });
				}
				for att in attachments {
					let sym = match att.kind {
						crate::input::AttachmentKind::File => "📄",
						crate::input::AttachmentKind::Folder => "📁",
						crate::input::AttachmentKind::Image => "🖼",
					};
					let name = att.path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
					let label = format!("{sym}{name}");
					chips.push(CenterChip { action: CenterAction::ShowPastePreview, label, hint: None });
				}
				if !chips.is_empty() {
					return CenterContent::Chips { label: None, chips };
				}
				// Fall through to tip if no pastes/attachments
			}
			AgentMode::Goal => {
				return CenterContent::Chips {
					label: Some("Goal idle".into()),
					chips: vec![CenterChip {
						action: CenterAction::ActionStartGoal,
						label: "set goal…".into(),
						hint: None,
					}],
				};
			}
			_ => {}
		}
	}

	// Fallback: rotating tip text for all other screens
	if fallback_tip.is_empty() {
		CenterContent::Text { text: mode.label().to_string(), style: CenterTextStyle::Muted }
	} else {
		CenterContent::Text { text: fallback_tip.to_string(), style: CenterTextStyle::Muted }
	}
}

/// Map permission chip → decision.
pub fn perm_decision(action: &CenterAction) -> Option<PermissionDecision> {
	match action {
		CenterAction::PermOnce => Some(PermissionDecision::AllowOnce),
		CenterAction::PermAlways => Some(PermissionDecision::AllowAlways),
		CenterAction::PermDeny => Some(PermissionDecision::Deny),
		_ => None,
	}
}

/// Format goal timer for the center bar: `⏱ 3:42/30:00 · 2/24`
pub fn goal_timer_line(
	elapsed: std::time::Duration,
	budget: std::time::Duration,
	iterations: u32,
	max_iterations: u32,
	paused: bool,
	completed: bool,
) -> String {
	fn mmss(d: std::time::Duration) -> String {
		let s = d.as_secs();
		format!("{}:{:02}", s / 60, s % 60)
	}
	if completed {
		return format!("✓ Goal done · {}", mmss(elapsed));
	}
	let flag = if paused { "⏸" } else { "⏱" };
	format!("{flag} {}/{} · {}/{}", mmss(elapsed), mmss(budget), iterations, max_iterations)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::goal_runner::PlanOptions;

	#[test]
	fn permission_wins_over_goal() {
		let plan = PlanOptions::default();
		let perm = PendingPermission {
			tool: "write".into(),
			preview: "src/a.rs".into(),
			requested_at: std::time::Instant::now(),
		};
		let c = build_center(
			AgentMode::Goal,
			true,
			Some("⏱ 1:00/30:00 · 1/24".into()),
			false,
			&plan,
			Some(&perm),
			None,
			"tip",
			true,
			&[],
			&[],
		);
		match c {
			CenterContent::Chips { chips, .. } => {
				assert!(chips.iter().any(|c| c.action == CenterAction::PermOnce));
			}
			_ => panic!("expected chips"),
		}
	}

	#[test]
	fn plan_shows_toggles() {
		let plan = PlanOptions { run_formatter: true, ..Default::default() };
		let c =
			build_center(AgentMode::Plan, false, None, false, &plan, None, None, "", true, &[], &[]);
		match c {
			CenterContent::Chips { chips, .. } => {
				assert!(chips.iter().any(|c| matches!(c.action, CenterAction::PlanApproveWrite)));
				assert!(chips.iter().any(|c| c.label.contains("fmt✓")));
			}
			_ => panic!("expected plan chips"),
		}
	}
}
