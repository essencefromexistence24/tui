//! Concrete API handler implementations integrating with ChatState, ProviderStore,
//! SessionStore, PluginRegistry, and the tool system.

#![allow(dead_code)]

use std::sync::Arc;

use anyhow::Result;
use serde_json::{Value, json};

use crate::{
	api::{ApiHandler, SseEvent},
	modes::AgentMode,
	plugin_system::global_registry,
	plugin_system_tool,
	providers::{
		ConnectedProvider, ProviderKind, ProviderStore, load_or_refresh_catalog, load_provider_store,
		save_provider_store,
	},
	session_store, tools,
};

// ── Context ─────────────────────────────────────────────────────────────

/// Shared application context for the API.
pub struct AppApiContext {
	pub provider_store: parking_lot::Mutex<ProviderStore>,
	pub model_catalog: parking_lot::Mutex<crate::providers::ModelsDevCatalog>,
}

impl AppApiContext {
	pub fn new() -> Self {
		Self {
			provider_store: parking_lot::Mutex::new(load_provider_store()),
			model_catalog: parking_lot::Mutex::new(load_or_refresh_catalog()),
		}
	}

	/// Force-refresh the model catalog from the network/cache.
	pub fn refresh_catalog(&self) {
		*self.model_catalog.lock() = load_or_refresh_catalog();
	}
}

// ── Handler ─────────────────────────────────────────────────────────────

pub struct AppApiHandler {
	ctx: Arc<AppApiContext>,
}

impl AppApiHandler {
	pub fn new(ctx: Arc<AppApiContext>) -> Self {
		Self { ctx }
	}
}

#[async_trait::async_trait]
impl ApiHandler for AppApiHandler {
	// ── Chat ─────────────────────────────────────────────────────────
	async fn handle_chat(&self, body: Value, events: tokio::sync::mpsc::Sender<SseEvent>) {
		let model = body.get("model").and_then(|v| v.as_str()).unwrap_or("big-pickle");
		let _message = body.get("message").and_then(|v| v.as_str()).unwrap_or("");
		let system = body.get("system").and_then(|v| v.as_str());
		let mode_str = body.get("mode").and_then(|v| v.as_str()).unwrap_or("ask");

		let mode = match mode_str {
			"write" => AgentMode::Write,
			"plan" => AgentMode::Plan,
			"goal" => AgentMode::Goal,
			"agent" => AgentMode::Agent,
			_ => AgentMode::Ask,
		};

		let history: Vec<(String, String)> = body
			.get("history")
			.and_then(|v| v.as_array())
			.map(|arr| {
				arr
					.iter()
					.filter_map(|entry| {
						let role = entry.get("role").and_then(|r| r.as_str())?;
						let content = entry.get("content").and_then(|c| c.as_str())?;
						Some((role.to_string(), content.to_string()))
					})
					.collect()
			})
			.unwrap_or_default();

		let zen_messages = history
			.into_iter()
			.map(|(role, content)| crate::zen::ApiMessage {
				role,
				content: Some(content),
				tool_call_id: None,
				tool_calls: None,
				name: None,
			})
			.collect::<Vec<_>>();

		let builtin_schemas = crate::tools::openai_tool_schemas(mode);
		let tool_schemas = plugin_system_tool::merge_tool_schemas(builtin_schemas);

		let start_msg = json!({"type":"start","model":model}).to_string();
		if events.send(SseEvent::Data(start_msg)).await.is_err() {
			return;
		}

		let (tx, rx) = std::sync::mpsc::channel::<String>();
		let events_clone = events.clone();
		let model = model.to_string();

		// Prepend system message if provided
		let mut chat_messages = zen_messages;
		if let Some(sys) = system {
			chat_messages.insert(
				0,
				crate::zen::ApiMessage {
					role: "system".into(),
					content: Some(sys.to_string()),
					tool_call_id: None,
					tool_calls: None,
					name: None,
				},
			);
		}

		// Spawn streaming in background, relay SSE events in foreground
		tokio::spawn(async move {
			let result =
				crate::zen::stream_chat_messages(&model, &chat_messages, Some(&tool_schemas), None, tx)
					.await;

			match result {
				Ok(turn) => {
					let _ = events_clone.try_send(SseEvent::Data(
						json!({
							"type": "turn",
							"content": turn.text,
							"tool_calls": turn.tool_calls.iter().map(|tc| json!({
								"id": tc.id,
								"name": tc.name,
								"arguments": tc.arguments,
							})).collect::<Vec<_>>(),
							"usage": {
								"prompt_tokens": turn.token_usage.prompt_tokens,
								"completion_tokens": turn.token_usage.completion_tokens,
								"total_tokens": turn.token_usage.total_tokens,
							}
						})
						.to_string(),
					));
					let _ = events_clone.try_send(SseEvent::Done);
				}
				Err(e) => {
					let _ = events_clone.try_send(SseEvent::Error(e.to_string()));
				}
			}
		});

		// Relay streamed token chunks
		while let Ok(chunk) = rx.recv() {
			if events
				.send(SseEvent::Data(json!({"type":"delta","content":chunk}).to_string()))
				.await
				.is_err()
			{
				break;
			}
		}
	}

	// ── Sessions ────────────────────────────────────────────────────
	async fn list_sessions(&self) -> Result<Value> {
		let sessions = session_store::load_all_sessions();
		let items: Vec<Value> = sessions
			.into_iter()
			.map(|s| {
				json!({
					"id": s.id,
					"name": s.name,
					"model": s.model,
					"provider": s.provider,
					"agent_mode": s.agent_mode.label(),
					"created_at": s.created_at.to_rfc3339(),
					"updated_at": s.updated_at.to_rfc3339(),
					"message_count": s.messages.len(),
				})
			})
			.collect();
		Ok(json!({ "sessions": items }))
	}

	async fn get_session(&self, id: &str) -> Result<Option<Value>> {
		match session_store::load_session_by_id(id) {
			Ok(session) => {
				let messages: Vec<Value> = session
					.messages
					.iter()
					.map(|m| {
						let role = match m.role {
							crate::components::MessageRole::User => "user",
							crate::components::MessageRole::Assistant => "assistant",
						};
						json!({
							"role": role,
							"content": m.content,
							"timestamp": m.timestamp.to_rfc3339(),
						})
					})
					.collect();
				Ok(Some(json!({
					"id": session.id,
					"name": session.name,
					"messages": messages,
					"model": session.model,
					"provider": session.provider,
					"agent_mode": session.agent_mode.label(),
					"created_at": session.created_at.to_rfc3339(),
					"updated_at": session.updated_at.to_rfc3339(),
				})))
			}
			Err(e) => {
				if e.to_string().contains("not found") {
					Ok(None)
				} else {
					Err(e)
				}
			}
		}
	}

	async fn delete_session(&self, id: &str) -> Result<bool> {
		session_store::delete_session(id)?;
		Ok(true)
	}

	// ── Providers ───────────────────────────────────────────────────
	async fn list_providers(&self) -> Result<Value> {
		let store = self.ctx.provider_store.lock();
		let catalog = self.ctx.model_catalog.lock();
		let rows = crate::provider_registry::list_providers(&catalog, &store);
		let items: Vec<Value> = rows
			.into_iter()
			.map(|r| {
				json!({
					"id": r.id,
					"name": r.name,
					"source": r.source,
					"connected": r.connected,
					"model_count": r.model_count,
					"health": format!("{:?}", r.health),
				})
			})
			.collect();
		Ok(json!({ "providers": items }))
	}

	async fn connect_provider(&self, body: Value) -> Result<Value> {
		let id = body.get("id").and_then(|v| v.as_str()).unwrap_or("");
		let name = body.get("name").and_then(|v| v.as_str()).unwrap_or(id);
		let base_url = body.get("base_url").and_then(|v| v.as_str());
		let api_key_env = body.get("api_key_env").and_then(|v| v.as_str());
		let model = body.get("default_model").and_then(|v| v.as_str());

		if id.is_empty() {
			return Err(anyhow::anyhow!("provider `id` is required"));
		}

		let mut store = self.ctx.provider_store.lock();
		let provider = ConnectedProvider {
			id: id.to_string(),
			name: name.to_string(),
			kind: ProviderKind::OpenAiCompatible,
			base_url: base_url.map(|s| s.to_string()),
			api_key_env: api_key_env.map(|s| s.to_string()),
			default_model: model.map(|s| s.to_string()),
			enabled: true,
		};

		if let Some(existing) = store.providers.iter_mut().find(|p| p.id == id) {
			*existing = provider;
		} else {
			store.providers.push(provider);
		}

		save_provider_store(&store)
			.map_err(|e| anyhow::anyhow!("failed to save provider store: {e}"))?;
		Ok(json!({ "status": "connected", "id": id }))
	}

	// ── Tools ───────────────────────────────────────────────────────
	async fn list_tools(&self) -> Result<Value> {
		let mode = AgentMode::Agent;
		let builtin: Vec<Value> = tools::tools_for_mode(mode)
			.into_iter()
			.map(|t| {
				json!({
					"name": t.kind.name(),
					"description": t.description,
					"type": "builtin",
				})
			})
			.collect();

		let plugin_tools: Vec<Value> = global_registry()
			.plugin_tool_defs()
			.into_iter()
			.map(|(plugin, def)| {
				json!({
					"name": def.name,
					"description": def.description,
					"type": "plugin",
					"plugin": plugin,
				})
			})
			.collect();

		let mut all = builtin;
		all.extend(plugin_tools);
		Ok(json!({ "tools": all }))
	}

	async fn execute_tool(&self, body: Value) -> Result<Value> {
		let name = body.get("name").and_then(|v| v.as_str()).unwrap_or("");
		let args = body.get("arguments").cloned().unwrap_or_default();

		if name.is_empty() {
			return Err(anyhow::anyhow!("tool `name` is required"));
		}

		if global_registry().is_plugin_tool(name) {
			return global_registry()
				.execute_tool(name, &args)
				.map(|result| json!({ "ok": true, "result": result }))
				.or_else(|e| Ok(json!({ "ok": false, "error": e.to_string() })));
		}

		let call =
			tools::ToolCall { id: "api_call".into(), name: name.into(), arguments: args.to_string() };

		let result = tools::execute(&call, std::path::Path::new("."), AgentMode::Agent, true);

		Ok(json!({
			"ok": result.ok,
			"title": result.title,
			"output": result.output,
			"preview": result.preview,
		}))
	}

	// ── Status ──────────────────────────────────────────────────────
	async fn status(&self) -> Result<Value> {
		let store = self.ctx.provider_store.lock();
		let catalog = self.ctx.model_catalog.lock();
		let plugins = global_registry().list_plugins();

		Ok(json!({
			"version": env!("CARGO_PKG_VERSION"),
			"plugins": plugins.len(),
			"providers": store.providers.len(),
			"models": catalog.model_count(),
			"sessions": session_store::load_all_sessions().len(),
		}))
	}

	// ── Models ──────────────────────────────────────────────────────
	async fn list_models(&self) -> Result<Value> {
		let catalog = self.ctx.model_catalog.lock();
		let mut models = Vec::new();

		for (display, id) in crate::zen::MODELS {
			models.push(json!({
				"id": id,
				"name": display,
				"provider": "OpenCode Zen",
				"free": true,
			}));
		}

		for provider in &catalog.providers {
			for model in &provider.models {
				models.push(json!({
					"id": model.id,
					"name": model.name,
					"provider": provider.name,
					"reasoning": model.reasoning,
					"tool_call": model.tool_call,
					"context": model.context,
				}));
			}
		}

		Ok(json!({ "models": models }))
	}

	// ── WebSocket ──────────────────────────────────────────────────
	async fn on_ws_message(&self, msg: Value) -> Result<Option<Value>> {
		let msg_type = msg.get("type").and_then(|v| v.as_str()).unwrap_or("");

		match msg_type {
			"ping" => Ok(Some(json!({ "type": "pong" }))),
			"execute_tool" => {
				let name = msg.get("name").and_then(|v| v.as_str()).unwrap_or("");
				let args = msg.get("arguments").cloned().unwrap_or_default();
				if global_registry().is_plugin_tool(name) {
					match global_registry().execute_tool(name, &args) {
						Ok(result) => Ok(Some(json!({
							"type": "tool_result",
							"name": name,
							"ok": true,
							"result": result,
						}))),
						Err(e) => Ok(Some(json!({
							"type": "tool_result",
							"name": name,
							"ok": false,
							"error": e.to_string(),
						}))),
					}
				} else {
					let call = tools::ToolCall {
						id: "ws_call".into(),
						name: name.into(),
						arguments: args.to_string(),
					};
					let result = tools::execute(&call, std::path::Path::new("."), AgentMode::Agent, true);
					Ok(Some(json!({
						"type": "tool_result",
						"name": result.name,
						"ok": result.ok,
						"title": result.title,
						"output": result.output,
					})))
				}
			}
			"list_plugins" => {
				let plugins = global_registry().list_plugins();
				Ok(Some(json!({ "type": "plugins", "plugins": plugins })))
			}
			"list_tools" => Ok(Some(json!({ "type": "tools", "tools": self.list_tools().await? }))),
			_ => Ok(Some(json!({
				"type": "error",
				"message": format!("unknown message type: {msg_type}"),
			}))),
		}
	}
}
