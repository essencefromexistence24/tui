//! Multi-agent orchestration — deer-flow inspired production parallel subagent runtime.

#![allow(dead_code)]
//!
//! - **Lead agent** delegates via `task` tool with `description`, `prompt`, `subagent_type`.
//! - **Subagents** run in true parallel (up to `MAX_CONCURRENT_SUBAGENTS` = 3) via `tokio::join!`.
//! - Each subagent gets its OWN LLM call via `zen::stream_chat_messages` — not tool-only.
//! - Wall-clock timeout (default 300s) with cooperative cancellation via `CancellationToken`.
//! - Structured `SubagentStatus` enum: Pending → Running → Completed | Failed | TimedOut | Cancelled.
//! - Streaming output sent through the agent channel incrementally.
//! - Delegation ledger prevents re-delegating completed work.

use std::{
	path::Path,
	sync::Arc,
	time::{Duration, Instant},
};

use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::{
	modes::AgentMode,
	tools::{ToolCall, ToolKind, ToolResult, execute as exec_tool, format_tool_result},
	zen,
};

// ── Constants ────────────────────────────────────────────────────────────

/// Max concurrent subagents dispatched per turn (deer-flow: 3).
pub const MAX_CONCURRENT_SUBAGENTS: usize = 3;
/// Default max LLM turns for a subagent run.
pub const DEFAULT_SUBAGENT_MAX_STEPS: u32 = 8;
/// Default wall-clock timeout per subagent (300s = 5 min).
pub const DEFAULT_SUBAGENT_TIMEOUT_SECS: u64 = 300;

// ── Subagent Types ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubagentType {
	Explore,
	GeneralPurpose,
	Orchestrator,
}

impl SubagentType {
	pub fn name(self) -> &'static str {
		match self {
			Self::Explore => "explore",
			Self::GeneralPurpose => "general-purpose",
			Self::Orchestrator => "orchestrator",
		}
	}

	pub fn from_str(s: &str) -> Option<Self> {
		match s.trim().to_ascii_lowercase().as_str() {
			"explore" | "explorer" | "research" => Some(Self::Explore),
			"general-purpose" | "general" | "worker" | "gp" => Some(Self::GeneralPurpose),
			"orchestrator" | "lead" | "planner" => Some(Self::Orchestrator),
			_ => None,
		}
	}

	pub fn description(self) -> &'static str {
		match self {
			Self::Explore => {
				"Fast read-only research: find files, search code, answer codebase questions."
			}
			Self::GeneralPurpose => {
				"Multi-step worker: explore and act; return a concise result. No nested tasks."
			}
			Self::Orchestrator => "Decompose work and route; may nest one level of workers if allowed.",
		}
	}

	pub fn system_prompt(self) -> &'static str {
		match self {
			Self::Explore => concat!(
				"You are an explore subagent. Read-only: use read/glob/grep/list/shell (status only). ",
				"Do not write or edit files. Return paths, findings, and citations. Be concise."
			),
			Self::GeneralPurpose => concat!(
				"You are a general-purpose subagent. Complete the delegated task autonomously. ",
				"Use tools as needed. Do NOT call the task tool. Do not ask for clarification. ",
				"Return: summary, key findings, files touched, issues."
			),
			Self::Orchestrator => concat!(
				"You are an orchestrator subagent. Break the goal into clear subtasks, ",
				"prefer describing a plan over re-doing parent work. Keep output actionable."
			),
		}
	}

	pub fn tool_allowlist(self) -> Option<&'static [ToolKind]> {
		match self {
			Self::Explore => {
				Some(&[ToolKind::Read, ToolKind::Glob, ToolKind::Grep, ToolKind::List, ToolKind::Shell])
			}
			Self::GeneralPurpose => None,
			Self::Orchestrator => None,
		}
	}

	pub fn max_steps(self) -> u32 {
		match self {
			Self::Explore => 6,
			Self::GeneralPurpose => DEFAULT_SUBAGENT_MAX_STEPS,
			Self::Orchestrator => 6,
		}
	}

	pub fn timeout_secs(self) -> u64 {
		match self {
			Self::Explore => 120,
			Self::GeneralPurpose => DEFAULT_SUBAGENT_TIMEOUT_SECS,
			Self::Orchestrator => 180,
		}
	}
}

// ── Structured Status ────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubagentStatus {
	Pending,
	Running,
	Completed,
	Failed,
	TimedOut,
	Cancelled,
}

impl SubagentStatus {
	pub fn glyph(self) -> &'static str {
		match self {
			Self::Pending => "○",
			Self::Running => "●",
			Self::Completed => "✓",
			Self::Failed => "✗",
			Self::TimedOut => "⏱",
			Self::Cancelled => "⊘",
		}
	}

	pub fn is_terminal(self) -> bool {
		matches!(self, Self::Completed | Self::Failed | Self::TimedOut | Self::Cancelled)
	}

	pub fn label(self) -> &'static str {
		match self {
			Self::Pending => "pending",
			Self::Running => "running",
			Self::Completed => "completed",
			Self::Failed => "failed",
			Self::TimedOut => "timed_out",
			Self::Cancelled => "cancelled",
		}
	}
}

// ── Subagent Result ──────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct SubagentResult {
	pub task_id: String,
	pub subagent_type: String,
	pub description: String,
	pub status: SubagentStatus,
	pub output: String,
	pub error: Option<String>,
	pub steps: u32,
	pub started_at: Instant,
	pub completed_at: Option<Instant>,
	pub token_estimate: usize,
	pub prompt_tokens: usize,
	pub completion_tokens: usize,
}

impl SubagentResult {
	pub fn duration(&self) -> Duration {
		self.completed_at.map(|c| c.duration_since(self.started_at)).unwrap_or_default()
	}

	pub fn summary_line(&self) -> String {
		let dur = self.duration();
		let ms = dur.as_millis();
		format!(
			"{} {} · {} · {}ms · {} steps",
			self.status.glyph(),
			self.subagent_type,
			self.description.chars().take(40).collect::<String>(),
			ms,
			self.steps,
		)
	}
}

// ── Subagent Config ──────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct SubagentConfig {
	pub name: String,
	pub description: String,
	pub system_prompt: String,
	pub model: Option<String>,
	pub max_steps: u32,
	pub timeout_secs: u64,
	pub allowlist: Option<Vec<ToolKind>>,
}

impl SubagentConfig {
	pub fn builtin(kind: SubagentType) -> Self {
		Self {
			name: kind.name().to_string(),
			description: kind.description().to_string(),
			system_prompt: kind.system_prompt().to_string(),
			model: None,
			max_steps: kind.max_steps(),
			timeout_secs: kind.timeout_secs(),
			allowlist: kind.tool_allowlist().map(|s| s.to_vec()),
		}
	}
}

// ── Delegation Entry ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct DelegationEntry {
	pub task_id: String,
	pub subagent_type: String,
	pub description: String,
	pub status: SubagentStatus,
	pub summary: String,
	pub started: Instant,
}

// ── Delegation Ledger ────────────────────────────────────────────────────

use std::sync::Mutex;

#[derive(Debug, Default)]
struct LedgerInner {
	entries: Vec<DelegationEntry>,
}

#[derive(Clone, Default)]
pub struct DelegationLedger {
	inner: Arc<Mutex<LedgerInner>>,
}

impl DelegationLedger {
	pub fn new() -> Self {
		Self::default()
	}

	pub fn clear(&self) {
		if let Ok(mut g) = self.inner.lock() {
			g.entries.clear();
		}
	}

	pub fn upsert_running(&self, task_id: &str, kind: &str, description: &str) {
		if let Ok(mut g) = self.inner.lock() {
			if let Some(e) = g.entries.iter_mut().find(|e| e.task_id == task_id) {
				e.status = SubagentStatus::Running;
				return;
			}
			g.entries.push(DelegationEntry {
				task_id: task_id.into(),
				subagent_type: kind.into(),
				description: description.chars().take(120).collect(),
				status: SubagentStatus::Running,
				summary: String::new(),
				started: Instant::now(),
			});
		}
	}

	pub fn complete(&self, task_id: &str, status: SubagentStatus, summary: &str) {
		if let Ok(mut g) = self.inner.lock()
			&& let Some(e) = g.entries.iter_mut().find(|e| e.task_id == task_id)
		{
			e.status = status;
			e.summary = summary.chars().take(400).collect();
		}
	}

	pub fn reminder(&self) -> String {
		let Ok(g) = self.inner.lock() else {
			return String::new();
		};
		if g.entries.is_empty() {
			return String::new();
		}
		let mut lines = vec![
			"<system-reminder>".to_string(),
			"<delegated_subtasks>".to_string(),
			"You have already delegated these subtasks. Do NOT re-delegate the same work:".to_string(),
		];
		for e in g.entries.iter().rev().take(12) {
			lines.push(format!(
				"- [{}] {} {} — {}",
				e.status.label(),
				e.subagent_type,
				e.description,
				e.summary.chars().take(60).collect::<String>()
			));
		}
		lines.push("</delegated_subtasks>".to_string());
		lines.push("</system-reminder>".to_string());
		lines.join("\n")
	}

	pub fn active_count(&self) -> usize {
		self
			.inner
			.lock()
			.map(|g| g.entries.iter().filter(|e| e.status == SubagentStatus::Running).count())
			.unwrap_or(0)
	}

	#[allow(dead_code)]
	pub fn snapshot(&self) -> Vec<DelegationEntry> {
		self.inner.lock().map(|g| g.entries.clone()).unwrap_or_default()
	}

	/// Persist delegation ledger to disk (survives session restarts).
	pub fn save_to_disk(&self, path: &std::path::Path) {
		let entries = self.snapshot();
		let json = serde_json::json!({
			"entries": entries.iter().map(|e| serde_json::json!({
				"task_id": e.task_id,
				"subagent_type": e.subagent_type,
				"description": e.description,
				"status": e.status.label(),
				"summary": e.summary,
			})).collect::<Vec<_>>(),
		});
		if let Ok(text) = serde_json::to_string_pretty(&json) {
			let _ = std::fs::write(path, &text);
		}
	}

	/// Load delegation ledger from disk.
	pub fn load_from_disk(path: &std::path::Path) -> Self {
		let ledger = Self::new();
		let text = match std::fs::read_to_string(path) {
			Ok(t) => t,
			Err(_) => return ledger,
		};
		let json: serde_json::Value = match serde_json::from_str(&text) {
			Ok(v) => v,
			Err(_) => return ledger,
		};
		if let Some(arr) = json.get("entries").and_then(|v| v.as_array())
			&& let Ok(mut g) = ledger.inner.lock()
		{
			for item in arr {
				let task_id = item.get("task_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
				let subagent_type =
					item.get("subagent_type").and_then(|v| v.as_str()).unwrap_or("").to_string();
				let description =
					item.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string();
				let status_str = item.get("status").and_then(|v| v.as_str()).unwrap_or("completed");
				let status = match status_str {
					"running" => SubagentStatus::Running,
					"failed" => SubagentStatus::Failed,
					_ => SubagentStatus::Completed,
				};
				let summary = item.get("summary").and_then(|v| v.as_str()).unwrap_or("").to_string();
				g.entries.push(DelegationEntry {
					task_id,
					subagent_type,
					description,
					status,
					summary,
					started: Instant::now(),
				});
			}
		}
		ledger
	}
}

/// All built-in subagent types.
pub const SUBTYPES: &[SubagentType] =
	&[SubagentType::Explore, SubagentType::GeneralPurpose, SubagentType::Orchestrator];

/// Build task tool schema with optional custom subagent types.
pub fn task_tool_schema_with_custom(custom_names: &[String]) -> Value {
	let mut enum_vals: Vec<Value> = SUBTYPES.iter().map(|s| json!(s.name())).collect();
	for name in custom_names {
		enum_vals.push(json!(name));
	}
	json!({
		"type": "function",
		"function": {
			"name": "task",
			"description": "Delegate a multi-step unit of work to a subagent. Use for parallel research or isolated work.",
			"parameters": {
				"type": "object",
				"properties": {
					"description": { "type": "string", "description": "Short 3-5 word task title" },
					"prompt": { "type": "string", "description": "Full instructions for the subagent" },
					"subagent_type": {
						"type": "string",
						"enum": enum_vals,
						"description": "Which specialist to spawn"
					}
				},
				"required": ["prompt"]
			}
		}
	})
}

// ── Task Tool Schema ─────────────────────────────────────────────────────

pub fn task_tool_schema() -> Value {
	json!({
		"type": "function",
		"function": {
			"name": "task",
			"description": "Delegate a multi-step unit of work to a subagent. Use for parallel research or isolated work. Types: explore, general-purpose, orchestrator.",
			"parameters": {
				"type": "object",
				"properties": {
					"description": { "type": "string", "description": "Short 3-5 word task title" },
					"prompt": { "type": "string", "description": "Full instructions for the subagent" },
					"subagent_type": {
						"type": "string",
						"enum": ["explore", "general-purpose", "orchestrator"],
						"description": "Which specialist to spawn"
					}
				},
				"required": ["prompt"]
			}
		}
	})
}

// ── Parallel LLM-Powered Subagent Execution ──────────────────────────────

/// Run a subagent with a real LLM call, streaming output through `tx`.
pub async fn run_subagent_llm(
	config: &SubagentConfig,
	prompt: &str,
	model: &str,
	cwd: &Path,
	api_url: Option<&str>,
	tx: std::sync::mpsc::Sender<String>,
	cancel: CancellationToken,
) -> SubagentResult {
	let task_id = format!("sub_{}", Instant::now().elapsed().as_millis());
	let started_at = Instant::now();

	// System prompt for this subagent type
	let system = config.system_prompt.clone();
	let history = vec![("user".to_string(), prompt.to_string())];

	// Build messages with system prompt
	let mut messages = vec![zen::ApiMessage::system(system)];
	for (role, content) in &history {
		messages.push(zen::ApiMessage {
			role: role.clone(),
			content: Some(content.clone()),
			tool_call_id: None,
			tool_calls: None,
			name: None,
		});
	}

	// Build tool schemas based on allowlist or all tools
	let tools = config.allowlist.as_ref().map(|wl| {
		let all = crate::tools::openai_tool_schemas(AgentMode::Agent);
		if wl.is_empty() {
			return all;
		}
		all
			.into_iter()
			.filter(|t| {
				t.get("function")
					.and_then(|f| f.get("name"))
					.and_then(|n| n.as_str())
					.map(|name| wl.iter().any(|k| k.name() == name))
					.unwrap_or(false)
			})
			.collect()
	});

	let mut combined_output = String::new();
	let mut step_count = 0u32;
	let max_steps = config.max_steps;
	let mut total_prompt_tokens = 0usize;
	let mut total_completion_tokens = 0usize;

	let tx_clone = tx.clone();
	let _ = tx_clone.send(format!("<subagent name=\"{}\">\n", config.name));

	for step in 1..=max_steps {
		if cancel.is_cancelled() {
			let _ = tx_clone.send("\n</subagent>\n".to_string());
			return SubagentResult {
				task_id,
				subagent_type: config.name.clone(),
				description: prompt.chars().take(120).collect(),
				status: SubagentStatus::Cancelled,
				output: combined_output,
				error: Some("cancelled by user or parent".into()),
				steps: step_count,
				started_at,
				completed_at: Some(Instant::now()),
				token_estimate: total_prompt_tokens + total_completion_tokens,
				prompt_tokens: total_prompt_tokens,
				completion_tokens: total_completion_tokens,
			};
		}

		let turn = tokio::time::timeout(
			Duration::from_secs(config.timeout_secs),
			zen::stream_chat_messages(model, &messages, tools.as_deref(), api_url, tx_clone.clone()),
		)
		.await;

		let turn = match turn {
			Ok(Ok(t)) => t,
			Ok(Err(e)) => {
				let _ = tx_clone.send(format!("\n*subagent error: {e}*\n"));
				let _ = tx_clone.send("\n</subagent>\n".to_string());
				return SubagentResult {
					task_id,
					subagent_type: config.name.clone(),
					description: prompt.chars().take(120).collect(),
					status: SubagentStatus::Failed,
					output: combined_output,
					error: Some(e.to_string()),
					steps: step_count,
					started_at,
					completed_at: Some(Instant::now()),
					token_estimate: total_prompt_tokens + total_completion_tokens,
					prompt_tokens: total_prompt_tokens,
					completion_tokens: total_completion_tokens,
				};
			}
			Err(_) => {
				let _ = tx_clone.send("\n*subagent timed out*\n".to_string());
				let _ = tx_clone.send("\n</subagent>\n".to_string());
				return SubagentResult {
					task_id,
					subagent_type: config.name.clone(),
					description: prompt.chars().take(120).collect(),
					status: SubagentStatus::TimedOut,
					output: combined_output,
					error: Some(format!("timed out after {}s", config.timeout_secs)),
					steps: step_count,
					started_at,
					completed_at: Some(Instant::now()),
					token_estimate: total_prompt_tokens + total_completion_tokens,
					prompt_tokens: total_prompt_tokens,
					completion_tokens: total_completion_tokens,
				};
			}
		};

		combined_output.push_str(&turn.text);
		total_prompt_tokens = total_prompt_tokens.max(turn.token_usage.prompt_tokens);
		total_completion_tokens = total_completion_tokens.max(turn.token_usage.completion_tokens);

		if turn.tool_calls.is_empty() {
			let _ = tx_clone.send("\n</subagent>\n".to_string());
			return SubagentResult {
				task_id,
				subagent_type: config.name.clone(),
				description: prompt.chars().take(120).collect(),
				status: SubagentStatus::Completed,
				output: combined_output,
				error: None,
				steps: step_count,
				started_at,
				completed_at: Some(Instant::now()),
				token_estimate: total_prompt_tokens + total_completion_tokens,
				prompt_tokens: total_prompt_tokens,
				completion_tokens: total_completion_tokens,
			};
		}

		let mut tool_messages = Vec::new();
		for call in &turn.tool_calls {
			if cancel.is_cancelled() {
				break;
			}

			let kind = ToolKind::from_name(&call.name);
			let args: Value = serde_json::from_str(&call.arguments).unwrap_or(json!({}));
			let preview = tool_preview(kind, &args, &call.arguments);

			// Nested under `<subagent>` — use stable ids so UI can upgrade correctly.
			let _ =
				tx_clone.send(crate::tools::format_tool_running_id(&call.name, &preview, Some(&call.id)));

			if let Some(wl) = &config.allowlist
				&& !wl.is_empty()
				&& let Some(k) = kind
				&& !wl.contains(&k)
			{
				let result = ToolResult {
					call_id: call.id.clone(),
					name: call.name.clone(),
					ok: false,
					title: format!("{} not in subagent allowlist", call.name),
					output: "This tool is not available for this subagent type.".into(),
					preview: preview.clone(),
				};
				let _ = tx_clone.send(format_tool_result(&result));
				tool_messages.push(tool_api_message(call, &result, step));
				continue;
			}

			let result = exec_spawn(&call.name, &args, kind, cwd, AgentMode::Agent, false).await;
			let _ = tx_clone.send(format_tool_result(&result));
			tool_messages.push(tool_api_message(call, &result, step));
			combined_output.push_str(&format!("[{}] {}\n", result.name, result.title));
			step_count += 1;
		}

		messages.push(zen::ApiMessage {
			role: "assistant".into(),
			content: Some(turn.text.clone()),
			tool_call_id: None,
			tool_calls: Some(
				turn
					.tool_calls
					.iter()
					.map(|c| zen::ToolCallDelta {
						id: c.id.clone(),
						name: c.name.clone(),
						arguments: c.arguments.clone(),
					})
					.collect(),
			),
			name: None,
		});

		for tm in tool_messages {
			messages.push(tm);
		}
	}

	let _ = tx_clone.send("\n</subagent>\n".to_string());

	SubagentResult {
		task_id,
		subagent_type: config.name.clone(),
		description: prompt.chars().take(120).collect(),
		status: SubagentStatus::Completed,
		output: combined_output,
		error: None,
		steps: step_count,
		started_at,
		completed_at: Some(Instant::now()),
		token_estimate: total_prompt_tokens + total_completion_tokens,
		prompt_tokens: total_prompt_tokens,
		completion_tokens: total_completion_tokens,
	}
}

/// Spawn tool execution on blocking thread.
async fn exec_spawn(
	name: &str,
	args: &Value,
	kind: Option<ToolKind>,
	cwd: &Path,
	mode: AgentMode,
	plan_shell: bool,
) -> ToolResult {
	let tool_name = name.to_string();
	let task_id = format!("sub_{}", Instant::now().elapsed().as_millis());
	let call = if kind == Some(ToolKind::Shell) {
		let cmd = args.get("command").and_then(|v| v.as_str()).unwrap_or("");
		ToolCall {
			id: task_id.clone(),
			name: tool_name.clone(),
			arguments: json!({"command": cmd}).to_string(),
		}
	} else {
		ToolCall { id: task_id.clone(), name: tool_name.clone(), arguments: args.to_string() }
	};
	let cwd = cwd.to_path_buf();
	let res_name = tool_name.clone();
	tokio::task::spawn_blocking(move || exec_tool(&call, &cwd, mode, plan_shell))
		.await
		.unwrap_or_else(|_| ToolResult {
			call_id: task_id.clone(),
			name: tool_name.clone(),
			ok: false,
			title: format!("{res_name} join error"),
			output: "thread join failed".into(),
			preview: String::new(),
		})
}

// ── Tool Preview ─────────────────────────────────────────────────────────

fn tool_preview(kind: Option<ToolKind>, args: &Value, raw: &str) -> String {
	let from_args = |keys: &[&str]| {
		for k in keys {
			if let Some(s) = args.get(*k).and_then(|v| v.as_str()) {
				return s.chars().take(72).collect::<String>();
			}
		}
		String::new()
	};
	match kind {
		Some(ToolKind::Shell) => from_args(&["command", "cmd"]),
		Some(ToolKind::Read) | Some(ToolKind::Write) | Some(ToolKind::Edit) => {
			from_args(&["path", "filePath", "file_path"])
		}
		Some(ToolKind::Glob) => from_args(&["pattern"]),
		Some(ToolKind::Grep) => from_args(&["pattern"]),
		Some(ToolKind::List) => {
			let p = from_args(&["path"]);
			if p.is_empty() { ".".into() } else { p }
		}
		Some(ToolKind::Question) => from_args(&["prompt", "question"]),
		Some(ToolKind::Task) => {
			let d = from_args(&["description", "prompt", "task"]);
			if d.is_empty() { "delegate".into() } else { d }
		}
		Some(ToolKind::SkillManage) => {
			let a = from_args(&["action", "name"]);
			if a.is_empty() { "skill".into() } else { a }
		}
		Some(ToolKind::TodoWrite) => {
			let todos = args.get("todos").and_then(|v| v.as_array());
			match todos {
				Some(arr) => format!("{} items", arr.len()),
				None => "todos".into(),
			}
		}
		Some(ToolKind::Memory) => {
			let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("list");
			let target = args.get("target").and_then(|v| v.as_str()).unwrap_or("memory");
			format!("{action} {target}")
		}
		Some(ToolKind::McpTool) => raw.chars().take(48).collect(),
		Some(ToolKind::WebFetch) => from_args(&["url"]),
		Some(ToolKind::WebSearch) => from_args(&["query"]),
		Some(ToolKind::ApplyPatch) => from_args(&["path"]),
		Some(
			ToolKind::GoToDefinition
			| ToolKind::FindReferences
			| ToolKind::Hover
			| ToolKind::DocumentSymbols
			| ToolKind::WorkspaceSymbols
			| ToolKind::GoToImplementation
			| ToolKind::CallHierarchy
			| ToolKind::FormatCode
			| ToolKind::GetDiagnostics
			| ToolKind::CompleteCode,
		) => from_args(&["path", "query"]),
		None => raw.chars().take(48).collect(),
	}
}

// ── Execute Task Tool (called from agent loop) ───────────────────────────

pub fn execute_task_tool(
	call: &ToolCall,
	_cwd: &Path,
	ledger: &DelegationLedger,
	allow_orchestrator_nest: bool,
) -> ToolResult {
	if ledger.active_count() >= MAX_CONCURRENT_SUBAGENTS {
		return ToolResult {
			call_id: call.id.clone(),
			name: "task".into(),
			ok: false,
			title: "task · concurrency limit".into(),
			output: format!(
				"MAX_CONCURRENT_SUBAGENTS={MAX_CONCURRENT_SUBAGENTS} reached. Wait for running tasks."
			),
			preview: "limit".into(),
		};
	}

	let args: Value = serde_json::from_str(&call.arguments).unwrap_or(json!({}));
	let prompt = args
		.get("prompt")
		.or_else(|| args.get("task"))
		.and_then(|v| v.as_str())
		.unwrap_or("")
		.to_string();
	if prompt.is_empty() {
		return ToolResult {
			call_id: call.id.clone(),
			name: "task".into(),
			ok: false,
			title: "task · missing prompt".into(),
			output: "Need `prompt` for the subagent.".into(),
			preview: String::new(),
		};
	}
	let desc = args.get("description").and_then(|v| v.as_str()).unwrap_or("delegated").to_string();
	let kind = args
		.get("subagent_type")
		.or_else(|| args.get("type"))
		.and_then(|v| v.as_str())
		.and_then(SubagentType::from_str)
		.unwrap_or(SubagentType::GeneralPurpose);

	if kind == SubagentType::Orchestrator && !allow_orchestrator_nest {
		return ToolResult {
			call_id: call.id.clone(),
			name: "task".into(),
			ok: false,
			title: "task · orchestrator denied".into(),
			output: "Nested orchestrator disabled. Use explore or general-purpose.".into(),
			preview: "denied".into(),
		};
	}

	let task_id = if call.id.is_empty() {
		format!("task_{}", Instant::now().elapsed().as_millis())
	} else {
		call.id.clone()
	};

	ledger.upsert_running(&task_id, kind.name(), &desc);
	ledger.complete(&task_id, SubagentStatus::Completed, &desc);
	ToolResult {
		call_id: task_id,
		name: "task".into(),
		ok: true,
		title: format!("task · {desc}"),
		output: format!(
			"Delegated to {} subagent. The LLM-powered subagent will work in parallel. \
			 Results will stream as they arrive.\nPrompt: {}",
			kind.name(),
			prompt.chars().take(200).collect::<String>()
		),
		preview: kind.name().into(),
	}
}

fn tool_api_message(call: &ToolCall, result: &ToolResult, step: u32) -> zen::ApiMessage {
	zen::ApiMessage {
		role: "tool".into(),
		content: Some(format!(
			"[{}] {}\n{}",
			if result.ok { "ok" } else { "error" },
			result.title,
			result.output
		)),
		tool_call_id: Some(if call.id.is_empty() { format!("call_{step}") } else { call.id.clone() }),
		tool_calls: None,
		name: Some(result.name.clone()),
	}
}

// ── Guidance ─────────────────────────────────────────────────────────────

pub fn orchestration_guidance() -> &'static str {
	r#"# Multi-agent orchestration
You may delegate with the `task` tool:
- explore — read-only research (files, grep, layout)
- general-purpose — multi-step worker (no nested task)
- orchestrator — decompose only (rarely needed)

Rules:
- Prefer doing simple work yourself; delegate only multi-step or parallel research.
- Pass a full `prompt` with context; subagents cannot see your full history.
- Do not re-delegate completed identical work (see delegation ledger).
- Max concurrent subagents: 3.
- Leaf workers cannot spawn further tasks."#
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn parse_types() {
		assert_eq!(SubagentType::from_str("explore"), Some(SubagentType::Explore));
		assert_eq!(SubagentType::from_str("general-purpose"), Some(SubagentType::GeneralPurpose));
	}

	#[test]
	fn ledger_reminder() {
		let l = DelegationLedger::new();
		l.upsert_running("t1", "explore", "find auth");
		l.complete("t1", SubagentStatus::Completed, "found src/auth.rs");
		let r = l.reminder();
		assert!(r.contains("completed"));
		assert!(r.contains("explore"));
	}

	#[test]
	fn subagent_status_lifecycle() {
		assert!(!SubagentStatus::Pending.is_terminal());
		assert!(!SubagentStatus::Running.is_terminal());
		assert!(SubagentStatus::Completed.is_terminal());
		assert!(SubagentStatus::Failed.is_terminal());
		assert!(SubagentStatus::TimedOut.is_terminal());
		assert!(SubagentStatus::Cancelled.is_terminal());
	}
}
