//! DX system-prompt stack (OpenCode-shaped, token-efficient).
//!
//! Layers (joined once per request as `role=system`):
//! 1. Base identity (DX brand)
//! 2. Profile policy (Ask / Write / Plan / Goal / Agent)
//! 3. Environment (cwd, git, platform, date, model)
//! 4. Project instructions (AGENTS.md, capped)
//! 5. First-turn session meta (title + todos) — only on first user turn
//!
//! Brand: **DX** (never "opencode" in product strings).

use std::{
	path::{Path, PathBuf},
	process::Command,
};

use crate::modes::AgentMode;

/// Caps keep the stack production-ready without blowing context.
const AGENTS_CAP: usize = 4_000;
const SYSTEM_HARD_CAP: usize = 12_000;

/// Inputs for assembling the system stack.
#[derive(Debug, Clone)]
pub struct SystemContext<'a> {
	pub mode: AgentMode,
	pub model_id: &'a str,
	pub model_display: &'a str,
	pub project_dir: &'a str,
	pub first_turn: bool,
	/// Optional compact plan/workspace signals (already truncated by caller).
	pub workspace_signals: Option<&'a str>,
}

/// Full system string for OpenAI-compatible `role=system`.
pub fn build_system(ctx: &SystemContext<'_>) -> String {
	use crate::agent_workspace::{BootstrapScope, build_bootstrap_prompt};

	let mut parts = Vec::with_capacity(8);
	// Identity: prefer SOUL/IDENTITY from workspace for Agent-like profiles
	let scope = BootstrapScope::for_mode(ctx.mode);
	if let Some(boot) = build_bootstrap_prompt(scope) {
		// Soul/identity first (Hermes identity slot)
		parts.push(boot);
	} else {
		parts.push(base_prompt().to_string());
	}
	// Always keep DX tool/workflow base for non-soul identity
	if scope != BootstrapScope::Full {
		parts.push(base_prompt().to_string());
	} else {
		// Full bootstrap already has soul — still inject compact tools workflow
		parts.push(base_tools_layer().to_string());
	}
	parts.push(profile_layer(ctx.mode));
	if matches!(ctx.mode, AgentMode::Agent | AgentMode::Goal) {
		parts.push(crate::orchestration::orchestration_guidance().to_string());
		parts.push(crate::skills::skills_guidance().to_string());
		if let Some(idx) = crate::skills::skills_index_prompt(24) {
			parts.push(idx);
		}
	}
	parts.push(environment_layer(ctx));
	if let Some(instr) = project_instructions(ctx.project_dir) {
		parts.push(instr);
	}
	if let Some(sig) = ctx.workspace_signals {
		let s = sig.trim();
		if !s.is_empty() {
			parts.push(format!("<workspace_signals>\n{}\n</workspace_signals>", truncate(s, 1_500)));
		}
	}
	if ctx.first_turn {
		parts.push(first_turn_layer().to_string());
	}

	let mut out = parts.join("\n\n");
	if out.chars().count() > SYSTEM_HARD_CAP {
		out = truncate(&out, SYSTEM_HARD_CAP);
		out.push_str("\n…[system truncated]");
	}
	out
}

fn base_tools_layer() -> &'static str {
	r#"# DX runtime
Tools (runtime executes them): shell, read, write, edit, glob, grep, list, task, question.
Keep using tools until the task is done. Prefer minimal diffs. Never invent file contents."#
}

/// Prefix for agents that only accept a single user string (CLI / some runtimes).
pub fn as_agent_prefix(system: &str, user: &str) -> String {
	format!("[system]\n{system}\n\n[user]\n{user}")
}

// ── Layers ──────────────────────────────────────────────────────────────

/// Token-efficient DX base (OpenCode-shaped duties, shorter text).
fn base_prompt() -> &'static str {
	// Keep under ~1.2k bytes so system stack stays lean.
	r#"You are DX, an interactive CLI coding agent for software engineering.

Rules:
- Be concise and direct. Prefer short answers unless detail is requested.
- Use tools to inspect/change code; do not invent file contents or URLs.
- Follow project conventions; check imports before adding libraries.
- Never commit unless asked. Never expose secrets. Prefer minimal diffs.
- Tool results may include noise; focus on signal.

Tools (runtime executes them — do not only print commands):
shell(command), read(path), write(path,content), edit(path,old_string,new_string),
glob(pattern), grep(pattern), list(path).

If the tools API is unavailable, emit executable forms the runtime runs immediately:
```bash
actual-command-here
```
or: <shell command="actual-command-here"/>
Do NOT only describe commands — emit them so the runtime executes, then continue
with more tools until done, then a short final answer.

Workflow: understand → tools → verify → concise answer."#
}

fn profile_layer(mode: AgentMode) -> String {
	let (approval, sandbox) = crate::profile_prompts::profile_policy(mode);
	let reasoning_guide = "When reasoning_effort is high, show your step-by-step analysis inside \
		<think> tags before answering. For code changes, explain the approach first, \
		then implement. For factual answers, keep thinking brief.";
	let policy = match mode {
		AgentMode::Ask => {
			"Mode: Ask — read-only. Use read/glob/grep/list tools. Do not write files or run destructive shell."
		}
		AgentMode::Write => {
			"Mode: Write — implement with tools (shell/read/edit/write). Verify when practical. Multi-step until done."
		}
		AgentMode::Plan => {
			"Mode: Plan — research with read tools if needed; produce an ordered plan with phases and \
			 a checkbox task list. Do not edit production code until the user approves (→ Write). \
			 End with a clear checklist the build agent can execute."
		}
		AgentMode::Goal => {
			"Mode: Goal — task-driven: keep using tools every iteration until the goal is fully done. \
			 Maintain `- [ ]`/`- [x]` tasks; prefer small verifiable steps; never stop after only \
			 proposing commands. Finish with \"Goal complete\" when all tasks are [x]."
		}
		AgentMode::Agent => {
			"Mode: Agent — full tools + multi-agent orchestration (task tool). \
			 Load SOUL/IDENTITY voice; edit, run commands, delegate explore/general-purpose subagents; \
			 multi-step until complete; safe defaults on destructive ops."
		}
		AgentMode::Multi => {
			"Mode: Multi — concurrent. You are one of multiple agents running in parallel. \
			 Use read-only tools (read/grep/glob/list/shell status) as needed, then answer. \
			 Do not spawn nested task/subagents. Keep the final answer concise."
		}
		AgentMode::Automation => {
			"Mode: Automation — scheduled. You are triggered by a timer or daily schedule. \
			 Run status checks, fix issues, edit files, and complete your goal autonomously. \
			 Be concise. This run will repeat automatically on the configured interval."
		}
		AgentMode::Codex => {
			"Mode: Codex — managed by codex app-server. The app-server handles model \
			 interaction, tool execution, and permissions."
		}
	};
	format!("{policy}\nApproval: {approval}. Sandbox: {sandbox}.\n{reasoning_guide}")
}

fn environment_layer(ctx: &SystemContext<'_>) -> String {
	let cwd = PathBuf::from(ctx.project_dir);
	let cwd_disp = cwd.display().to_string();
	let git = is_git_repo(&cwd);
	let platform = std::env::consts::OS;
	let date = chrono::Local::now().format("%Y-%m-%d").to_string();
	format!(
		"<env>\n\
		 model: {} ({})\n\
		 cwd: {cwd_disp}\n\
		 git: {}\n\
		 platform: {platform}\n\
		 date: {date}\n\
		</env>",
		ctx.model_display,
		ctx.model_id,
		if git { "yes" } else { "no" },
	)
}

/// First-turn only: session title + optional todos (DX meta).
fn first_turn_layer() -> &'static str {
	r#"<session_meta>
CRITICAL — before any other visible text (after optional thinking), emit exactly one line:
TITLE: <long descriptive chat name that fills at least 3 sidebar lines>
Example:
TITLE: Diagnose and fix the Windows login timeout that blocks users after password reset and breaks the streaming chat response path

Rules:
- TITLE is a long, human chat name — NOT a copy of the user prompt and NOT one or two words.
- Write about 14–28 words / roughly 90–180 characters so it wraps to at least 3 lines in a ~40-column sidebar.
- Capture goal + subject + context + stakes (what, where, why).
- Good: "Investigate Cargo test failures in the Windows CI pipeline and propose a durable fix for flaky integration suites"
- Bad: "Auth", "Bug fix", "Help", "Tests", or pasting the user's message back.
- Do not wrap TITLE in markdown code fences.
- Then answer the user normally. Optional: up to 8 `- [ ]` todos for multi-step work.
- Never skip TITLE on the first reply. Do not emit TITLE again later.
</session_meta>"#
}

fn project_instructions(project_dir: &str) -> Option<String> {
	let text = load_agents_md(Some(project_dir))?;
	let body = truncate(text.trim(), AGENTS_CAP);
	Some(format!("<project_instructions>\n{body}\n</project_instructions>"))
}

/// Nearest AGENTS.md / CLAUDE.md walking up from cwd.
pub fn load_agents_md(cwd: Option<&str>) -> Option<String> {
	let start = cwd
		.map(PathBuf::from)
		.or_else(|| std::env::current_dir().ok())
		.unwrap_or_else(|| PathBuf::from("."));

	let mut dir = start;
	for _ in 0..12 {
		for name in ["AGENTS.md", "Agents.md", "agents.md", "CLAUDE.md"] {
			let p = dir.join(name);
			if p.is_file()
				&& let Ok(text) = std::fs::read_to_string(&p)
			{
				let trimmed = text.trim();
				if !trimmed.is_empty() {
					return Some(trimmed.to_string());
				}
			}
		}
		if !dir.pop() {
			break;
		}
	}
	None
}

fn is_git_repo(cwd: &Path) -> bool {
	Command::new("git")
		.args(["rev-parse", "--is-inside-work-tree"])
		.current_dir(cwd)
		.output()
		.map(|o| o.status.success())
		.unwrap_or(false)
}

fn truncate(s: &str, max_chars: usize) -> String {
	let count = s.chars().count();
	if count <= max_chars {
		return s.to_string();
	}
	let head: String = s.chars().take(max_chars.saturating_sub(1)).collect();
	format!("{head}…")
}

/// Rough char estimate for telemetry / status.
#[allow(dead_code)]
pub fn estimate_system_chars(ctx: &SystemContext<'_>) -> usize {
	build_system(ctx).chars().count()
}

#[cfg(test)]
mod tests {
	use super::*;

	fn sample_ctx(first: bool) -> SystemContext<'static> {
		SystemContext {
			mode: AgentMode::Write,
			model_id: "big-pickle",
			model_display: "Big Pickle",
			project_dir: ".",
			first_turn: first,
			workspace_signals: None,
		}
	}

	#[test]
	fn brand_is_dx_not_opencode() {
		let s = build_system(&sample_ctx(true));
		assert!(s.contains("You are DX"));
		assert!(!s.to_ascii_lowercase().contains("opencode"));
		assert!(s.contains("TITLE:"));
	}

	#[test]
	fn first_turn_meta_only_when_flagged() {
		let a = build_system(&sample_ctx(true));
		let b = build_system(&sample_ctx(false));
		assert!(a.contains("session_meta"));
		assert!(!b.contains("session_meta"));
	}

	#[test]
	fn system_stays_bounded() {
		let s = build_system(&sample_ctx(true));
		assert!(s.chars().count() < SYSTEM_HARD_CAP);
		// Base should be far under an 8k dump.
		assert!(base_prompt().len() < 1_200);
	}

	#[test]
	fn agent_prefix_wraps() {
		let p = as_agent_prefix("SYS", "hello");
		assert!(p.contains("[system]"));
		assert!(p.contains("hello"));
	}

	#[test]
	fn profile_layers_differ() {
		let mut ask = sample_ctx(false);
		ask.mode = AgentMode::Ask;
		let mut write = sample_ctx(false);
		write.mode = AgentMode::Write;
		assert!(build_system(&ask).contains("Ask"));
		assert!(build_system(&write).contains("Write"));
	}
}
