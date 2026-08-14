//! OpenCode-compatible slash commands for the DX TUI.
//!
//! Type `/` in the prompt for autocomplete. Submit runs
//! [`ChatState::handle_slash_command`].

use std::{
	path::{Path, PathBuf},
	sync::Arc,
};

use crate::{
	bridge::AppMode,
	components::{Message, MessageRole},
	modes::{AgentMode, RuntimeMode},
	state::{BottomPopup, ChatState, CommandDialog, ThinkingVisibility},
	theme::ChatTheme,
};

/// One slash command entry for autocomplete + docs.
#[derive(Debug, Clone, Copy)]
pub struct SlashSpec {
	/// Canonical name including leading `/` (e.g. `/sessions`).
	pub name: &'static str,
	/// Alternate spellings the user may type.
	pub aliases: &'static [&'static str],
	pub description: &'static str,
	/// When false, still listed but may no-op with a toast if gated.
	pub always_visible: bool,
}

/// Full OpenCode-style command surface (hardcoded + server-side builtins).
pub const SLASH_COMMANDS: &[SlashSpec] = &[
	// ── Navigation ──────────────────────────────────────────
	SlashSpec {
		name: "/home",
		aliases: &["/splash"],
		description: "Return to splash / home screen",
		always_visible: true,
	},
	SlashSpec {
		name: "/files",
		aliases: &["/fb", "/filebrowser", "/browse"],
		description: "Open file browser",
		always_visible: true,
	},
	// ── Session ──────────────────────────────────────────────
	SlashSpec {
		name: "/sessions",
		aliases: &["/resume", "/continue"],
		description: "Switch session (session list)",
		always_visible: true,
	},
	SlashSpec {
		name: "/new",
		aliases: &["/clear"],
		description: "Create a new session",
		always_visible: true,
	},
	SlashSpec {
		name: "/share",
		aliases: &[],
		description: "Share session / copy share link",
		always_visible: true,
	},
	SlashSpec {
		name: "/unshare",
		aliases: &[],
		description: "Unshare current session",
		always_visible: true,
	},
	SlashSpec {
		name: "/rename",
		aliases: &[],
		description: "Rename current session",
		always_visible: true,
	},
	SlashSpec {
		name: "/name",
		aliases: &["/whoami", "/me"],
		description: "Set your display name (default: You)",
		always_visible: true,
	},
	SlashSpec {
		name: "/timeline",
		aliases: &[],
		description: "Jump to a message in this session",
		always_visible: true,
	},
	SlashSpec {
		name: "/fork",
		aliases: &[],
		description: "Fork session from a message",
		always_visible: true,
	},
	SlashSpec {
		name: "/compact",
		aliases: &["/summarize"],
		description: "Compact/summarize session context",
		always_visible: true,
	},
	SlashSpec {
		name: "/undo",
		aliases: &[],
		description: "Undo last user exchange",
		always_visible: true,
	},
	SlashSpec { name: "/redo", aliases: &[], description: "Redo last undo", always_visible: true },
	SlashSpec {
		name: "/copy",
		aliases: &[],
		description: "Copy full transcript to clipboard",
		always_visible: true,
	},
	SlashSpec {
		name: "/copy-response",
		aliases: &["/copy-last"],
		description: "Copy last assistant response to clipboard",
		always_visible: true,
	},
	SlashSpec {
		name: "/workspace",
		aliases: &["/soul", "/agent-workspace"],
		description: "Show agent workspace path + bootstrap files (SOUL.md…)",
		always_visible: true,
	},
	SlashSpec {
		name: "/update-check",
		aliases: &["/update"],
		description: "Check for DX TUI updates (Hermes-style cache)",
		always_visible: true,
	},
	SlashSpec {
		name: "/delegations",
		aliases: &["/tasks-ledger"],
		description: "Show multi-agent delegation ledger",
		always_visible: true,
	},
	SlashSpec {
		name: "/skills",
		aliases: &["/skill"],
		description: "List auto-learned skills (Hermes-style library)",
		always_visible: true,
	},
	SlashSpec {
		name: "/export",
		aliases: &[],
		description: "Export transcript to a file",
		always_visible: true,
	},
	SlashSpec {
		name: "/move",
		aliases: &[],
		description: "Move session to another project directory",
		always_visible: true,
	},
	// ── Display ──────────────────────────────────────────────
	SlashSpec {
		name: "/timestamps",
		aliases: &["/toggle-timestamps"],
		description: "Toggle message timestamps",
		always_visible: true,
	},
	SlashSpec {
		name: "/thinking",
		aliases: &["/toggle-thinking"],
		description: "Cycle thinking block visibility",
		always_visible: true,
	},
	// ── Agent / model ────────────────────────────────────────
	SlashSpec {
		name: "/models",
		aliases: &["/mo", "/model"],
		description: "Switch model",
		always_visible: true,
	},
	SlashSpec {
		name: "/agents",
		aliases: &["/agent"],
		description: "Switch agent mode (Ask/Write/Plan/Goal)",
		always_visible: true,
	},
	SlashSpec {
		name: "/mcps",
		aliases: &["/mcp"],
		description: "MCP servers (probe + list)",
		always_visible: true,
	},
	SlashSpec {
		name: "/variants",
		aliases: &[],
		description: "Switch model variant",
		always_visible: true,
	},
	// ── Provider ─────────────────────────────────────────────
	SlashSpec {
		name: "/providers",
		aliases: &[],
		description: "Browse and connect AI providers",
		always_visible: true,
	},
	SlashSpec {
		name: "/org",
		aliases: &["/orgs", "/switch-org"],
		description: "Switch organization",
		always_visible: true,
	},
	// ── Prompt / editor ──────────────────────────────────────
	SlashSpec {
		name: "/editor",
		aliases: &[],
		description: "Open prompt in external editor",
		always_visible: true,
	},
	SlashSpec { name: "/skills", aliases: &[], description: "Browse skills", always_visible: true },
	// ── Workspace ────────────────────────────────────────────
	SlashSpec {
		name: "/workspaces",
		aliases: &[],
		description: "Manage workspaces (experimental)",
		always_visible: true,
	},
	SlashSpec {
		name: "/warp",
		aliases: &[],
		description: "Change workspace for session",
		always_visible: true,
	},
	// ── VCS ──────────────────────────────────────────────────
	SlashSpec {
		name: "/diff",
		aliases: &[],
		description: "Open full-screen differ",
		always_visible: true,
	},
	// ── System ───────────────────────────────────────────────
	SlashSpec {
		name: "/status",
		aliases: &[],
		description: "System / session status",
		always_visible: true,
	},
	SlashSpec {
		name: "/debug",
		aliases: &[],
		description: "Debug information",
		always_visible: true,
	},
	SlashSpec {
		name: "/themes",
		aliases: &["/theme"],
		description: "Switch UI theme",
		always_visible: true,
	},
	SlashSpec {
		name: "/help",
		aliases: &[],
		description: "Help and shortcuts",
		always_visible: true,
	},
	SlashSpec {
		name: "/exit",
		aliases: &["/quit", "/q"],
		description: "Exit DX",
		always_visible: true,
	},
	// ── Mode shortcuts (DX extras) ───────────────────────────
	SlashSpec { name: "/ask", aliases: &[], description: "Ask mode", always_visible: true },
	SlashSpec { name: "/write", aliases: &[], description: "Write mode", always_visible: true },
	SlashSpec { name: "/plan", aliases: &[], description: "Plan mode", always_visible: true },
	SlashSpec { name: "/goal", aliases: &[], description: "Goal mode", always_visible: true },
	SlashSpec {
		name: "/agent",
		aliases: &[],
		description: "Full dx-agent tool profile",
		always_visible: true,
	},
	SlashSpec {
		name: "/codex",
		aliases: &[],
		description: "Switch to Codex mode (app-server backend)",
		always_visible: true,
	},
	SlashSpec {
		name: "/codex-connect",
		aliases: &["/codex-start"],
		description: "In-process codex mode (no URL needed)",
		always_visible: true,
	},
	SlashSpec {
		name: "/codex-resume",
		aliases: &[],
		description: "Resume a codex thread by ID",
		always_visible: true,
	},
	SlashSpec {
		name: "/codex-fork",
		aliases: &[],
		description: "Fork a codex thread by ID",
		always_visible: true,
	},
	SlashSpec {
		name: "/voice",
		aliases: &["/stt", "/tts"],
		description: "STT / TTS panel (dx-flow)",
		always_visible: true,
	},
	SlashSpec {
		name: "/share-channel",
		aliases: &[],
		description: "Share session to a social channel",
		always_visible: true,
	},
	SlashSpec {
		name: "/channels-start",
		aliases: &["/gateway-start"],
		description: "Start dx-agent channel gateway",
		always_visible: true,
	},
	SlashSpec {
		name: "/channels-stop",
		aliases: &["/gateway-stop"],
		description: "Stop dx-agent channel gateway",
		always_visible: true,
	},
	SlashSpec {
		name: "/channel-doctor",
		aliases: &["/doctor-channels"],
		description: "Channel + gateway health check",
		always_visible: true,
	},
	SlashSpec {
		name: "/goal-pause",
		aliases: &[],
		description: "Pause goal auto-continue",
		always_visible: true,
	},
	SlashSpec {
		name: "/goal-resume",
		aliases: &[],
		description: "Resume paused goal",
		always_visible: true,
	},
	SlashSpec {
		name: "/goal-extend",
		aliases: &[],
		description: "Extend goal budget (+15m +4 iters)",
		always_visible: true,
	},
	SlashSpec {
		name: "/bind-channel",
		aliases: &[],
		description: "Bind session to a channel thread",
		always_visible: true,
	},
	SlashSpec {
		name: "/plan-run",
		aliases: &[],
		description: "Run plan fmt/lint/LSP probes and attach",
		always_visible: true,
	},
	SlashSpec {
		name: "/flow-warm",
		aliases: &[],
		description: "Warm dx-flow local text model",
		always_visible: true,
	},
	SlashSpec {
		name: "/flow-unload",
		aliases: &[],
		description: "Unload dx-flow local model",
		always_visible: true,
	},
	SlashSpec {
		name: "/tasks",
		aliases: &["/todo"],
		description: "List sidebar tasks (cycle: /tasks N)",
		always_visible: true,
	},
	SlashSpec {
		name: "/fmt",
		aliases: &["/format"],
		description: "Run formatter (check; /fmt apply writes)",
		always_visible: true,
	},
	SlashSpec {
		name: "/lint",
		aliases: &[],
		description: "Run project linter (clippy/eslint/ruff)",
		always_visible: true,
	},
	SlashSpec {
		name: "/lsp",
		aliases: &["/diagnostics", "/diag"],
		description: "Collect LSP-quality diagnostics",
		always_visible: true,
	},
	SlashSpec {
		name: "/vcs",
		aliases: &["/git"],
		description: "Git / VCS status in sidebar + chat",
		always_visible: true,
	},
	SlashSpec {
		name: "/subagents",
		aliases: &["/agents-list"],
		description: "List subagents from this session",
		always_visible: true,
	},
	SlashSpec {
		name: "/doctor",
		aliases: &["/workspace"],
		description: "Full workspace doctor (fmt/lint/lsp/vcs)",
		always_visible: true,
	},
	// ── Server-side builtins ─────────────────────────────────
	SlashSpec {
		name: "/init",
		aliases: &[],
		description: "Guided AGENTS.md setup",
		always_visible: true,
	},
	SlashSpec {
		name: "/review",
		aliases: &[],
		description: "Review changes (commit|branch|pr)",
		always_visible: true,
	},
	// ── CLI-style ────────────────────────────────────────────
	SlashSpec { name: ":q", aliases: &[], description: "Vim-style exit", always_visible: true },
];

/// Autocomplete pairs `(command, description)` including aliases as separate rows.
pub fn autocomplete_pairs() -> Vec<(&'static str, &'static str)> {
	let mut out = Vec::with_capacity(SLASH_COMMANDS.len() * 2);
	for spec in SLASH_COMMANDS {
		if !spec.always_visible {
			continue;
		}
		out.push((spec.name, spec.description));
		for alias in spec.aliases {
			out.push((*alias, spec.description));
		}
	}
	out
}

/// Resolve raw input (e.g. `/resume foo`) to canonical command + rest args.
pub fn resolve(input: &str) -> Option<ResolvedCommand<'_>> {
	let trimmed = input.trim();
	if trimmed.is_empty() {
		return None;
	}
	// Allow `:q` without requiring leading `/`
	let (cmd_token, args) = if trimmed == ":q" || trimmed.starts_with(":q ") {
		(":q", trimmed.strip_prefix(":q").unwrap_or("").trim())
	} else if trimmed.starts_with('/') {
		let mut parts = trimmed.splitn(2, char::is_whitespace);
		let cmd = parts.next()?.trim();
		let rest = parts.next().unwrap_or("").trim();
		(cmd, rest)
	} else {
		return None;
	};

	let cmd_lower = cmd_token.to_ascii_lowercase();
	for spec in SLASH_COMMANDS {
		if spec.name.eq_ignore_ascii_case(&cmd_lower)
			|| spec.aliases.iter().any(|a| a.eq_ignore_ascii_case(&cmd_lower))
		{
			return Some(ResolvedCommand { canonical: spec.name, args });
		}
	}
	None
}

#[derive(Debug, Clone, Copy)]
pub struct ResolvedCommand<'a> {
	pub canonical: &'static str,
	pub args: &'a str,
}

/// Result of handling a slash command (does not send a chat message).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SlashResult {
	/// Handled successfully; optional toast already set by handler.
	Handled,
	/// Request application exit.
	Exit,
	/// Unknown slash command — show toast, do not chat.
	Unknown(String),
	/// Switch the app to the given mode.
	SwitchMode(AppMode),
}

impl ChatState {
	/// If `input` is a slash (or `:q`) command, run it and return Some.
	/// Returns None when this is a normal user message.
	pub fn try_handle_slash(&mut self, input: &str) -> Option<SlashResult> {
		let trimmed = input.trim();
		if !(trimmed.starts_with('/') || trimmed == ":q" || trimmed.starts_with(":q ")) {
			return None;
		}
		Some(self.handle_slash_command(trimmed))
	}

	pub fn handle_slash_command(&mut self, input: &str) -> SlashResult {
		let Some(resolved) = resolve(input) else {
			let name = input.split_whitespace().next().unwrap_or(input);
			self.show_toast(format!("Unknown command: {name}  (try /help)"));
			return SlashResult::Unknown(name.to_string());
		};

		// Exit any splash/animation so dialogs and UI feedback are visible
		self.animation.animation_mode = false;

		match resolved.canonical {
			// ── Navigation ──────────────────────────────────
			"/home" | "/splash" => {
				self.animation.animation_mode = true;
				self.animation.current_animation_index = 0;
				self.restart_current_animation();
				self.close_dialog();
				self.show_toast("Home — splash screen".into());
				SlashResult::Handled
			}
			"/files" | "/fb" | "/filebrowser" | "/browse" => {
				self.show_toast("File browser — ←/→ navigate · Esc back".into());
				SlashResult::SwitchMode(AppMode::FilePicker)
			}
			// ── Session ──────────────────────────────────────
			"/sessions" => {
				self.persist_current_session();
				self.ui.dialog = CommandDialog::Sessions;
				self.ui.dialog_cursor = 0;
				self.show_toast("Sessions — Enter switch · n new · Esc close".into());
				SlashResult::Handled
			}
			"/new" => {
				self.cmd_new_session();
				SlashResult::Handled
			}
			"/share" => {
				self.cmd_share();
				SlashResult::Handled
			}
			"/unshare" => {
				self.cmd_unshare();
				SlashResult::Handled
			}
			"/rename" => {
				self.ui.dialog = CommandDialog::Rename;
				self.ui.dialog_input = self.session.session_name.clone();
				self.ui.dialog_cursor = 0;
				self.show_toast("Rename session — type name, Enter save, Esc cancel".into());
				SlashResult::Handled
			}
			"/name" | "/whoami" | "/me" => {
				if !resolved.args.is_empty() {
					self.set_user_display_name(resolved.args);
				} else {
					self.ui.dialog = CommandDialog::UserName;
					self.ui.dialog_input = self.user_display_name.clone();
					self.ui.dialog_cursor = 0;
					self.show_toast("Your name — type, Enter save, Esc cancel".into());
				}
				SlashResult::Handled
			}
			"/timeline" => {
				self.ui.dialog = CommandDialog::Timeline;
				self.ui.dialog_cursor = 0;
				self.show_toast("Timeline — ↑/↓ · Enter jump · Esc close".into());
				SlashResult::Handled
			}
			"/fork" => {
				if resolved.args.is_empty() {
					self.ui.dialog = CommandDialog::Fork;
					self.ui.dialog_cursor = 0;
					self.show_toast("Fork — select message · Enter fork · Esc cancel".into());
				} else if let Ok(idx) = resolved.args.parse::<usize>() {
					self.cmd_fork_at(idx);
				} else {
					self.show_toast("Usage: /fork [message_index]".into());
				}
				SlashResult::Handled
			}
			"/compact" => {
				self.cmd_compact();
				SlashResult::Handled
			}
			// Note: aliases already resolve to /compact
			"/undo" => {
				self.cmd_undo();
				SlashResult::Handled
			}
			"/redo" => {
				self.cmd_redo();
				SlashResult::Handled
			}
			"/copy" => {
				self.cmd_copy_transcript();
				SlashResult::Handled
			}
			"/copy-response" | "/copy-last" => {
				self.copy_last_assistant_response();
				SlashResult::Handled
			}
			"/workspace" | "/soul" | "/agent-workspace" => {
				let root = crate::agent_workspace::ensure_workspace();
				let doctor = crate::agent_workspace::doctor_line();
				let soul = crate::agent_workspace::read_workspace_file("SOUL.md")
					.map(|s| s.chars().take(200).collect::<String>())
					.unwrap_or_else(|| "(missing)".into());
				self.messages.push(crate::components::Message::assistant(format!(
					"## Agent workspace\n\n\
					 {doctor}\n\n\
					 Path: `{}`\n\n\
					 Bootstrap files: SOUL.md · IDENTITY.md · USER.md · TOOLS.md · \
					 HEARTBEAT.md · MEMORY.md · AGENTS.md\n\n\
					 ### SOUL.md (preview)\n```\n{soul}\n```\n\n\
					 Edit files on disk; Agent profile reloads them into the system stack.",
					root.display()
				)));
				self.show_toast(format!("Workspace · {}", root.display()));
				SlashResult::Handled
			}
			"/update-check" | "/update" => {
				let st = crate::update_check::check_for_updates();
				self.update_status_line = Some(st.message.clone());
				self.show_toast(st.message);
				SlashResult::Handled
			}
			"/delegations" | "/tasks-ledger" => {
				let rem = self.delegation_ledger.reminder();
				let body = if rem.is_empty() {
					"No delegations this session.\n\n\
					 In **Agent** mode, use the `task` tool with subagent_type \
					 `explore` | `general-purpose` | `orchestrator`."
						.to_string()
				} else {
					rem
				};
				self
					.messages
					.push(crate::components::Message::assistant(format!("## Delegation ledger\n\n{body}")));
				SlashResult::Handled
			}
			"/skills" | "/skill" => {
				let list = crate::skills::list_skills();
				let dir = crate::skills::skills_dir();
				let body = if list.is_empty() {
					format!(
						"No skills yet under `{}`.\n\n\
						 After successful multi-step work (5+ tools), DX auto-saves a skill.\n\
						 Or in **Agent** mode call `skill_manage` action=create.",
						dir.display()
					)
				} else {
					let rows = list
						.iter()
						.map(|s| format!("- **{}** — {}", s.name, s.description))
						.collect::<Vec<_>>()
						.join("\n");
					format!(
						"Skills library (`{}`):\n\n{rows}\n\n\
						 Use skill_manage action=view name=<slug> in Agent mode.",
						dir.display()
					)
				};
				self.messages.push(crate::components::Message::assistant(format!("## Skills\n\n{body}")));
				SlashResult::Handled
			}
			"/export" => {
				let default = format!("session-{}.md", short_id(&self.session.chat_id));
				self.ui.dialog = CommandDialog::Export;
				self.ui.dialog_input = default;
				self.ui.dialog_cursor = 0;
				self.ui.export_include_thinking = true;
				self.ui.export_include_tools = true;
				self.show_toast("Export — edit filename · Enter save · Tab options · Esc".into());
				SlashResult::Handled
			}
			"/move" => {
				self.ui.dialog = CommandDialog::Move;
				self.ui.dialog_input = self.session.session_project_dir.clone();
				self.show_toast("Move session — enter project path · Enter · Esc".into());
				SlashResult::Handled
			}

			// ── Display ──────────────────────────────────────
			"/timestamps" => {
				self.ui.show_timestamps = !self.ui.show_timestamps;
				self.show_toast(if self.ui.show_timestamps {
					"Timestamps: shown".into()
				} else {
					"Timestamps: hidden".into()
				});
				SlashResult::Handled
			}
			"/thinking" => {
				self.thinking_visibility = self.thinking_visibility.cycle();
				self.apply_thinking_visibility();
				self.show_toast(format!("Thinking: {}", self.thinking_visibility.label()));
				SlashResult::Handled
			}

			// ── Agent / model ────────────────────────────────
			"/models" => {
				self.open_popup(BottomPopup::Models);
				SlashResult::Handled
			}
			"/agents" => {
				self.open_popup(BottomPopup::AgentMode);
				SlashResult::Handled
			}
			"/mcps" => {
				self.sidebar.refresh();
				let snap = self.sidebar.snapshot();
				let connected = snap.mcp.iter().filter(|m| m.connected).count();
				self.ui.dialog = CommandDialog::Mcps;
				self.ui.dialog_cursor = 0;
				self.show_toast(format!("MCP: {} listed · {connected} ready · Esc close", snap.mcp.len()));
				SlashResult::Handled
			}
			"/variants" => {
				self.show_toast("No model variants available for the current model".into());
				SlashResult::Handled
			}

			// ── Provider ─────────────────────────────────────
			"/providers" => {
				// Open models.dev provider connect menu (75+ providers when catalog loads).
				if self.provider.models_catalog.provider_count() == 0 {
					self.reload_models_catalog();
				}
				self.open_popup(crate::state::BottomPopup::Connect);
				self.show_toast(format!(
					"Connect provider · {} in catalog · Enter to add",
					self.provider.models_catalog.provider_count()
				));
				SlashResult::Handled
			}
			"/org" => {
				self.show_toast("Only one organization configured".into());
				SlashResult::Handled
			}

			// ── Prompt / editor ──────────────────────────────
			"/editor" => {
				self.cmd_external_editor();
				SlashResult::Handled
			}
			// /skills handled above (agent skill library)

			// ── Workspace ────────────────────────────────────
			"/workspaces" | "/warp" => {
				if std::env::var("OPENCODE_EXPERIMENTAL_WORKSPACES").is_ok()
					|| std::env::var("DX_EXPERIMENTAL_WORKSPACES").is_ok()
				{
					self.ui.dialog = CommandDialog::Workspaces;
					self.ui.dialog_cursor = 0;
					self.show_toast("Workspaces · Esc close".into());
				} else {
					self
						.show_toast("Workspaces experimental — set OPENCODE_EXPERIMENTAL_WORKSPACES=1".into());
				}
				SlashResult::Handled
			}

			// ── VCS ──────────────────────────────────────────
			"/diff" => {
				self.open_differ();
				SlashResult::Handled
			}

			// ── System ───────────────────────────────────────
			"/status" => {
				self.ui.dialog = CommandDialog::Status;
				self.ui.dialog_cursor = 0;
				SlashResult::Handled
			}
			"/debug" => {
				self.ui.dialog = CommandDialog::Debug;
				self.ui.dialog_cursor = 0;
				SlashResult::Handled
			}
			"/themes" => {
				self.ui.dialog = CommandDialog::Themes;
				self.ui.dialog_cursor = 0;
				self.show_toast("Themes — ↑/↓ · Enter apply · Esc".into());
				SlashResult::Handled
			}
			"/help" => {
				self.ui.dialog = CommandDialog::Help;
				self.ui.dialog_cursor = 0;
				SlashResult::Handled
			}
			"/exit" | ":q" => SlashResult::Exit,

			// ── Modes ────────────────────────────────────────
			"/ask" => {
				self.set_agent_mode(AgentMode::Ask);
				SlashResult::Handled
			}
			"/write" => {
				self.set_agent_mode(AgentMode::Write);
				SlashResult::Handled
			}
			"/plan" => {
				self.set_agent_mode(AgentMode::Plan);
				SlashResult::Handled
			}
			"/goal" => {
				self.set_agent_mode(AgentMode::Goal);
				SlashResult::Handled
			}
			"/goal-pause" => {
				if self.goal.active {
					self.goal.pause();
					self.goal_pending_continue = false;
					self.show_toast("Goal paused".into());
				} else {
					self.show_toast("No active goal".into());
				}
				SlashResult::Handled
			}
			"/goal-resume" => {
				if self.goal.active && self.goal.paused {
					self.goal.resume();
					self.show_toast("Goal resumed".into());
				} else if self.goal.active {
					self.show_toast("Goal is not paused".into());
				} else {
					self.show_toast("No active goal".into());
				}
				SlashResult::Handled
			}
			"/goal-extend" => {
				self.goal.extend(std::time::Duration::from_secs(15 * 60), 4);
				self.show_toast(self.goal.status_line());
				SlashResult::Handled
			}
			"/channels-start" => {
				match crate::channel_actions::start_channel_gateway() {
					Ok(msg) => self.show_toast(msg),
					Err(e) => self.show_toast(format!("Gateway start: {e}")),
				}
				SlashResult::Handled
			}
			"/channels-stop" => {
				match crate::channel_actions::stop_channel_gateway() {
					Ok(msg) => self.show_toast(msg),
					Err(e) => self.show_toast(format!("Gateway stop: {e}")),
				}
				SlashResult::Handled
			}
			"/channel-doctor" => {
				self.provider.channels = crate::channels::load_channels();
				let rows = crate::channel_actions::channel_doctor(&self.provider.channels);
				let summary =
					rows.into_iter().take(6).map(|(k, v)| format!("{k}:{v}")).collect::<Vec<_>>().join(" · ");
				self.show_toast(summary);
				SlashResult::Handled
			}
			"/bind-channel" => {
				self.provider.channels = crate::channels::load_channels();
				let list = crate::channel_actions::sendable_channels(&self.provider.channels);
				if let Some(ch) = list.first() {
					match crate::channel_actions::bind_session_to_channel(&self.session.chat_id, ch, None) {
						Ok(msg) => self.show_toast(msg),
						Err(e) => self.show_toast(format!("Bind failed: {e}")),
					}
				} else {
					self.show_toast("No channels available — configure dx-agent".into());
				}
				SlashResult::Handled
			}
			"/plan-run" => {
				let report = crate::goal_runner::run_plan_tools(&self.plan_options);
				let preview: String = report.chars().take(120).collect();
				self.messages.push(Message::assistant(format!(
					"<think>\n(plan tools)\n</think>\n```command name=\"plan-run\"\n{report}\n```"
				)));
				self.sidebar.refresh_with_diagnostics(self.plan_options.use_lsp);
				self.show_toast(format!("Plan tools · {preview}"));
				SlashResult::Handled
			}
			"/fmt" | "/format" => {
				let cwd = std::path::PathBuf::from(
					self.plan_options.target_folder.as_deref().unwrap_or(&self.session.session_project_dir),
				);
				let apply = resolved.args.eq_ignore_ascii_case("apply")
					|| resolved.args.eq_ignore_ascii_case("--apply")
					|| resolved.args.eq_ignore_ascii_case("write");
				let r = if apply {
					crate::workspace_tools::apply_formatter(&cwd)
				} else {
					crate::workspace_tools::run_formatter(&cwd)
				};
				self.sidebar.set_tool_reports(Some(r.summary.clone()), None);
				self.messages.push(Message::assistant(format!(
					"<think>\n(formatter)\n</think>\n{}",
					r.fence("formatter")
				)));
				self.show_toast(r.summary);
				SlashResult::Handled
			}
			"/lint" => {
				let cwd = std::path::PathBuf::from(
					self.plan_options.target_folder.as_deref().unwrap_or(&self.session.session_project_dir),
				);
				let r = crate::workspace_tools::run_linter(&cwd);
				self.sidebar.set_tool_reports(None, Some(r.summary.clone()));
				self
					.messages
					.push(Message::assistant(format!("<think>\n(linter)\n</think>\n{}", r.fence("linter"))));
				self.show_toast(r.summary);
				SlashResult::Handled
			}
			"/lsp" | "/diagnostics" | "/diag" => {
				let cwd = std::path::PathBuf::from(self.session.session_project_dir.clone());
				let (diags, summary) = crate::workspace_tools::collect_diagnostics(&cwd);
				self.sidebar.refresh();
				let snap = self.sidebar.snapshot();
				let mut body = format!("{summary}\n");
				for d in diags.iter().take(40) {
					body.push_str(&format!(
						"{} {}:{}:{} {}\n",
						d.severity.glyph(),
						d.path,
						d.line,
						d.col,
						d.message
					));
				}
				let servers: String =
					snap.lsp.iter().map(|s| s.name.as_str()).collect::<Vec<_>>().join(", ");
				self.messages.push(Message::assistant(format!(
					"<think>\n(diagnostics)\n</think>\n```command name=\"lsp\"\nservers: {servers}\n{body}\n```"
				)));
				self.show_toast(summary);
				SlashResult::Handled
			}
			"/vcs" | "/git" => {
				let cwd = std::path::PathBuf::from(self.session.session_project_dir.clone());
				let v = crate::workspace_tools::collect_vcs(&cwd);
				self.sidebar.refresh();
				let mut body = format!("{}\n{}\n", v.summary, v.last_commit);
				for s in &v.short_status {
					body.push_str(s);
					body.push('\n');
				}
				if v.short_status.is_empty() && v.available {
					body.push_str("(clean)\n");
				}
				self.messages.push(Message::assistant(format!(
					"<think>\n(vcs)\n</think>\n```command name=\"vcs:git\"\n{body}\n```"
				)));
				// Refresh differ stats when dirty
				if v.dirty {
					let (add, del) = crate::diff_view::quick_diff_stats();
					self.diff_state.total_additions = add;
					self.diff_state.total_deletions = del;
				}
				self.show_toast(v.summary);
				SlashResult::Handled
			}
			"/subagents" | "/agents-list" => {
				self.sidebar.sync_subagents(&self.messages);
				let snap = self.sidebar.snapshot();
				if snap.subagents.is_empty() {
					self.show_toast("No subagents this session".into());
				} else {
					let mut body = String::new();
					for s in &snap.subagents {
						body.push_str(&format!(
							"{} {} — {} ({} lines)\n",
							s.phase.glyph(),
							s.name,
							s.preview,
							s.line_count
						));
					}
					self.messages.push(Message::assistant(format!(
						"<think>\n(subagents)\n</think>\n```command name=\"subagents\"\n{body}\n```"
					)));
					self.show_toast(format!("{} subagent(s)", snap.subagents.len()));
				}
				SlashResult::Handled
			}
			"/doctor" => {
				// Full doctor (uses -j12 cargo check under the hood).
				let report = crate::sidebar_data::try_run_workspace_check();
				let agent_ws = crate::agent_workspace::doctor_line();
				self.sidebar.refresh_with_diagnostics(true);
				self.messages.push(Message::assistant(format!(
					"<think>\n(workspace doctor)\n</think>\n```command name=\"doctor\"\n{report}\n{agent_ws}\n```"
				)));
				self.show_toast("Workspace doctor complete".into());
				SlashResult::Handled
			}
			"/flow-warm" => {
				let flow = Arc::clone(&self.flow_backend);
				if let Ok(handle) = tokio::runtime::Handle::try_current() {
					handle.spawn(async move {
						let mut f = flow.lock().await;
						match f.init().await {
							Ok(()) => tracing::info!("dx-flow warm ok"),
							Err(e) => tracing::warn!("dx-flow warm: {e}"),
						}
					});
				}
				self.show_toast("Warming dx-flow…".into());
				SlashResult::Handled
			}
			"/flow-unload" => {
				let flow = Arc::clone(&self.flow_backend);
				if let Ok(handle) = tokio::runtime::Handle::try_current() {
					handle.spawn(async move {
						let mut f = flow.lock().await;
						f.set_selected_model("");
						f.refresh_models();
					});
				}
				self.show_toast("dx-flow model selection cleared".into());
				SlashResult::Handled
			}
			"/tasks" | "/todo" => {
				if let Ok(idx) = resolved.args.parse::<usize>() {
					self.sidebar.cycle_task(idx);
					self.show_toast(format!("Task {idx} cycled"));
				} else if !resolved.args.is_empty() {
					self.sidebar.complete_task_matching(resolved.args);
					self.show_toast(format!("Completed tasks matching «{}»", resolved.args));
				} else {
					let snap = self.sidebar.snapshot();
					let n = snap.tasks.len();
					let preview = snap
						.tasks
						.iter()
						.take(4)
						.map(|t| {
							format!("{} {}", t.status.glyph(), t.content.chars().take(24).collect::<String>())
						})
						.collect::<Vec<_>>()
						.join(" · ");
					self.show_toast(if n == 0 {
						"No tasks — assistant checkboxes appear here".into()
					} else {
						format!("{n} tasks · {preview}")
					});
				}
				SlashResult::Handled
			}
			"/agent" => {
				self.set_agent_mode(AgentMode::Agent);
				SlashResult::Handled
			}
			"/codex" => {
				self.set_agent_mode(AgentMode::Codex);
				if self.codex_bridge.is_none() && self.codex_connection.is_none() {
					let (tx, rx) = tokio::sync::oneshot::channel();
					self.codex_connection = Some(rx);
					tokio::spawn(async move {
						let result = crate::codex_bridge::CodexBridge::start().await;
						let _ = tx.send(result);
					});
					self.show_toast("Connecting to codex...".into());
				} else {
					self.show_toast("Codex mode active".into());
				}
				SlashResult::Handled
			}
			"/codex-connect" => {
				self.set_agent_mode(AgentMode::Codex);
				self.show_toast("In-process mode only; no URL needed".into());
				if self.codex_bridge.is_none() && self.codex_connection.is_none() {
					let (tx, rx) = tokio::sync::oneshot::channel();
					self.codex_connection = Some(rx);
					tokio::spawn(async move {
						let result = crate::codex_bridge::CodexBridge::start().await;
						let _ = tx.send(result);
					});
					self.show_toast("Connecting to codex...".into());
				}
				SlashResult::Handled
			}
			"/codex-resume" => {
				if resolved.args.is_empty() {
					self.show_toast("Usage: /codex-resume <thread_id>".into());
				} else if self.codex_bridge.is_some() {
					let (tx, rx) = tokio::sync::oneshot::channel();
					let (handle, config, mode) = {
						let bridge = self.codex_bridge.as_ref().unwrap();
						(bridge.request_handle(), bridge.config().clone(), bridge.thread_params_mode())
					};
					let tid = resolved.args.to_string();
					tokio::spawn(async move {
						let result =
							crate::codex_bridge::CodexBridge::resume_thread_rpc(&handle, &config, mode, &tid)
								.await;
						let _ = tx.send(result);
					});
					self.codex_pending_operation = Some(rx);
					self.show_toast(format!("Resuming codex thread: {}", resolved.args));
				} else {
					self.show_toast("Codex not connected. Use /codex first.".into());
				}
				SlashResult::Handled
			}
			"/codex-fork" => {
				if resolved.args.is_empty() {
					self.show_toast("Usage: /codex-fork <thread_id>".into());
				} else if self.codex_bridge.is_some() {
					let (tx, rx) = tokio::sync::oneshot::channel();
					let (handle, config, mode) = {
						let bridge = self.codex_bridge.as_ref().unwrap();
						(bridge.request_handle(), bridge.config().clone(), bridge.thread_params_mode())
					};
					let tid = resolved.args.to_string();
					tokio::spawn(async move {
						let result =
							crate::codex_bridge::CodexBridge::fork_thread_rpc(&handle, &config, mode, &tid).await;
						let _ = tx.send(result);
					});
					self.codex_pending_operation = Some(rx);
					self.show_toast(format!("Forking codex thread: {}", resolved.args));
				} else {
					self.show_toast("Codex not connected. Use /codex first.".into());
				}
				SlashResult::Handled
			}
			"/voice" => {
				self.voice_state.panel.open_panel();
				let raw = input.trim().to_ascii_lowercase();
				if raw.starts_with("/tts") {
					self.voice_state.panel.mode = crate::voice::VoiceMode::Tts;
				} else if raw.starts_with("/stt") {
					self.voice_state.panel.mode = crate::voice::VoiceMode::Stt;
				}
				let (stt, tts) = crate::voice::probe_voice_ready();
				self.voice_state.panel.stt_ready = stt;
				self.voice_state.panel.tts_ready = tts;
				self.show_toast("Voice panel · Tab STT/TTS · Enter run · Esc close".into());
				SlashResult::Handled
			}
			"/share-channel" => {
				self.open_popup(crate::state::BottomPopup::ShareChannel);
				SlashResult::Handled
			}

			// ── Server-side style ────────────────────────────
			"/init" => {
				self.cmd_init_agents_md();
				SlashResult::Handled
			}
			"/review" => {
				self.cmd_review(resolved.args);
				SlashResult::Handled
			}

			other => {
				self.show_toast(format!("Command {other} not implemented yet"));
				SlashResult::Handled
			}
		}
	}

	// ── Session helpers ────────────────────────────────────────

	pub fn persist_current_session(&mut self) {
		let snap = self.snapshot_session();
		crate::session_store::autosave(&snap);
		if let Some(slot) = self.session.session_store.iter_mut().find(|s| s.id == snap.id) {
			*slot = snap.clone();
		} else {
			self.session.session_store.push(snap.clone());
		}
		// Also persist via the new session database with full-text indexing.
		if let Err(e) = self.session_db.save_session(&snap, &self.messages) {
			tracing::warn!("session_db save failed: {e}");
		} else {
			self.session_search.index_session(
				&snap.id,
				&format!(
					"{} {} {} {}",
					snap.name,
					snap.model,
					snap.model_display,
					self.messages.iter().map(|m| m.content.as_str()).collect::<Vec<_>>().join(" "),
				),
			);
		}
		self.save_prefs();
	}

	fn snapshot_session(&self) -> StoredSession {
		StoredSession {
			id: self.session.chat_id.clone(),
			name: self.session.session_name.clone(),
			messages: self.messages.clone(),
			model: self.provider.selected_model.clone(),
			model_display: self.provider.model_display_name.clone(),
			provider: self.provider.model_provider_name.clone(),
			agent_mode: self.agent_mode,
			runtime_mode: self.runtime_mode,
			created_at: self.session.session_created_at,
			updated_at: chrono::Local::now(),
			shared: self.session.session_shared,
			share_url: self.session.share_url.clone(),
			project_dir: self.session.session_project_dir.clone(),
		}
	}

	pub fn cmd_new_session(&mut self) {
		self.persist_current_session();
		// Push undo of "whole session clear" is not needed; start fresh.
		self.undo_stack.clear();
		self.redo_stack.clear();
		self.messages.clear();
		self.session.chat_id = uuid::Uuid::new_v4().to_string();
		self.session.session_name = format!("Session {}", short_id(&self.session.chat_id));
		self.session.title_from_ai = false;
		self.session.title_auto_generated = false;
		self.session.session_start_time = std::time::Instant::now();
		self.session.session_created_at = chrono::Local::now();
		self.session.session_shared = false;
		self.session.share_url = None;
		self.ui.chat_scroll_offset = 0;
		self.ui.active_message_index = None;
		self.is_loading = false;
		self.animation.animation_mode = true;
		self.animation.current_animation_index = 0;
		self.restart_current_animation();
		// Reset dx-agent multi-turn history for the new session.
		let backend = Arc::clone(&self.agent_backend);
		let sid = self.session.chat_id.clone();
		if let Ok(handle) = tokio::runtime::Handle::try_current() {
			handle.spawn(async move {
				backend.reset_session(Some(&sid)).await;
			});
		}
		self.close_dialog();
		self.show_toast(format!("New session: {}", self.session.session_name));
		self.save_prefs();
	}

	pub fn cmd_switch_session(&mut self, index: usize) {
		self.persist_current_session();
		let Some(target) = self.session.session_store.get(index).cloned() else {
			self.show_toast("Session not found".into());
			return;
		};
		self.load_session(target);
		self.close_dialog();
		self.animation.animation_mode = false;
		self.show_toast(format!("Resumed: {}", self.session.session_name));
	}

	pub fn load_session_from_store(&mut self, s: StoredSession) {
		self.load_session(s);
	}

	fn load_session(&mut self, s: StoredSession) {
		self.session.chat_id = s.id.clone();
		self.session.session_name = s.name;
		self.messages = s.messages;
		self.provider.selected_model = s.model;
		self.provider.model_display_name = s.model_display;
		self.provider.model_provider_name = s.provider;
		self.agent_mode = s.agent_mode;
		self.runtime_mode = s.runtime_mode;
		self.selected_local_mode = s.runtime_mode.label().to_string();
		self.session.session_created_at = s.created_at;
		self.session.session_shared = s.shared;
		self.session.share_url = s.share_url;
		self.session.session_project_dir = s.project_dir;
		self.ui.chat_scroll_offset = 0;
		self.undo_stack.clear();
		self.redo_stack.clear();
		self.is_loading = false;
		// Rebind agent history to this session id (will reseed on next turn).
		let backend = Arc::clone(&self.agent_backend);
		let sid = s.id;
		if let Ok(handle) = tokio::runtime::Handle::try_current() {
			handle.spawn(async move {
				// Force rebind by resetting with the target id.
				backend.reset_session(None).await;
				backend.reset_session(Some(&sid)).await;
			});
		}
		self.save_prefs();
	}

	pub fn cmd_share(&mut self) {
		// Local share: generate a file:// or dx:// link and copy it.
		let url = format!(
			"dx://session/{}?name={}",
			self.session.chat_id,
			urlencoding_light(&self.session.session_name)
		);
		self.session.session_shared = true;
		self.session.share_url = Some(url.clone());
		match cli_clipboard::set_contents(url.clone()) {
			Ok(()) => self.show_toast(format!("Shared · link copied: {url}")),
			Err(e) => self.show_toast(format!("Shared (clipboard failed: {e}): {url}")),
		}
		self.persist_current_session();
	}

	pub fn cmd_unshare(&mut self) {
		if !self.session.session_shared {
			self.show_toast("Session is not shared".into());
			return;
		}
		self.session.session_shared = false;
		self.session.share_url = None;
		self.persist_current_session();
		self.show_toast("Session unshared".into());
	}

	pub fn cmd_rename_apply(&mut self) {
		let name = self.ui.dialog_input.trim().to_string();
		if name.is_empty() {
			self.show_toast("Name cannot be empty".into());
			return;
		}
		self.session.session_name = name.clone();
		self.persist_current_session();
		self.close_dialog();
		self.show_toast(format!("Renamed to: {name}"));
	}

	pub fn cmd_user_name_apply(&mut self) {
		let name = self.ui.dialog_input.trim().to_string();
		self.close_dialog();
		self.set_user_display_name(name);
	}

	pub fn cmd_fork_at(&mut self, message_index: usize) {
		if message_index >= self.messages.len() {
			self.show_toast(format!(
				"Message index {message_index} out of range (0..{})",
				self.messages.len().saturating_sub(1)
			));
			return;
		}
		self.persist_current_session();
		let forked_msgs: Vec<Message> = self.messages[..=message_index].to_vec();
		self.undo_stack.clear();
		self.redo_stack.clear();
		self.messages = forked_msgs;
		self.session.chat_id = uuid::Uuid::new_v4().to_string();
		self.session.session_name = format!("{} (fork)", self.session.session_name);
		self.session.session_created_at = chrono::Local::now();
		self.session.session_start_time = std::time::Instant::now();
		self.session.session_shared = false;
		self.session.share_url = None;
		self.ui.chat_scroll_offset = 0;
		self.close_dialog();
		self.animation.animation_mode = false;
		self.show_toast(format!("Forked at message {message_index} → {}", self.session.session_name));
	}

	pub fn cmd_compact(&mut self) {
		if self.messages.is_empty() {
			self.show_toast("Nothing to compact".into());
			return;
		}
		self.push_undo();
		let report = crate::compaction::compact_messages(&mut self.messages, false);
		self.ui.chat_scroll_offset = 0;
		self.show_toast(format!(
			"Compacted {}→{} msgs ({}→{} tok)",
			report.before_msgs, report.after_msgs, report.before_tokens, report.after_tokens
		));
	}

	pub fn cmd_undo(&mut self) {
		// Revert last user+assistant exchange
		if self.messages.is_empty() {
			self.show_toast("Nothing to undo".into());
			return;
		}
		// Snapshot for /redo
		self.redo_stack.push(self.messages.clone());
		if self.redo_stack.len() > 32 {
			self.redo_stack.remove(0);
		}
		// Pop trailing assistant response(s), then the user prompt
		while self.messages.last().is_some_and(|m| m.role == MessageRole::Assistant) {
			self.messages.pop();
		}
		if self.messages.last().is_some_and(|m| m.role == MessageRole::User) {
			self.messages.pop();
		}
		self.show_toast("Undid last exchange".into());
	}

	pub fn cmd_redo(&mut self) {
		let Some(snapshot) = self.redo_stack.pop() else {
			self.show_toast("Nothing to redo".into());
			return;
		};
		self.undo_stack.push(self.messages.clone());
		self.messages = snapshot;
		self.show_toast("Redid last undo".into());
	}

	fn push_undo(&mut self) {
		self.undo_stack.push(self.messages.clone());
		if self.undo_stack.len() > 32 {
			self.undo_stack.remove(0);
		}
		// New edit branch clears redo history
		self.redo_stack.clear();
	}

	pub fn cmd_copy_transcript(&mut self) {
		let text = self.transcript_markdown(true, true);
		match cli_clipboard::set_contents(text.clone()) {
			Ok(()) => self.show_toast(format!("Copied transcript ({} chars)", text.len())),
			Err(e) => self.show_toast(format!("Clipboard error: {e}")),
		}
	}

	pub fn cmd_export_apply(&mut self) {
		let name = self.ui.dialog_input.trim().to_string();
		if name.is_empty() {
			self.show_toast("Filename required".into());
			return;
		}
		let path = PathBuf::from(&name);
		let md =
			self.transcript_markdown(self.ui.export_include_thinking, self.ui.export_include_tools);
		match std::fs::write(&path, md) {
			Ok(()) => {
				self.close_dialog();
				self.show_toast(format!("Exported → {}", path.display()));
			}
			Err(e) => self.show_toast(format!("Export failed: {e}")),
		}
	}

	pub fn cmd_move_apply(&mut self) {
		let dir = self.ui.dialog_input.trim().to_string();
		if dir.is_empty() {
			self.show_toast("Directory required".into());
			return;
		}
		let path = PathBuf::from(&dir);
		if !path.is_dir() {
			self.show_toast(format!("Not a directory: {dir}"));
			return;
		}
		self.session.session_project_dir = path.display().to_string();
		self.persist_current_session();
		self.close_dialog();
		self.show_toast(format!("Session project → {}", path.display()));
	}

	pub fn transcript_markdown(&self, include_thinking: bool, include_tools: bool) -> String {
		let mut out = String::new();
		out.push_str(&format!(
			"# {}\n\n- id: `{}`\n- model: {} ({})\n- mode: {}\n- runtime: {}\n\n---\n\n",
			self.session.session_name,
			self.session.chat_id,
			self.provider.model_display_name,
			self.provider.model_provider_name,
			self.agent_mode.label(),
			self.runtime_mode.label(),
		));
		for (i, msg) in self.messages.iter().enumerate() {
			let role = match msg.role {
				MessageRole::User => "User",
				MessageRole::Assistant => "Assistant",
			};
			out.push_str(&format!("## [{i}] {role}\n\n"));
			let mut body = msg.content.clone();
			if !include_thinking {
				body = strip_tag_block(&body, "<think>", "</think>");
			}
			if !include_tools {
				body = strip_fenced_commands(&body);
			}
			out.push_str(body.trim());
			out.push_str("\n\n");
		}
		out
	}

	pub fn apply_thinking_visibility(&mut self) {
		let expand = matches!(self.thinking_visibility, ThinkingVisibility::Show);
		for msg in &mut self.messages {
			if msg.role == MessageRole::Assistant && msg.has_thinking() {
				msg.thinking_expanded = expand;
			}
		}
	}

	pub fn cmd_external_editor(&mut self) {
		let editor = std::env::var("EDITOR")
			.or_else(|_| std::env::var("VISUAL"))
			.unwrap_or_else(|_| if cfg!(windows) { "notepad".into() } else { "vi".into() });
		let tmp =
			std::env::temp_dir().join(format!("dx-prompt-{}.md", short_id(&self.session.chat_id)));
		let initial = self.input.content.clone();
		if let Err(e) = std::fs::write(&tmp, &initial) {
			self.show_toast(format!("Editor temp file failed: {e}"));
			return;
		}
		// Note: launching a blocking editor from the TUI will suspend the terminal briefly.
		let status = std::process::Command::new(&editor).arg(&tmp).status();
		match status {
			Ok(s) if s.success() => match std::fs::read_to_string(&tmp) {
				Ok(text) => {
					self.input.replace_content(text.trim_end());
					self.show_toast(format!("Loaded prompt from {editor}"));
				}
				Err(e) => self.show_toast(format!("Read editor output failed: {e}")),
			},
			Ok(s) => self.show_toast(format!("Editor exited with {s}")),
			Err(e) => self.show_toast(format!("Failed to launch {editor}: {e}")),
		}
		let _ = std::fs::remove_file(&tmp);
	}

	pub fn cmd_init_agents_md(&mut self) {
		let path = Path::new("AGENTS.md");
		if path.exists() {
			self.show_toast("AGENTS.md already exists".into());
			return;
		}
		let scaffold = r#"# AGENTS.md

## Project Overview

Describe this project for the coding agent.

## Stack

- Language:
- Framework:
- Build:

## Conventions

- Prefer small, focused changes
- Run tests after meaningful edits

## Boundaries

### Do Not Modify

- secrets / credentials
- generated lockfile churn without need

### Safe to Modify

- application source under `src/`
"#;
		match std::fs::write(path, scaffold) {
			Ok(()) => self.show_toast("Created AGENTS.md scaffold".into()),
			Err(e) => self.show_toast(format!("Failed to write AGENTS.md: {e}")),
		}
	}

	pub fn cmd_review(&mut self, args: &str) {
		let scope = if args.is_empty() { "uncommitted" } else { args };
		// Open differ for uncommitted; for branch/pr show toast with git status summary.
		match scope {
			"commit" | "branch" | "pr" => {
				let summary = git_review_summary(scope);
				self.messages.push(Message::assistant(format!(
					"**Review ({scope})**\n\n```\n{summary}\n```\n\nUse `/diff` for the full-screen differ."
				)));
				self.animation.animation_mode = false;
				self.show_toast(format!("Review: {scope}"));
			}
			_ => {
				self.open_differ();
				self.show_toast("Review: uncommitted changes (differ)".into());
			}
		}
	}

	pub fn close_dialog(&mut self) {
		self.ui.dialog = CommandDialog::None;
		self.ui.dialog_input.clear();
		self.ui.dialog_cursor = 0;
	}

	/// Lines for the active command dialog list UI.
	pub fn dialog_list_items(&self) -> Vec<(String, String)> {
		match self.ui.dialog {
			CommandDialog::None => Vec::new(),
			CommandDialog::Sessions => {
				let mut items = Vec::new();
				// Current first
				items.push((
					format!("● {} (current)", self.session.session_name),
					format!("{} msgs · {}", self.messages.len(), short_id(&self.session.chat_id)),
				));
				for s in &self.session.session_store {
					if s.id == self.session.chat_id {
						continue;
					}
					items.push((
						format!("○ {}", s.name),
						format!("{} msgs · {}", s.messages.len(), short_id(&s.id)),
					));
				}
				if items.len() == 1 && self.session.session_store.is_empty() {
					// only current
				}
				items
			}
			CommandDialog::Timeline | CommandDialog::Fork => self
				.messages
				.iter()
				.enumerate()
				.map(|(i, m)| {
					let role = match m.role {
						MessageRole::User => "User",
						MessageRole::Assistant => "Asst",
					};
					let preview: String = m.content.chars().take(48).collect();
					(format!("[{i}] {role}"), preview.replace('\n', " "))
				})
				.collect(),
			CommandDialog::Themes => {
				ChatTheme::available_themes().into_iter().map(|(name, title)| (title, name)).collect()
			}
			CommandDialog::Help => SLASH_COMMANDS
				.iter()
				.take(40)
				.map(|s| {
					let aliases = if s.aliases.is_empty() {
						String::new()
					} else {
						format!(" ({})", s.aliases.join(", "))
					};
					(format!("{}{aliases}", s.name), s.description.to_string())
				})
				.collect(),
			CommandDialog::Status => {
				let (approval, sandbox) = crate::profile_prompts::profile_policy(self.agent_mode);
				let mut rows = vec![
					("Session".into(), self.session.session_name.clone()),
					("Chat ID".into(), self.session.chat_id.clone()),
					("Messages".into(), self.messages.len().to_string()),
					(
						"Model".into(),
						format!("{}, {}", self.provider.model_display_name, self.provider.model_provider_name),
					),
					("Mode".into(), self.agent_mode.label().into()),
					("Policy".into(), format!("{approval} / {sandbox}")),
					("Runtime".into(), self.runtime_mode.label().into()),
					("Tokens".into(), self.token_usage_label()),
					("Cost".into(), self.cost_label()),
					("Diffs".into(), self.diff_label()),
					(
						"Token-save".into(),
						crate::token_save::telemetry_line(
							&self.session.last_token_save_report,
							self.session.token_save_enabled,
						),
					),
					(
						"Providers".into(),
						crate::provider_registry::doctor_summary(
							&self.provider.models_catalog,
							&self.provider.provider_store,
						),
					),
					(
						"Top providers".into(),
						crate::provider_registry::list_providers(
							&self.provider.models_catalog,
							&self.provider.provider_store,
						)
						.into_iter()
						.take(3)
						.map(|r| r.label())
						.collect::<Vec<_>>()
						.join(" | "),
					),
					("Shared".into(), if self.session.session_shared { "yes".into() } else { "no".into() }),
					("Project".into(), self.session.session_project_dir.clone()),
					("Sessions dir".into(), crate::session_store::sessions_root().display().to_string()),
					("Thinking".into(), self.thinking_visibility.label().into()),
					("Timestamps".into(), if self.ui.show_timestamps { "on".into() } else { "off".into() }),
					("Goal".into(), self.goal.status_line()),
				];
				if crate::omniroute::proxy_enabled() {
					rows.push(("OmniRoute".into(), "proxy on".into()));
				}
				rows
			}
			CommandDialog::Debug => vec![
				("dx-tui".into(), env!("CARGO_PKG_VERSION").into()),
				("chat_id".into(), self.session.chat_id.clone()),
				("sessions_stored".into(), self.session.session_store.len().to_string()),
				("sessions_disk".into(), crate::session_store::sessions_root().display().to_string()),
				("prefs".into(), crate::tui_prefs::path_display()),
				("undo_depth".into(), self.undo_stack.len().to_string()),
				("redo_depth".into(), self.redo_stack.len().to_string()),
				("loading".into(), self.is_loading.to_string()),
				("animation_mode".into(), self.animation.animation_mode.to_string()),
				("agent_ready".into(), self.agent_backend.is_ready().to_string()),
				("flow_source".into(), crate::channels::flow_source_available().to_string()),
				("agent_source".into(), crate::channels::agent_source_available().to_string()),
				("cwd".into(), self.session.session_project_dir.clone()),
			],
			CommandDialog::Skills => vec![
				("/review".into(), "Review changes".into()),
				("/init".into(), "Scaffold AGENTS.md".into()),
				("/compact".into(), "Compact context".into()),
				("/diff".into(), "Open differ".into()),
				("/status".into(), "Session status".into()),
			],
			CommandDialog::Connect => vec![
				(
					"Remote · OpenCode Zen".into(),
					if self.runtime_mode == RuntimeMode::Remote { "active".into() } else { "switch".into() },
				),
				(
					"Local · dx-flow".into(),
					if self.runtime_mode == RuntimeMode::Local { "active".into() } else { "switch".into() },
				),
				("dx-agent channels".into(), "open channels menu".into()),
			],
			CommandDialog::Mcps => vec![
				("MCP".into(), "See right sidebar · MCP accordion".into()),
				("Configure".into(), "Menu → MCP Servers".into()),
			],
			CommandDialog::Workspaces => vec![
				("Current".into(), self.session.session_project_dir.clone()),
				("Home".into(), dirs::home_dir().map(|p| p.display().to_string()).unwrap_or_default()),
			],
			CommandDialog::Rename
			| CommandDialog::UserName
			| CommandDialog::Export
			| CommandDialog::Note
			| CommandDialog::Move => Vec::new(),
		}
	}

	pub fn activate_dialog_selection(&mut self) {
		match self.ui.dialog {
			CommandDialog::Sessions => {
				// index 0 = current; rest map to session_store excluding current
				if self.ui.dialog_cursor == 0 {
					self.close_dialog();
					self.show_toast("Already on current session".into());
					return;
				}
				let others: Vec<_> = self
					.session
					.session_store
					.iter()
					.enumerate()
					.filter(|(_, s)| s.id != self.session.chat_id)
					.map(|(i, _)| i)
					.collect();
				let store_idx = others.get(self.ui.dialog_cursor - 1).copied();
				if let Some(i) = store_idx {
					self.cmd_switch_session(i);
				}
			}
			CommandDialog::Timeline => {
				if self.ui.dialog_cursor < self.messages.len() {
					self.scroll_to_message_index(self.ui.dialog_cursor);
					self.ui.active_message_index = Some(self.ui.dialog_cursor);
					self.close_dialog();
					self.show_toast(format!("Jumped to message {}", self.ui.dialog_cursor));
				}
			}
			CommandDialog::Fork => {
				self.cmd_fork_at(self.ui.dialog_cursor);
			}
			CommandDialog::Themes => {
				let themes = ChatTheme::available_themes();
				if let Some((name, _)) = themes.get(self.ui.dialog_cursor) {
					self.apply_theme(name, self.theme_mode);
					self.close_dialog();
					self.show_toast(format!("Theme: {name}"));
				}
			}
			CommandDialog::Skills => {
				let skills = self.dialog_list_items();
				if let Some((cmd, _)) = skills.get(self.ui.dialog_cursor) {
					self.input.replace_content(cmd);
					self.close_dialog();
					self.show_toast(format!("Inserted {cmd}"));
				}
			}
			CommandDialog::Connect => match self.ui.dialog_cursor {
				0 => {
					if self.runtime_mode != RuntimeMode::Remote {
						self.toggle_runtime_mode();
					}
					self.close_dialog();
				}
				1 => {
					if self.runtime_mode != RuntimeMode::Local {
						self.toggle_runtime_mode();
					}
					self.close_dialog();
				}
				2 => {
					self.close_dialog();
					self.open_popup(BottomPopup::Channels);
				}
				_ => self.close_dialog(),
			},
			CommandDialog::Workspaces => {
				if let Some((_, path)) = self.dialog_list_items().get(self.ui.dialog_cursor)
					&& Path::new(path).is_dir()
				{
					self.session.session_project_dir = path.clone();
					self.show_toast(format!("Workspace → {path}"));
				}
				self.close_dialog();
			}
			CommandDialog::Rename => self.cmd_rename_apply(),
			CommandDialog::UserName => self.cmd_user_name_apply(),
			CommandDialog::Export => self.cmd_export_apply(),
			CommandDialog::Note => {
				let note = self.ui.dialog_input.trim().to_string();
				self.sidebar.set_note(note);
				self.close_dialog();
				self.show_toast("Session note saved".into());
			}
			CommandDialog::Move => self.cmd_move_apply(),
			CommandDialog::Help | CommandDialog::Status | CommandDialog::Debug | CommandDialog::Mcps => {
				self.close_dialog();
			}
			CommandDialog::None => {}
		}
	}

	pub fn dialog_move(&mut self, delta: i32) {
		let len = self.dialog_list_items().len().max(1);
		if delta < 0 {
			self.ui.dialog_cursor = self.ui.dialog_cursor.saturating_sub((-delta) as usize);
		} else {
			self.ui.dialog_cursor = (self.ui.dialog_cursor + delta as usize).min(len.saturating_sub(1));
		}
	}
}

/// Snapshot of a chat session for `/sessions` resume.
#[derive(Debug, Clone)]
pub struct StoredSession {
	pub id: String,
	pub name: String,
	pub messages: Vec<Message>,
	pub model: String,
	pub model_display: String,
	pub provider: String,
	pub agent_mode: AgentMode,
	pub runtime_mode: RuntimeMode,
	pub created_at: chrono::DateTime<chrono::Local>,
	pub updated_at: chrono::DateTime<chrono::Local>,
	pub shared: bool,
	pub share_url: Option<String>,
	pub project_dir: String,
}

fn short_id(id: &str) -> &str {
	id.split('-').next().unwrap_or(id)
}

fn urlencoding_light(s: &str) -> String {
	s.chars()
		.map(|c| match c {
			'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
			' ' => "%20".into(),
			_ => format!("%{:02X}", c as u8),
		})
		.collect()
}

fn strip_tag_block(s: &str, open: &str, close: &str) -> String {
	let mut out = String::new();
	let mut rest = s;
	while let Some(start) = rest.find(open) {
		out.push_str(&rest[..start]);
		if let Some(end_rel) = rest[start..].find(close) {
			rest = &rest[start + end_rel + close.len()..];
		} else {
			break;
		}
	}
	out.push_str(rest);
	out
}

fn strip_fenced_commands(s: &str) -> String {
	let mut out = String::new();
	let mut in_cmd = false;
	for line in s.lines() {
		let t = line.trim();
		if t.starts_with("```command") || t == "```approval" {
			in_cmd = true;
			continue;
		}
		if in_cmd {
			if t == "```" {
				in_cmd = false;
			}
			continue;
		}
		out.push_str(line);
		out.push('\n');
	}
	out
}

fn git_review_summary(scope: &str) -> String {
	let args: &[&str] = match scope {
		"commit" => &["log", "-5", "--oneline"],
		"branch" => &["status", "-sb"],
		"pr" => &["log", "main..HEAD", "--oneline"],
		_ => &["status", "--short"],
	};
	std::process::Command::new("git")
		.args(args)
		.output()
		.map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
		.unwrap_or_else(|e| format!("git failed: {e}"))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn resolve_aliases() {
		assert_eq!(resolve("/resume").unwrap().canonical, "/sessions");
		assert_eq!(resolve("/clear").unwrap().canonical, "/new");
		assert_eq!(resolve("/mo").unwrap().canonical, "/models");
		assert_eq!(resolve("/q").unwrap().canonical, "/exit");
		assert_eq!(resolve(":q").unwrap().canonical, ":q");
		assert_eq!(resolve("/summarize").unwrap().canonical, "/compact");
	}

	#[test]
	fn resolve_args() {
		let r = resolve("/review branch").unwrap();
		assert_eq!(r.canonical, "/review");
		assert_eq!(r.args, "branch");
	}

	#[test]
	fn autocomplete_includes_aliases() {
		let pairs = autocomplete_pairs();
		assert!(pairs.iter().any(|(c, _)| *c == "/sessions"));
		assert!(pairs.iter().any(|(c, _)| *c == "/resume"));
		assert!(pairs.iter().any(|(c, _)| *c == "/new"));
	}
}
