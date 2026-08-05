//! Right sidebar: Tasks · Subagents · LSPs · Plugins · MCPs · Prompts · Notes.

use std::{
	path::Path,
	sync::{Arc, Mutex},
	time::{Duration, Instant},
};

use crate::{
	components::Message,
	workspace_tools::{self, SubagentRecord, extract_subagents, which_bin},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
	Pending,
	InProgress,
	Done,
	Cancelled,
}

impl TaskStatus {
	pub fn glyph(self) -> &'static str {
		match self {
			Self::Pending => "☐",
			Self::InProgress => "◐",
			Self::Done => "☑",
			Self::Cancelled => "☒",
		}
	}
}

#[derive(Debug, Clone)]
pub struct TaskItem {
	pub content: String,
	pub status: TaskStatus,
}

#[derive(Debug, Clone)]
pub struct LspServerStatus {
	pub name: String,
	pub language: String,
	pub available: bool,
}

#[derive(Debug, Clone)]
pub struct PluginStatus {
	pub name: String,
	pub enabled: bool,
}

#[derive(Debug, Clone)]
pub struct McpStatus {
	pub name: String,
	pub connected: bool,
	pub detail: String,
}

/// Sidebar accordion sections (after session title).
pub const SIDEBAR_SECTION_COUNT: usize = 7;

pub mod section {
	pub const NAMES: [&str; super::SIDEBAR_SECTION_COUNT] =
		["Tasks", "Prompts", "Notes", "Subagents", "LSP", "Plugins", "MCP"];
}

#[derive(Debug, Clone, Default)]
pub struct PromptItem {
	pub id: String,
	pub content: String,
}

#[derive(Debug, Clone, Default)]
pub struct SidebarSnapshot {
	pub lsp: Vec<LspServerStatus>,
	pub plugins: Vec<PluginStatus>,
	pub mcp: Vec<McpStatus>,
	pub subagents: Vec<SubagentRecord>,
	pub tasks: Vec<TaskItem>,
	pub prompts: Vec<PromptItem>,
	pub note: String,
	pub last_refresh: Option<Instant>,
}

#[derive(Clone, Default)]
pub struct SidebarState {
	inner: Arc<Mutex<SidebarSnapshot>>,
}

impl SidebarState {
	pub fn new() -> Self {
		let s = Self::default();
		s.refresh();
		s
	}

	pub fn snapshot(&self) -> SidebarSnapshot {
		self.inner.lock().map(|g| g.clone()).unwrap_or_default()
	}

	pub fn set_tasks(&self, tasks: Vec<TaskItem>) {
		if let Ok(mut g) = self.inner.lock() {
			g.tasks = tasks;
		}
	}

	pub fn merge_tasks(&self, extra: Vec<TaskItem>) {
		if let Ok(mut g) = self.inner.lock() {
			for t in extra {
				if !g.tasks.iter().any(|x| x.content == t.content) {
					g.tasks.push(t);
				}
			}
		}
	}

	/// Advance task status: ☐ → ◐ → ☑ → ☒ → remove.
	pub fn cycle_task(&self, index: usize) {
		if let Ok(mut g) = self.inner.lock() {
			if index >= g.tasks.len() {
				return;
			}
			match g.tasks[index].status {
				TaskStatus::Pending => g.tasks[index].status = TaskStatus::InProgress,
				TaskStatus::InProgress => g.tasks[index].status = TaskStatus::Done,
				TaskStatus::Done => g.tasks[index].status = TaskStatus::Cancelled,
				TaskStatus::Cancelled => {
					g.tasks.remove(index);
				}
			}
		}
	}

	pub fn remove_task(&self, index: usize) {
		if let Ok(mut g) = self.inner.lock()
			&& index < g.tasks.len()
		{
			g.tasks.remove(index);
		}
	}

	pub fn remove_completed_tasks(&self) {
		if let Ok(mut g) = self.inner.lock() {
			g.tasks.retain(|t| !matches!(t.status, TaskStatus::Done | TaskStatus::Cancelled));
		}
	}

	pub fn complete_task_matching(&self, needle: &str) {
		let n = needle.to_ascii_lowercase();
		if let Ok(mut g) = self.inner.lock() {
			for t in g.tasks.iter_mut() {
				if t.content.to_ascii_lowercase().contains(&n) {
					t.status = TaskStatus::Done;
				}
			}
		}
	}

	/// Replace tasks from a full todowrite payload (keeps order; drops empty).
	pub fn apply_todo_list(&self, todos: Vec<TaskItem>) {
		if let Ok(mut g) = self.inner.lock() {
			g.tasks = todos.into_iter().filter(|t| !t.content.trim().is_empty()).collect();
		}
	}

	pub fn sync_subagents(&self, messages: &[Message]) {
		let recs = extract_subagents(messages);
		if let Ok(mut g) = self.inner.lock() {
			g.subagents = recs;
		}
	}

	pub fn set_tool_reports(&self, _fmt: Option<String>, _lint: Option<String>) {
		// No TOOLS section — kept as no-op for call sites.
	}

	pub fn refresh(&self) {
		self.refresh_with_diagnostics(false);
	}

	pub fn refresh_with_diagnostics(&self, _run_diags: bool) {
		let lsp = probe_lsp_servers();
		let plugins = probe_plugins();
		let mcp = probe_mcp();
		if let Ok(mut g) = self.inner.lock() {
			g.lsp = lsp;
			g.plugins = plugins;
			g.mcp = mcp;
			g.last_refresh = Some(Instant::now());
		}
	}

	pub fn refresh_diagnostics(&self) {
		// Diagnostics live in chat via /lsp; sidebar only lists LSPs on PATH.
		self.refresh();
	}

	pub fn add_prompt(&self, content: String) {
		if let Ok(mut g) = self.inner.lock() {
			let id = format!("p{}", g.prompts.len() + 1);
			g.prompts.push(PromptItem { id, content });
		}
	}

	pub fn remove_prompt(&self, index: usize) {
		if let Ok(mut g) = self.inner.lock()
			&& index < g.prompts.len()
		{
			g.prompts.remove(index);
		}
	}

	pub fn clear_prompts(&self) {
		if let Ok(mut g) = self.inner.lock() {
			g.prompts.clear();
		}
	}

	pub fn set_note(&self, note: String) {
		if let Ok(mut g) = self.inner.lock() {
			g.note = note;
		}
	}

	pub fn refresh_if_stale(&self, max_age: Duration) {
		let stale = self
			.inner
			.lock()
			.ok()
			.and_then(|g| g.last_refresh)
			.map(|t| t.elapsed() > max_age)
			.unwrap_or(true);
		if stale {
			self.refresh();
		}
	}

	/// Seven sections: Tasks · Prompts · Notes · Subagents · LSP · Plugins · MCP.
	pub fn section_lines(&self) -> [(&'static str, Vec<String>); SIDEBAR_SECTION_COUNT] {
		let snap = self.snapshot();

		let tasks = {
			if snap.tasks.is_empty() {
				vec!["No Tasks Yet".into()]
			} else {
				snap
					.tasks
					.iter()
					.map(|t| {
						let label = match t.status {
							TaskStatus::Pending => "pending",
							TaskStatus::InProgress => "active",
							TaskStatus::Done => "done",
							TaskStatus::Cancelled => "cancelled",
						};
						format!("{} [{}] {}", t.status.glyph(), label, t.content)
					})
					.collect()
			}
		};

		let subagents = {
			let mut lines = Vec::new();
			if snap.subagents.is_empty() {
				lines.push("—".into());
			} else {
				for s in snap.subagents.iter().take(12) {
					let bit = if s.preview.is_empty() {
						format!("{} lines", s.line_count)
					} else {
						s.preview.chars().take(28).collect()
					};
					lines.push(format!("{} {} · {bit}", s.phase.glyph(), s.name));
				}
			}
			lines
		};

		let lsps = {
			let ready: Vec<_> = snap.lsp.iter().filter(|s| s.available).collect();
			if ready.is_empty() {
				vec!["—".into()]
			} else {
				ready.iter().take(10).map(|s| format!("● {} · {}", s.name, s.language)).collect()
			}
		};

		let plugins = {
			if snap.plugins.is_empty() {
				vec!["—".into()]
			} else {
				snap
					.plugins
					.iter()
					.take(12)
					.map(|p| format!("{} {}", if p.enabled { "●" } else { "○" }, p.name))
					.collect()
			}
		};

		let mcps = {
			let real: Vec<_> = snap.mcp.iter().filter(|m| m.name != "(none configured)").collect();
			if real.is_empty() {
				vec!["—".into()]
			} else {
				real
					.iter()
					.take(12)
					.map(|m| {
						format!(
							"{} {} · {}",
							if m.connected { "●" } else { "○" },
							m.name,
							m.detail.chars().take(20).collect::<String>()
						)
					})
					.collect()
			}
		};

		let prompts = {
			if snap.prompts.is_empty() {
				vec!["No Prompts Yet".into()]
			} else {
				snap
					.prompts
					.iter()
					.map(|p| {
						let single_line = p.content.replace('\n', " ");
						let text = single_line.chars().take(36).collect::<String>();
						if single_line.len() > 36 { format!("{text}…") } else { text }
					})
					.collect()
			}
		};

		let notes = {
			// First element is always empty (reserved for top border), content follows
			let mut lines = vec![String::new()];
			if snap.note.is_empty() {
				lines.push("No Notes Yet".into());
			} else {
				for l in snap.note.lines() {
					let clipped = l.chars().take(36).collect::<String>();
					lines.push(if l.len() > 36 { format!("{clipped}…") } else { clipped });
				}
			}
			// Pad to at least 1 border + 5 content lines
			while lines.len() < 6 {
				lines.push(String::new());
			}
			lines
		};

		let bodies = [tasks, prompts, notes, subagents, lsps, plugins, mcps];
		std::array::from_fn(|i| (section::NAMES[i], bodies[i].clone()))
	}
}

fn which(name: &str) -> bool {
	which_bin(name)
}

pub fn probe_lsp_servers() -> Vec<LspServerStatus> {
	const CANDIDATES: &[(&str, &str)] = &[
		("rust-analyzer", "Rust"),
		("typescript-language-server", "TypeScript"),
		("pyright", "Python"),
		("gopls", "Go"),
		("clangd", "C/C++"),
		("lua-language-server", "Lua"),
		("vscode-json-language-server", "JSON"),
		("yaml-language-server", "YAML"),
		("bash-language-server", "Bash"),
		("zls", "Zig"),
	];
	CANDIDATES
		.iter()
		.filter(|(bin, _)| which(bin))
		.map(|(bin, lang)| LspServerStatus {
			name: (*bin).into(),
			language: (*lang).into(),
			available: true,
		})
		.collect()
}

fn probe_plugins() -> Vec<PluginStatus> {
	let mut plugins = Vec::new();
	// Real project plugins only (no marketing / stack labels).
	let roots = [Path::new("src/file_browser/plugin/preset/plugins"), Path::new("plugins")];
	for preset in roots {
		if !preset.is_dir() {
			continue;
		}
		if let Ok(rd) = std::fs::read_dir(preset) {
			for e in rd.flatten().take(20) {
				let name = e.file_name().to_string_lossy().to_string();
				if name.ends_with(".lua") {
					plugins
						.push(PluginStatus { name: name.trim_end_matches(".lua").to_string(), enabled: true });
				} else if e.path().is_dir() {
					plugins.push(PluginStatus { name, enabled: true });
				}
			}
		}
	}
	plugins
}

fn probe_mcp() -> Vec<McpStatus> {
	let mut out = Vec::new();
	let candidates = [
		dirs::home_dir().map(|h| h.join(".config/dx/mcp.toml")),
		dirs::home_dir().map(|h| h.join(".config/dx/config.toml")),
		dirs::home_dir().map(|h| h.join(".codex/config.toml")),
		dirs::home_dir().map(|h| h.join(".claude/mcp.json")),
		dirs::home_dir().map(|h| h.join(".cursor/mcp.json")),
	];
	for path in candidates.into_iter().flatten() {
		if !path.is_file() {
			continue;
		}
		let Ok(text) = std::fs::read_to_string(&path) else {
			continue;
		};
		if path.extension().and_then(|e| e.to_str()) == Some("json") {
			if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text)
				&& let Some(servers) = v.get("mcpServers").and_then(|s| s.as_object())
			{
				for (name, cfg) in servers {
					let cmd = cfg.get("command").and_then(|c| c.as_str()).unwrap_or("");
					let connected = !cmd.is_empty() && which(cmd);
					out.push(McpStatus {
						name: name.clone(),
						connected,
						detail: if connected { "ready".into() } else { "offline".into() },
					});
				}
			}
			continue;
		}
		let mut current: Option<String> = None;
		let mut current_cmd: Option<String> = None;
		let flush = |out: &mut Vec<McpStatus>, name: Option<String>, cmd: Option<String>| {
			if let Some(name) = name {
				let connected = cmd.as_ref().map(|c| which(c)).unwrap_or(false);
				if !out.iter().any(|m| m.name == name) {
					out.push(McpStatus {
						name,
						connected,
						detail: if connected { "ready".into() } else { "configured".into() },
					});
				}
			}
		};
		for line in text.lines() {
			let t = line.trim();
			if let Some(rest) = t.strip_prefix("[mcp_servers.").or_else(|| t.strip_prefix("[mcp.")) {
				flush(&mut out, current.take(), current_cmd.take());
				if let Some(end) = rest.find(']') {
					let name = rest[..end].trim().trim_matches('"').to_string();
					if !name.is_empty() {
						current = Some(name);
					}
				}
			} else if current.is_some()
				&& let Some(rest) = t.strip_prefix("command")
			{
				let val = rest
					.trim_start_matches(|c: char| c == '=' || c.is_whitespace())
					.trim_matches('"')
					.trim_matches('\'')
					.to_string();
				if !val.is_empty() {
					current_cmd = Some(val);
				}
			}
		}
		flush(&mut out, current, current_cmd);
	}
	out
}

pub fn workspace_tool_status() -> Vec<(String, bool, String)> {
	workspace_tools::tool_inventory(
		&std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf()),
	)
	.into_iter()
	.map(|t| (t.name, t.available, t.detail))
	.collect()
}

pub fn try_run_workspace_check() -> String {
	workspace_tools::workspace_doctor(
		&std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf()),
	)
}
