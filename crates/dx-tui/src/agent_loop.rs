//! Multi-step agent loop: model → tools → model until done.
//!
//! Mirrors OpenCode `SessionPrompt.loop` semantics for the Ratatui shell:
//! keep calling the model while tool calls (native or recovered markdown) remain.

use std::{path::PathBuf, time::Duration};

use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::sync::mpsc::Sender;
use tokio_util::sync::CancellationToken;

use crate::{
	modes::AgentMode,
	orchestration::DelegationLedger,
	orchestration::{SubagentConfig, SubagentType, run_subagent_llm},
	permission_hub::{self, PermissionHub},
	question_hub::QuestionHub,
	sidebar_data::{SidebarState, TaskItem, TaskStatus},
	tools::{
		self, PermissionDecision, ToolCall, ToolKind, ToolResult, extract_markdown_tool_calls,
		format_context_group_summary, format_tool_result, format_tool_running, needs_permission,
		tool_message_content,
	},
	zen::{self, ChatTurn, ToolCallDelta},
};

/// Safety caps.
const MAX_STEPS: u32 = 24;
const DOOM_SAME_TOOL: u32 = 4;
const PERM_TIMEOUT: Duration = Duration::from_secs(180);
const QUESTION_TIMEOUT: Duration = Duration::from_secs(300);

pub struct LoopInput {
	pub model: String,
	pub system: String,
	/// Prior turns as (role, content). Last user message already included.
	pub history: Vec<(String, String)>,
	pub mode: AgentMode,
	pub cwd: PathBuf,
	pub plan_allow_shell: bool,
	pub api_url: Option<String>,
	/// When false, skip native tools array (still recover markdown tools).
	pub enable_native_tools: bool,
	pub max_steps: u32,
	pub permission: Option<PermissionHub>,
	pub questions: Option<QuestionHub>,
	/// Multi-agent delegation ledger (Agent/Goal).
	pub ledger: Option<DelegationLedger>,
	/// Sidebar state for updating session todos/tasks.
	pub sidebar: Option<SidebarState>,
}

/// Run until the model stops requesting tools or caps hit.
pub async fn run(input: LoopInput, tx: Sender<String>) -> Result<()> {
	let max_steps = if input.max_steps == 0 { MAX_STEPS } else { input.max_steps.min(MAX_STEPS) };

	let mut messages: Vec<zen::ApiMessage> = Vec::new();
	let mut system = input.system.clone();
	if let Some(ref ledger) = input.ledger {
		let rem = ledger.reminder();
		if !rem.is_empty() {
			system.push_str("\n\n");
			system.push_str(&rem);
		}
	}
	if !system.trim().is_empty() {
		messages.push(zen::ApiMessage::system(system));
	}
	for (role, content) in &input.history {
		if role == "system" {
			continue;
		}
		messages.push(zen::ApiMessage {
			role: role.clone(),
			content: Some(content.clone()),
			tool_call_id: None,
			tool_calls: None,
			name: None,
		});
	}

	// Native tools can be disabled mid-loop if the provider rejects them.
	let mut native_tools = input.enable_native_tools;
	let tool_schemas = tools::openai_tool_schemas(input.mode);

	let mut last_signature = String::new();
	let mut same_sig_count = 0u32;
	let mut always_allow: Vec<String> = Vec::new();
	// For context-tool grouping summary
	let mut ctx_counts: Vec<(ToolKind, u32)> =
		vec![(ToolKind::Read, 0), (ToolKind::Grep, 0), (ToolKind::Glob, 0), (ToolKind::List, 0)];

	for step in 1..=max_steps {
		tracing::debug!(step, mode = %input.mode.label(), native_tools, "agent_loop step");

		let tools_arg =
			if native_tools && !tool_schemas.is_empty() { Some(tool_schemas.as_slice()) } else { None };

		let mut turn = match zen::stream_chat_messages(
			&input.model,
			&messages,
			tools_arg,
			input.api_url.as_deref(),
			tx.clone(),
		)
		.await
		{
			Ok(t) => t,
			Err(e) => {
				// If native tools blew up mid-loop, drop them and retry this step once.
				if native_tools {
					tracing::warn!("stream failed with tools ({e}); disabling native tools and retrying");
					native_tools = false;
					let _ =
						tx.send("\n*Provider rejected tools — continuing with recovered tool steps…*\n".into());
					match zen::stream_chat_messages(
						&input.model,
						&messages,
						None,
						input.api_url.as_deref(),
						tx.clone(),
					)
					.await
					{
						Ok(t) => t,
						Err(e2) => {
							return Err(e2).with_context(|| format!("agent loop step {step}"));
						}
					}
				} else {
					return Err(e).with_context(|| format!("agent loop step {step}"));
				}
			}
		};

		if turn.tools_disabled {
			native_tools = false;
		}

		// If the provider streamed a turn with zero tools while we offered tools,
		// still try markdown recovery (many free models ignore tool_choice).
		let mut calls = std::mem::take(&mut turn.tool_calls);
		let recovered = if calls.is_empty() {
			extract_markdown_tool_calls(&turn.text, input.mode)
		} else {
			Vec::new()
		};
		let is_recovered = !recovered.is_empty();
		if is_recovered {
			let names: Vec<&str> = recovered.iter().map(|c| c.name.as_str()).collect();
			let _ =
				tx.send(format!("\n*Running {} tool step(s): {}…*\n", recovered.len(), names.join(", ")));
			calls = recovered;
			// Markdown-recovered tools must NOT use OpenAI tool-message protocol
			// (assistant never emitted native tool_calls — that breaks the next turn).
			native_tools = false;
		}

		if calls.is_empty() {
			// Flush context group summary if we accumulated any this turn.
			emit_context_summary(&tx, &ctx_counts);
			messages.push(assistant_api_message(&turn));
			return Ok(());
		}

		// Guarantee every call has a stable id (native streams sometimes omit ids)
		for (i, c) in calls.iter_mut().enumerate() {
			if c.id.is_empty() {
				c.id = format!("call_{step}_{i}");
			}
		}

		// History: native tool_calls only when the model actually emitted them.
		// Recovered tools keep plain assistant text + later user-role tool results.
		let mut turn_for_hist = turn.clone();
		if native_tools && !is_recovered {
			turn_for_hist.tool_calls = calls.clone();
		} else {
			turn_for_hist.tool_calls = Vec::new();
		}
		messages.push(assistant_api_message(&turn_for_hist));

		let sig = calls
			.iter()
			.map(|c| format!("{}:{}", c.name, c.arguments.chars().take(120).collect::<String>()))
			.collect::<Vec<_>>()
			.join("|");
		if sig == last_signature {
			same_sig_count += 1;
			if same_sig_count >= DOOM_SAME_TOOL {
				let _ = tx.send("\n*Stopped: repeated the same tool calls (doom-loop guard).*\n".into());
				return Ok(());
			}
		} else {
			same_sig_count = 0;
			last_signature = sig;
		}

		// Use OpenAI tool role only for genuine native tool_calls.
		let use_native_tool_msgs = native_tools && !is_recovered;

		// ── Parallel tool dispatch ──────────────────────────────────────
		// Collect into owned vecs first so we don't borrow `calls` (which doesn't live long enough for tokio::spawn)
		let (sequential, parallel_calls): (Vec<_>, Vec<_>) = calls.clone().into_iter().partition(|c| {
			let k = ToolKind::from_name(&c.name);
			matches!(
				k,
				Some(ToolKind::Question | ToolKind::Task | ToolKind::SkillManage | ToolKind::TodoWrite)
			)
		});

		for call in &sequential {
			let kind = ToolKind::from_name(&call.name);
			let args: Value =
				serde_json::from_str(&call.arguments).unwrap_or_else(|_| serde_json::json!({}));
			let preview = tool_preview(kind, &args, &call.arguments);

			match kind {
				Some(ToolKind::Question) => {
					let result = run_question(call, &args, &input.questions, &tx).await;
					let _ = tx.send(format_tool_result(&result));
					push_tool_result(&mut messages, call, &result, step, use_native_tool_msgs);
				}
				Some(ToolKind::TodoWrite) => {
					let _ = tx.send(format_tool_running("todowrite", &preview));
					let todos: Vec<TaskItem> = args
						.get("todos")
						.and_then(|v| v.as_array())
						.map(|arr| {
							arr
								.iter()
								.filter_map(|item| {
									let content = item.get("content")?.as_str()?.to_string();
									let status =
										match item.get("status").and_then(|s| s.as_str()).unwrap_or("pending") {
											"in_progress" => TaskStatus::InProgress,
											"completed" | "done" => TaskStatus::Done,
											"cancelled" => TaskStatus::Cancelled,
											_ => TaskStatus::Pending,
										};
									Some(TaskItem { content, status })
								})
								.collect()
						})
						.unwrap_or_default();
					let todo_count = todos.len();
					let todo_output = todos
						.iter()
						.map(|t| format!("{} {}", t.status.glyph(), t.content))
						.collect::<Vec<_>>()
						.join("\n");
					if let Some(ref side) = input.sidebar {
						side.apply_todo_list(todos);
					}
					let result = ToolResult {
						call_id: call.id.clone(),
						name: "todowrite".into(),
						ok: true,
						title: format!("Todos · {} items", todo_count),
						output: todo_output,
						preview: format!("{} todos", todo_count),
					};
					let _ = tx.send(format_tool_result(&result));
					push_tool_result(&mut messages, call, &result, step, use_native_tool_msgs);
				}
				Some(ToolKind::Task) => {
					// Nested subagent: stream under `<subagent>` (tools become child cards).
					// Await completion so the model receives real output, not a fake placeholder.
					let ledger = input.ledger.clone().unwrap_or_default();
					let model = input.model.clone();
					let cwd = input.cwd.clone();
					let api_url = input.api_url.clone();
					let tx_c = tx.clone();
					let cancel = CancellationToken::new();
					let sub_type_name = args
						.get("subagent_type")
						.and_then(|v| v.as_str())
						.unwrap_or("general-purpose")
						.to_string();
					let custom_registry = crate::subagent_registry::load_custom_subagents();
					let config = crate::subagent_registry::resolve_subagent(&sub_type_name, &custom_registry)
						.unwrap_or_else(|| SubagentConfig::builtin(SubagentType::GeneralPurpose));
					let config_name = config.name.clone();
					let prompt = args.get("prompt").and_then(|v| v.as_str()).unwrap_or("").to_string();
					let desc =
						args.get("description").and_then(|v| v.as_str()).unwrap_or("delegated").to_string();

					let _ = tx.send(
						crate::stream_events::StreamEvent::SubagentMeta {
							name: config_name.clone(),
							status: "running".into(),
						}
						.encode(),
					);

					let result =
						run_subagent_llm(&config, &prompt, &model, &cwd, api_url.as_deref(), tx_c, cancel)
							.await;
					let ok = result.status == crate::orchestration::SubagentStatus::Completed;
					ledger.complete(
						&result.task_id,
						if ok {
							crate::orchestration::SubagentStatus::Completed
						} else {
							crate::orchestration::SubagentStatus::Failed
						},
						&format!("{} · {}ms", result.status.label(), result.duration().as_millis()),
					);
					let _ = tx.send(
						crate::stream_events::StreamEvent::SubagentMeta {
							name: config_name.clone(),
							status: if ok { "done".into() } else { "failed".into() },
						}
						.encode(),
					);
					let tool_result = ToolResult {
						call_id: call.id.clone(),
						name: "task".into(),
						ok,
						title: format!("Task · {desc} · {config_name}"),
						output: if result.output.trim().is_empty() {
							format!(
								"Subagent {} · {} · {} steps",
								config_name,
								result.status.label(),
								result.steps
							)
						} else {
							result.output.chars().take(12_000).collect()
						},
						preview: config_name,
					};
					// Don't double-paint as a top-level tool fence — subagent block already streamed.
					// Still feed the model the real result.
					push_tool_result(&mut messages, call, &tool_result, step, use_native_tool_msgs);
				}
				Some(ToolKind::SkillManage) => {
					let _ = tx.send(format_tool_running("skill_manage", &preview));
					let call_c = call.clone();
					let result =
						tokio::task::spawn_blocking(move || crate::skills::execute_skill_manage(&call_c))
							.await
							.unwrap_or_else(|e| ToolResult {
								call_id: call.id.clone(),
								name: "skill_manage".into(),
								ok: false,
								title: "skill join error".into(),
								output: e.to_string(),
								preview: preview.clone(),
							});
					let _ = tx.send(format_tool_result(&result));
					push_tool_result(&mut messages, call, &result, step, use_native_tool_msgs);
				}
				_ => {}
			}
		}

		// Parallel: regular tools (shell, read, write, edit, glob, grep, list)
		use futures::future::join_all;
		let mut handles = Vec::new();
		for call in parallel_calls.iter() {
			let kind = ToolKind::from_name(&call.name);
			let args: Value = serde_json::from_str(&call.arguments).unwrap_or(json!({}));
			let preview = tool_preview(kind, &args, &call.arguments);
			let call_id = call.id.clone();
			let call_name = call.name.clone();

			let mut denied = false;
			if let Some(k) = kind {
				let key = format!("{}:{}", k.name(), preview);
				let auto = always_allow.iter().any(|a| a == &key || a == k.name());
				if !auto && needs_permission(k, &args, input.mode) {
					let decision = if let Some(ref hub) = input.permission {
						let tool_n = k.name().to_string();
						let prev = preview.clone();
						let cid = call_id.clone();
						let tx_ui = tx.clone();
						hub.request(&tool_n, &prev, PERM_TIMEOUT, || {
							let _ = tx_ui.send(format!(
								"{}{}\n{}\n",
								permission_hub::PERM_REQ_PREFIX,
								tool_n,
								prev.chars().take(120).collect::<String>()
							));
							// In-stream approval card + structured event for interactive UI.
							let _ = tx_ui.send(format!(
								"\n```approval id=\"{cid}\" tool=\"{tool_n}\"\n{tool_n}\n{}\n[y] Allow once   [a] Always   [n] Deny\n```\n",
								prev.chars().take(160).collect::<String>()
							));
							let _ = tx_ui.send(
								crate::stream_events::StreamEvent::Permission {
									tool: tool_n.clone(),
									preview: prev.clone(),
									call_id: cid,
								}
								.encode(),
							);
						})
						.await
					} else {
						PermissionDecision::AllowOnce
					};
					match decision {
						PermissionDecision::AllowAlways => {
							always_allow.push(key);
							let _ = tx.send(
								crate::stream_events::StreamEvent::PermissionResolved {
									call_id: call_id.clone(),
									decision: "always".into(),
								}
								.encode(),
							);
						}
						PermissionDecision::Deny => {
							denied = true;
							let _ = tx.send(
								crate::stream_events::StreamEvent::PermissionResolved {
									call_id: call_id.clone(),
									decision: "deny".into(),
								}
								.encode(),
							);
						}
						PermissionDecision::AllowOnce => {
							let _ = tx.send(
								crate::stream_events::StreamEvent::PermissionResolved {
									call_id: call_id.clone(),
									decision: "once".into(),
								}
								.encode(),
							);
						}
					}
				}
			}

			let _ = tx.send(crate::tools::format_tool_running_id(&call_name, &preview, Some(&call_id)));
			let tx_c = tx.clone();
			let call_c = call.clone();
			let cwd = input.cwd.clone();
			let mode = input.mode;
			let plan_shell = input.plan_allow_shell;
			let is_shell = matches!(kind, Some(ToolKind::Shell));

			let call_id2 = call_id.clone();
			let call_name2 = call_name.clone();
			let call_name_denied = call_name.clone();

			handles.push(tokio::spawn(async move {
				let started = std::time::Instant::now();
				let result = if denied {
					ToolResult {
						call_id,
						name: call_name_denied,
						ok: false,
						title: format!("{call_name} denied by user"),
						output: "User denied this tool call.".into(),
						preview: String::new(),
					}
				} else if is_shell {
					// Live line streaming into the Terminal card.
					let tx_live = tx_c.clone();
					let id_live = call_id2.clone();
					let call_live = call_c.clone();
					let cwd_live = cwd.clone();
					tokio::task::spawn_blocking(move || {
						let args: serde_json::Value =
							serde_json::from_str(&call_live.arguments).unwrap_or_else(|_| serde_json::json!({}));
						tools::exec_shell_live(&call_live.id, &args, &cwd_live, |line| {
							let _ = tx_live.send(
								crate::stream_events::StreamEvent::ToolDelta {
									id: id_live.clone(),
									chunk: line.to_string(),
								}
								.encode(),
							);
						})
					})
					.await
					.unwrap_or_else(|_| ToolResult {
						call_id: call_id2,
						name: call_name2,
						ok: false,
						title: "tool join error".into(),
						output: "join failed".into(),
						preview: String::new(),
					})
				} else {
					tokio::task::spawn_blocking(move || tools::execute(&call_c, &cwd, mode, plan_shell))
						.await
						.unwrap_or_else(|_| ToolResult {
							call_id: call_id2,
							name: call_name2,
							ok: false,
							title: "tool join error".into(),
							output: "join failed".into(),
							preview: String::new(),
						})
				};
				let _ = tx_c.send(crate::tools::format_tool_result_ex(&result, Some(started.elapsed())));
				(kind, result)
			}));
		}

		let results: Vec<(Option<ToolKind>, ToolResult)> =
			join_all(handles).await.into_iter().filter_map(|r| r.ok()).collect();
		for (kind, result) in &results {
			if let Some(k) = kind
				&& k.is_context_tool()
				&& let Some((_, n)) = ctx_counts.iter_mut().find(|(t, _)| *t == *k)
			{
				*n = n.saturating_add(1);
			}
			if !result.ok {
				let _ = tx.send(format!("{}{}\n", permission_hub::ERROR_CARD_PREFIX, result.title));
			}
			let matched_call = calls.iter().find(|c| c.id == result.call_id);
			if let Some(matched_call) = matched_call {
				push_tool_result(&mut messages, matched_call, result, step, use_native_tool_msgs);
			}
		}

		// Continue the loop — model sees tool results and can call more tools.
		tokio::time::sleep(Duration::from_millis(10)).await;
	}

	emit_context_summary(&tx, &ctx_counts);
	let _ = tx.send(format!(
		"\n*Stopped after {max_steps} tool steps (safety cap). Continue with another message if needed.*\n"
	));
	let _ = tx.send(format!(
		"{}Re-run or refine your prompt to continue.\n",
		permission_hub::RETRY_HINT_PREFIX
	));
	Ok(())
}

async fn run_question(
	call: &ToolCall,
	args: &Value,
	hub: &Option<QuestionHub>,
	tx: &Sender<String>,
) -> ToolResult {
	let prompt = args
		.get("prompt")
		.or_else(|| args.get("question"))
		.and_then(|v| v.as_str())
		.unwrap_or("Choose an option:")
		.to_string();
	let options: Vec<String> = args
		.get("options")
		.and_then(|v| v.as_array())
		.map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect())
		.unwrap_or_else(|| vec!["Yes".into(), "No".into()]);

	let _ = tx.send(format_tool_running("question", &prompt));

	if let Some(hub) = hub {
		hub.ask(&call.id, &prompt, options.clone());
		let _ = tx.send(format!(
			"{}{}\n{}\n",
			permission_hub::QUESTION_REQ_PREFIX,
			prompt,
			options.join(" | ")
		));
		// In-stream question card (selectable in the message list).
		let opts_block = options
			.iter()
			.enumerate()
			.map(|(i, o)| format!("  {}. {o}", i + 1))
			.collect::<Vec<_>>()
			.join("\n");
		let _ = tx.send(format!("\n```question id=\"{}\"\n{prompt}\n{opts_block}\n```\n", call.id));
		let _ = tx.send(
			crate::stream_events::StreamEvent::Question {
				id: call.id.clone(),
				prompt: prompt.clone(),
				options: options.clone(),
			}
			.encode(),
		);
		if let Some(answer) = hub.wait_reply(QUESTION_TIMEOUT).await {
			return ToolResult {
				call_id: call.id.clone(),
				name: "question".into(),
				ok: true,
				title: "Question · answered".to_string(),
				output: format!("User answered: {answer}"),
				preview: answer.chars().take(60).collect(),
			};
		}
		return ToolResult {
			call_id: call.id.clone(),
			name: "question".into(),
			ok: false,
			title: "Question · timeout".into(),
			output: "User did not answer in time.".into(),
			preview: prompt.chars().take(40).collect(),
		};
	}

	// No UI hub — return options for the model.
	ToolResult {
		call_id: call.id.clone(),
		name: "question".into(),
		ok: true,
		title: "Question · (no UI)".into(),
		output: format!("Could not show UI. Prompt: {prompt}\nOptions: {}", options.join(", ")),
		preview: prompt.chars().take(40).collect(),
	}
}

fn emit_context_summary(tx: &Sender<String>, counts: &[(ToolKind, u32)]) {
	let total: u32 = counts.iter().map(|(_, n)| *n).sum();
	if total >= 2 {
		let line = format_context_group_summary(counts);
		if !line.is_empty() {
			let _ = tx.send(line);
		}
	}
}

fn push_tool_result(
	messages: &mut Vec<zen::ApiMessage>,
	call: &ToolCall,
	result: &ToolResult,
	step: u32,
	native: bool,
) {
	if native {
		messages.push(zen::ApiMessage {
			role: "tool".into(),
			content: Some(tool_message_content(result)),
			tool_call_id: Some(if call.id.is_empty() { format!("call_{step}") } else { call.id.clone() }),
			tool_calls: None,
			name: Some(result.name.clone()),
		});
	} else {
		let text = format!("Result of command `{}`:\n{}", result.name, tool_message_content(result));
		messages.push(zen::ApiMessage {
			role: "user".into(),
			content: Some(text),
			tool_call_id: None,
			tool_calls: None,
			name: None,
		});
	}
}

fn assistant_api_message(turn: &ChatTurn) -> zen::ApiMessage {
	let tool_calls = if turn.tool_calls.is_empty() {
		None
	} else {
		Some(
			turn
				.tool_calls
				.iter()
				.map(|c| ToolCallDelta {
					id: c.id.clone(),
					name: c.name.clone(),
					arguments: c.arguments.clone(),
				})
				.collect(),
		)
	};
	zen::ApiMessage {
		role: "assistant".into(),
		content: Some(if turn.text.is_empty() { String::new() } else { turn.text.clone() }),
		tool_call_id: None,
		tool_calls,
		name: None,
	}
}

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

/// Whether this mode should use the multi-step tool loop (vs plain one-shot chat).
/// All profiles use the loop so shell/read/write recovery works everywhere.
pub fn mode_uses_tool_loop(mode: AgentMode) -> bool {
	matches!(
		mode,
		AgentMode::Ask
			| AgentMode::Write
			| AgentMode::Plan
			| AgentMode::Goal
			| AgentMode::Agent
			| AgentMode::Multi
			| AgentMode::Automation
	)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn all_modes_use_loop() {
		for m in AgentMode::ALL {
			assert!(mode_uses_tool_loop(m), "mode {:?} should use tool loop", m);
		}
	}
}

/// Integration tests: spin up a local wiremock server and feed predefined
/// SSE streams to the agent loop, verifying end-to-end tool-loop behaviour.
#[cfg(test)]
mod integration_tests {
	use super::*;

	use std::sync::mpsc;
	use std::time::Duration;

	use serde_json::json;
	use tokio::time::timeout;
	use wiremock::matchers::method;
	use wiremock::{Mock, MockServer, ResponseTemplate};

	const TEST_TIMEOUT: Duration = Duration::from_secs(15);

	// ── SSE body helpers ────────────────────────────────────────────────

	fn sse_body(chunks: Vec<serde_json::Value>) -> String {
		let mut body = String::new();
		for chunk in chunks {
			let line = serde_json::to_string(&chunk).expect("sse chunk json");
			body.push_str(&format!("data: {line}\n\n"));
		}
		body.push_str("data: [DONE]\n\n");
		body
	}

	fn text_response(text: &str) -> ResponseTemplate {
		ResponseTemplate::new(200).set_body_string(sse_body(vec![json!({
			"choices": [{
				"delta": { "content": text },
				"finish_reason": "stop"
			}],
			"usage": {
				"prompt_tokens": 5,
				"completion_tokens": 10,
				"total_tokens": 15
			}
		})]))
	}

	fn tool_call_response(name: &str, arguments: &str) -> ResponseTemplate {
		let arg_obj: serde_json::Value = serde_json::from_str(arguments).unwrap_or(json!({}));
		let arg_str = serde_json::to_string(&arg_obj).expect("args json");

		ResponseTemplate::new(200).set_body_string(sse_body(vec![
			// First delta starts the tool call stream
			json!({
				"choices": [{
					"delta": {
						"tool_calls": [{
							"index": 0,
							"id": "call_test_1",
							"type": "function",
							"function": { "name": name, "arguments": "" }
						}]
					},
					"finish_reason": null
				}]
			}),
			// Second delta delivers the remaining arguments + finish
			json!({
				"choices": [{
					"delta": {
						"tool_calls": [{
							"index": 0,
							"function": { "arguments": arg_str }
						}]
					},
					"finish_reason": "tool_calls"
				}],
				"usage": {
					"prompt_tokens": 5,
					"completion_tokens": 10,
					"total_tokens": 15
				}
			}),
		]))
	}

	fn error_response(status: u16, body: &str) -> ResponseTemplate {
		ResponseTemplate::new(status).set_body_string(body.to_string())
	}

	// ── Input builder ───────────────────────────────────────────────────

	fn base_input(api_url: String) -> LoopInput {
		LoopInput {
			model: "test-model".into(),
			system: String::new(),
			history: vec![("user".into(), "Hello, please help.".into())],
			mode: AgentMode::Ask,
			cwd: std::env::current_dir().unwrap_or_default(),
			plan_allow_shell: false,
			api_url: Some(api_url),
			enable_native_tools: true,
			max_steps: 10,
			permission: None,
			questions: None,
			ledger: None,
			sidebar: None,
		}
	}

	// ── Collector ───────────────────────────────────────────────────────

	/// Block until channel disconnects (agent loop finished), collect all text.
	/// MUST be called via `spawn_blocking` to avoid starving the tokio runtime.
	fn collect_all(rx: mpsc::Receiver<String>) -> String {
		let mut out = String::new();
		while let Ok(msg) = rx.recv() {
			out.push_str(&msg);
		}
		out
	}

	/// Run the agent loop with the given input and collect all output.
	/// The blocking receiver runs on a separate blocking thread so the
	/// tokio task driving the agent loop can make progress.
	async fn run_and_collect(input: LoopInput) -> String {
		let (tx, rx) = mpsc::channel();
		let handle = tokio::spawn(async move { run(input, tx).await });
		let output =
			tokio::task::spawn_blocking(move || collect_all(rx)).await.expect("collector panicked");
		handle.await.unwrap().unwrap();
		output
	}

	// ── Tests ───────────────────────────────────────────────────────────

	#[tokio::test]
	async fn text_only_response() {
		let srv = MockServer::start().await;
		Mock::given(method("POST"))
			.respond_with(text_response("Hello, I am an AI assistant."))
			.mount(&srv)
			.await;

		let input = base_input(srv.uri());
		let output = timeout(TEST_TIMEOUT, run_and_collect(input)).await.expect("test timed out");

		assert!(output.contains("AI assistant"), "expected assistant text in output: {output}");
	}

	/// Single tool-call step with max_steps=1 so the loop does one
	/// iteration, executes the tool, then exits at the loop boundary.
	#[tokio::test]
	async fn tool_call_execution() {
		let srv = MockServer::start().await;
		Mock::given(method("POST"))
			.respond_with(tool_call_response("glob", r#"{"pattern":"src/*.rs"}"#))
			.mount(&srv)
			.await;

		let mut input = base_input(srv.uri());
		input.max_steps = 1;
		let output = timeout(TEST_TIMEOUT, run_and_collect(input)).await.expect("test timed out");

		assert!(output.contains("Glob"), "expected Glob tool marker in output: {output}");
		assert!(output.contains(".rs"), "expected glob results (.rs files) in output: {output}");
	}

	/// Same tool call repeated → doom-loop guard fires after 4 repetitions.
	#[tokio::test]
	async fn doom_loop_guard() {
		let srv = MockServer::start().await;
		Mock::given(method("POST"))
			.respond_with(tool_call_response("glob", r#"{"pattern":"*.md"}"#))
			.mount(&srv)
			.await;

		let mut input = base_input(srv.uri());
		input.max_steps = 10;
		let output = timeout(TEST_TIMEOUT, run_and_collect(input)).await.expect("test timed out");

		assert!(
			output.contains("doom-loop") || output.contains("repeated"),
			"expected doom-loop guard message in output: {output}"
		);
	}

	/// Tool calls every step → max_steps safety cap fires.
	#[tokio::test]
	async fn max_steps_cap() {
		let srv = MockServer::start().await;
		Mock::given(method("POST"))
			.respond_with(tool_call_response("glob", r#"{"pattern":"*.md"}"#))
			.mount(&srv)
			.await;

		let mut input = base_input(srv.uri());
		input.max_steps = 2;
		let output = timeout(TEST_TIMEOUT, run_and_collect(input)).await.expect("test timed out");

		assert!(output.contains("Stopped after 2"), "expected max-steps message in output: {output}");
	}

	/// HTTP 500 propagates as an error from `run()`.
	#[tokio::test]
	async fn http_error_propagates() {
		let srv = MockServer::start().await;
		Mock::given(method("POST"))
			.respond_with(error_response(500, r#"{"error":"server error"}"#))
			.mount(&srv)
			.await;

		let input = base_input(srv.uri());
		let result = timeout(TEST_TIMEOUT, async {
			let (tx, rx) = mpsc::channel();
			let handle = tokio::spawn(async move { run(input, tx).await });
			// Drain rx on blocking thread so tx doesn't stall
			let _ = tokio::task::spawn_blocking(move || collect_all(rx)).await;
			handle.await.unwrap()
		})
		.await
		.expect("test timed out");

		assert!(result.is_err(), "expected error from agent loop on HTTP 500");
	}
}
