//! Goal mode: keep the agent running until the goal is complete (or time budget ends).

use std::time::{Duration, Instant};

use crate::modes::AgentMode;

#[derive(Debug, Clone)]
pub struct GoalState {
	pub active: bool,
	pub paused: bool,
	pub goal_text: String,
	pub started_at: Option<Instant>,
	/// Soft wall-clock budget (default 30 minutes).
	pub budget: Duration,
	/// Accumulated elapsed excluding paused periods.
	pub accrued: Duration,
	pub pause_started: Option<Instant>,
	pub iterations: u32,
	pub max_iterations: u32,
	pub completed: bool,
	pub last_status: String,
}

impl Default for GoalState {
	fn default() -> Self {
		Self {
			active: false,
			paused: false,
			goal_text: String::new(),
			started_at: None,
			budget: Duration::from_secs(30 * 60),
			accrued: Duration::ZERO,
			pause_started: None,
			iterations: 0,
			max_iterations: 24,
			completed: false,
			last_status: String::new(),
		}
	}
}

impl GoalState {
	pub fn start(&mut self, goal: impl Into<String>) {
		self.active = true;
		self.paused = false;
		self.goal_text = goal.into();
		self.started_at = Some(Instant::now());
		self.accrued = Duration::ZERO;
		self.pause_started = None;
		self.iterations = 0;
		self.completed = false;
		self.last_status = "Goal started".into();
	}

	pub fn stop(&mut self, reason: &str) {
		self.active = false;
		self.paused = false;
		self.completed = true;
		self.pause_started = None;
		self.last_status = reason.to_string();
	}

	pub fn pause(&mut self) {
		if self.active && !self.paused {
			self.paused = true;
			self.pause_started = Some(Instant::now());
			// Fold current running segment into accrued
			if let Some(start) = self.started_at {
				self.accrued += start.elapsed();
			}
			self.started_at = None;
			self.last_status = "paused".into();
		}
	}

	pub fn resume(&mut self) {
		if self.active && self.paused {
			self.paused = false;
			self.pause_started = None;
			self.started_at = Some(Instant::now());
			self.last_status = "resumed".into();
		}
	}

	/// Extend wall-clock budget by `extra` and optionally iterations.
	pub fn extend(&mut self, extra: Duration, more_iterations: u32) {
		self.budget += extra;
		self.max_iterations = self.max_iterations.saturating_add(more_iterations);
		if self.completed && !self.active {
			// Re-open if user extends after stop
			self.active = true;
			self.completed = false;
			self.paused = false;
			self.started_at = Some(Instant::now());
		}
		self.last_status =
			format!("extended +{}m · max_iter {}", extra.as_secs() / 60, self.max_iterations);
	}

	pub fn elapsed(&self) -> Duration {
		let mut total = self.accrued;
		if !self.paused
			&& let Some(start) = self.started_at
		{
			total += start.elapsed();
		}
		total
	}

	pub fn remaining(&self) -> Duration {
		self.budget.saturating_sub(self.elapsed())
	}

	pub fn timed_out(&self) -> bool {
		self.active && !self.paused && self.elapsed() >= self.budget
	}

	pub fn iteration_budget_hit(&self) -> bool {
		self.iterations >= self.max_iterations
	}

	pub fn tick_iteration(&mut self) {
		self.iterations = self.iterations.saturating_add(1);
		self.last_status = format!("iteration {}/{}", self.iterations, self.max_iterations);
	}

	/// Whether auto-continue is allowed (active, not paused, not done).
	pub fn can_continue(&self) -> bool {
		self.active && !self.paused && !self.completed
	}

	/// Heuristic: assistant claims done / checklist complete.
	pub fn detect_completion(text: &str) -> bool {
		let lower = text.to_ascii_lowercase();
		const MARKERS: &[&str] = &[
			"goal complete",
			"goal completed",
			"all tasks done",
			"all todos completed",
			"✅ done",
			"status: complete",
			"successfully completed the goal",
		];
		if MARKERS.iter().any(|m| lower.contains(m)) {
			return true;
		}
		// All checkboxes checked and none open
		let open = text.matches("- [ ]").count() + text.matches("* [ ]").count();
		let closed =
			text.matches("- [x]").count() + text.matches("- [X]").count() + text.matches("* [x]").count();
		closed > 0 && open == 0 && closed >= 2
	}

	pub fn status_line(&self) -> String {
		if !self.active && !self.completed {
			return "Goal: idle".into();
		}
		let mins = self.elapsed().as_secs() / 60;
		let rem = self.remaining().as_secs() / 60;
		if self.completed {
			return format!("Goal: done · {mins}m · {}", self.last_status);
		}
		if self.paused {
			return format!(
				"Goal: PAUSED · {mins}m · {}/{} · {}",
				self.iterations,
				self.max_iterations,
				self.goal_text.chars().take(28).collect::<String>()
			);
		}
		format!(
			"Goal: {}m elapsed · {rem}m left · {}/{} · {}",
			mins,
			self.iterations,
			self.max_iterations,
			self.goal_text.chars().take(32).collect::<String>()
		)
	}

	/// Compact bottom-bar center timer (OpenCode-style status surface).
	pub fn bar_timer_line(&self) -> String {
		crate::bottom_center::goal_timer_line(
			self.elapsed(),
			self.budget,
			self.iterations,
			self.max_iterations,
			self.paused,
			self.completed && !self.active,
		)
	}
}

/// Synthetic user prompt after Plan → Write approve (OpenCode plan_exit / build).
pub fn plan_approve_write_prompt(plan_summary: &str) -> String {
	format!(
		"The plan has been approved. Switch to implementation.\n\
		 Plan context: {plan_summary}\n\
		 Execute the plan: inspect the codebase, apply the changes with tools, \
		 and verify (fmt/lint/test when practical). Do not stop after only describing steps."
	)
}

/// Plan mode: optional folder + tool checklist for the composer.
#[derive(Debug, Clone, Default)]
pub struct PlanOptions {
	pub target_folder: Option<String>,
	pub run_formatter: bool,
	pub run_linter: bool,
	pub use_lsp: bool,
	pub use_vcs: bool,
	pub allow_shell: bool,
}

impl PlanOptions {
	pub fn prompt_prefix(&self) -> String {
		let mut parts = vec!["[mode:plan]".to_string()];
		if let Some(dir) = &self.target_folder {
			parts.push(format!("[cwd:{dir}]"));
		}
		if self.run_formatter {
			parts.push("[tools:formatter]".into());
		}
		if self.run_linter {
			parts.push("[tools:linter]".into());
		}
		if self.use_lsp {
			parts.push("[tools:lsp]".into());
		}
		if self.use_vcs {
			parts.push("[tools:vcs]".into());
		}
		if self.allow_shell {
			parts.push("[tools:shell]".into());
		}
		parts.join(" ")
	}

	pub fn summary(&self) -> String {
		format!(
			"folder={} fmt={} lint={} lsp={} vcs={} shell={}",
			self.target_folder.as_deref().unwrap_or("."),
			self.run_formatter,
			self.run_linter,
			self.use_lsp,
			self.use_vcs,
			self.allow_shell
		)
	}
}

/// Build the next goal continuation prompt for multi-turn goal runs.
pub fn goal_continuation_prompt(goal: &GoalState) -> String {
	format!(
		"[mode:goal] Continue working on the goal until it is complete.\n\
		 Goal: {}\n\
		 Iteration: {}/{}\n\
		 Elapsed: {}s · Budget: {}s\n\
		 When finished, mark all tasks [x] and write \"Goal complete\".",
		goal.goal_text,
		goal.iterations,
		goal.max_iterations,
		goal.elapsed().as_secs(),
		goal.budget.as_secs(),
	)
}

#[allow(dead_code)]
pub fn mode_needs_plan_options(mode: AgentMode) -> bool {
	mode == AgentMode::Plan
}

/// Run plan checklist tools (fmt/lint/lsp/vcs) and return a report for the prompt context.
pub fn run_plan_tools(opts: &PlanOptions) -> String {
	use crate::workspace_tools::{collect_diagnostics, collect_vcs, run_formatter, run_linter};
	use std::path::PathBuf;

	let cwd = PathBuf::from(opts.target_folder.clone().unwrap_or_else(|| {
		std::env::current_dir().map(|p| p.display().to_string()).unwrap_or_else(|_| ".".into())
	}));
	let mut parts = Vec::new();
	parts.push(format!("Plan workspace: {}", cwd.display()));

	if opts.run_formatter {
		let r = run_formatter(&cwd);
		parts.push(format!("formatter: {}", r.summary));
		if !r.ok {
			let body = r.body();
			parts.push(body.chars().take(800).collect());
		}
	}
	if opts.run_linter {
		let r = run_linter(&cwd);
		parts.push(format!("linter: {}", r.summary));
		if !r.ok {
			let body = r.body();
			parts.push(body.chars().take(800).collect());
		}
	}
	if opts.use_lsp {
		let (diags, sum) = collect_diagnostics(&cwd);
		parts.push(format!("lsp/diagnostics: {sum}"));
		for d in diags.iter().take(12) {
			parts.push(format!(
				"  {} {}:{}:{} {}",
				d.severity.glyph(),
				d.path,
				d.line,
				d.col,
				d.message.chars().take(80).collect::<String>()
			));
		}
	}
	if opts.use_vcs {
		let v = collect_vcs(&cwd);
		parts.push(format!("vcs: {}", v.summary));
		if !v.last_commit.is_empty() {
			parts.push(format!("  HEAD {}", v.last_commit));
		}
		for s in v.short_status.iter().take(10) {
			parts.push(format!("  {s}"));
		}
	}
	if opts.allow_shell {
		parts.push("shell: allowed in plan context".into());
	} else {
		parts.push("shell: denied".into());
	}
	parts.join("\n")
}
