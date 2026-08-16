//! OpenCode Zen / OpenAI-compatible chat streaming (with optional tools).

use std::collections::HashMap;
use std::sync::mpsc;
use std::time::Duration;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::tools::ToolCall;

/// OpenCode Zen free models currently usable with the public bearer header.
pub const MODELS: &[(&str, &str)] = &[
	("Big Pickle", "big-pickle"),
	("DeepSeek V4 Flash", "deepseek-v4-flash-free"),
	("MiMo-V2.5", "mimo-v2.5-free"),
	("Hy3 Free", "hy3-free"),
	("Nemotron 3 Ultra", "nemotron-3-ultra-free"),
];

/// Default remote model: OpenCode Zen Big Pickle.
pub const DEFAULT_MODEL: &str = "big-pickle";
pub const DEFAULT_MODEL_DISPLAY: &str = "Big Pickle";
pub const DEFAULT_PROVIDER: &str = "OpenCode Zen";
pub const ZEN_URL: &str = "https://opencode.ai/zen/v1/chat/completions";

/// Bottom-bar provider label. All listed free remote models are served via OpenCode Zen.
#[allow(dead_code)]
pub fn provider_for_model(model_id: &str) -> &'static str {
	if model_id.contains("qwen")
		|| model_id.contains("vibethinker")
		|| model_id.contains("ministral")
		|| model_id.contains("xlam")
		|| model_id.starts_with("dx-flow")
	{
		return "dx-flow";
	}
	DEFAULT_PROVIDER
}

/// Rough free-tier cost estimate (USD). Zen free models are $0.
#[allow(dead_code)]
pub fn estimate_cost_usd(_model_id: &str, _input_tokens: usize, _output_tokens: usize) -> f64 {
	0.0
}

// ── Public API message types (tool-loop) ────────────────────────────────

/// Flexible chat message for multi-step tool loops.
#[derive(Debug, Clone)]
pub struct ApiMessage {
	pub role: String,
	pub content: Option<String>,
	pub tool_call_id: Option<String>,
	pub tool_calls: Option<Vec<ToolCallDelta>>,
	pub name: Option<String>,
}

impl ApiMessage {
	pub fn system(content: impl Into<String>) -> Self {
		Self {
			role: "system".into(),
			content: Some(content.into()),
			tool_call_id: None,
			tool_calls: None,
			name: None,
		}
	}

	#[allow(dead_code)]
	pub fn user(content: impl Into<String>) -> Self {
		Self {
			role: "user".into(),
			content: Some(content.into()),
			tool_call_id: None,
			tool_calls: None,
			name: None,
		}
	}

	#[allow(dead_code)]
	pub fn assistant(content: impl Into<String>) -> Self {
		Self {
			role: "assistant".into(),
			content: Some(content.into()),
			tool_call_id: None,
			tool_calls: None,
			name: None,
		}
	}
}

/// In-memory tool call (serialized to OpenAI shape via `api_message_to_value`).
#[derive(Debug, Clone)]
pub struct ToolCallDelta {
	pub id: String,
	pub name: String,
	pub arguments: String,
}

#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
	pub prompt_tokens: usize,
	pub completion_tokens: usize,
	pub total_tokens: usize,
}

#[derive(Debug, Clone)]
pub struct ChatTurn {
	pub text: String,
	pub tool_calls: Vec<ToolCall>,
	#[allow(dead_code)]
	pub finish_reason: Option<String>,
	pub token_usage: TokenUsage,
	/// True when the provider rejected tools and this turn was retried tool-free.
	#[allow(dead_code)]
	pub tools_disabled: bool,
}

fn strip_model_id(model: &str) -> &str {
	model.strip_prefix("zen/").or_else(|| model.strip_prefix("omniroute/")).unwrap_or(model)
}

// ── Wire format ─────────────────────────────────────────────────────────

#[derive(Serialize)]
struct WireRequest {
	model: String,
	messages: Vec<Value>,
	max_tokens: u32,
	stream: bool,
	#[serde(skip_serializing_if = "Option::is_none")]
	tools: Option<Vec<Value>>,
	#[serde(skip_serializing_if = "Option::is_none")]
	tool_choice: Option<String>,
}

fn api_message_to_value(m: &ApiMessage) -> Value {
	let mut map = serde_json::Map::new();
	map.insert("role".into(), Value::String(m.role.clone()));
	if let Some(ref c) = m.content {
		// OpenAI allows null content with tool_calls; empty string is safer for most proxies.
		map.insert("content".into(), Value::String(c.clone()));
	} else if m.tool_calls.is_none() {
		map.insert("content".into(), Value::String(String::new()));
	}
	if let Some(ref id) = m.tool_call_id {
		map.insert("tool_call_id".into(), Value::String(id.clone()));
	}
	if let Some(ref name) = m.name {
		map.insert("name".into(), Value::String(name.clone()));
	}
	if let Some(ref tcs) = m.tool_calls {
		let arr: Vec<Value> =
			tcs.iter().map(|tc| json_tool_call(&tc.id, &tc.name, &tc.arguments)).collect();
		map.insert("tool_calls".into(), Value::Array(arr));
	}
	Value::Object(map)
}

fn json_tool_call(id: &str, name: &str, arguments: &str) -> Value {
	serde_json::json!({
		"id": if id.is_empty() { "call_0" } else { id },
		"type": "function",
		"function": {
			"name": name,
			"arguments": arguments
		}
	})
}

#[derive(Deserialize, Default)]
struct StreamUsage {
	#[serde(default)]
	prompt_tokens: usize,
	#[serde(default)]
	completion_tokens: usize,
	#[serde(default)]
	total_tokens: usize,
}

#[derive(Deserialize)]
struct StreamChunk {
	#[serde(default)]
	choices: Vec<StreamChoice>,
	#[serde(default)]
	usage: Option<StreamUsage>,
}

#[derive(Deserialize)]
struct StreamChoice {
	#[serde(default)]
	delta: StreamDelta,
	#[serde(default)]
	finish_reason: Option<String>,
}

#[derive(Deserialize, Default)]
struct StreamDelta {
	#[serde(default)]
	content: Option<String>,
	#[serde(default)]
	reasoning_content: Option<String>,
	#[serde(default)]
	reasoning: Option<String>,
	#[serde(default)]
	tool_calls: Option<Vec<StreamToolCall>>,
}

#[derive(Deserialize, Default)]
struct StreamToolCall {
	#[serde(default)]
	index: usize,
	#[serde(default)]
	id: Option<String>,
	#[serde(default)]
	#[allow(dead_code)]
	r#type: Option<String>,
	#[serde(default)]
	function: Option<StreamFunction>,
}

#[derive(Deserialize, Default)]
struct StreamFunction {
	#[serde(default)]
	name: Option<String>,
	#[serde(default)]
	arguments: Option<String>,
}

// ── Public stream APIs ──────────────────────────────────────────────────

#[allow(dead_code)]
pub async fn stream_chat(
	model: &str,
	history: Vec<(String, String)>,
	tx: mpsc::Sender<String>,
) -> Result<()> {
	stream_chat_url_with_system(model, history, tx, ZEN_URL, None).await
}

/// Stream chat with optional DX system stack (OpenAI-compatible).
pub async fn stream_chat_with_system(
	model: &str,
	history: Vec<(String, String)>,
	tx: mpsc::Sender<String>,
	system: Option<String>,
) -> Result<()> {
	stream_chat_url_with_system(model, history, tx, ZEN_URL, system).await
}

/// Stream chat to any OpenAI-compatible endpoint (Zen, OmniRoute, etc.).
#[allow(dead_code)]
pub async fn stream_chat_url(
	model: &str,
	history: Vec<(String, String)>,
	tx: mpsc::Sender<String>,
	url: &str,
) -> Result<()> {
	stream_chat_url_with_system(model, history, tx, url, None).await
}

/// Like [`stream_chat_url`] with a dedicated system message (token-efficient DX stack).
pub async fn stream_chat_url_with_system(
	model: &str,
	history: Vec<(String, String)>,
	tx: mpsc::Sender<String>,
	url: &str,
	system: Option<String>,
) -> Result<()> {
	let mut messages = Vec::new();
	if let Some(sys) = system.filter(|s| !s.trim().is_empty()) {
		messages.push(ApiMessage::system(sys));
	}
	for (role, content) in history {
		if role == "system" {
			continue;
		}
		messages.push(ApiMessage {
			role,
			content: Some(content),
			tool_call_id: None,
			tool_calls: None,
			name: None,
		});
	}
	let turn = stream_chat_messages(model, &messages, None, Some(url), tx).await?;
	let _ = turn;
	Ok(())
}

/// Full tool-aware stream. Streams text deltas to `tx` and returns accumulated turn.
pub async fn stream_chat_messages(
	model: &str,
	messages: &[ApiMessage],
	tools: Option<&[Value]>,
	url: Option<&str>,
	tx: mpsc::Sender<String>,
) -> Result<ChatTurn> {
	let url = url.unwrap_or(ZEN_URL);
	let model_id = strip_model_id(model);
	let wire_messages: Vec<Value> = messages.iter().map(api_message_to_value).collect();

	let has_tools = tools.map(|t| !t.is_empty()).unwrap_or(false);
	let body = WireRequest {
		model: model_id.to_string(),
		messages: wire_messages,
		max_tokens: 8192,
		stream: true,
		tools: if has_tools { tools.map(|t| t.to_vec()) } else { None },
		tool_choice: if has_tools { Some("auto".into()) } else { None },
	};

	let client = reqwest::Client::builder()
		.timeout(Duration::from_secs(180))
		.build()
		.context("Failed to build HTTP client")?;

	let mut response = client
		.post(url)
		.json(&body)
		.send()
		.await
		.with_context(|| format!("Failed to send request to {url}"))?;

	if !response.status().is_success() {
		let status = response.status();
		let text = response.text().await.unwrap_or_default();
		// If tools rejected, retry once without tools so chat still works.
		if has_tools
			&& (status.as_u16() == 400
				|| status.as_u16() == 422
				|| text.to_ascii_lowercase().contains("tool")
				|| text.to_ascii_lowercase().contains("function"))
		{
			tracing::warn!("tools rejected by provider ({status}); retrying without tools");
			let mut turn = Box::pin(stream_chat_messages(model, messages, None, Some(url), tx)).await?;
			turn.tools_disabled = true;
			return Ok(turn);
		}
		anyhow::bail!("{url} returned {status}: {text}");
	}

	let mut buffer = String::new();
	let mut thinking_open = false;
	let mut text = String::new();
	let mut finish_reason: Option<String> = None;
	let mut token_usage = TokenUsage::default();
	// index -> (id, name, arguments)
	let mut tool_acc: HashMap<usize, (String, String, String)> = HashMap::new();

	while let Some(chunk) = response.chunk().await? {
		let chunk_str = String::from_utf8_lossy(&chunk);
		buffer.push_str(&chunk_str);

		while let Some(line_end) = buffer.find('\n') {
			let line = buffer[..line_end].trim().to_string();
			buffer.drain(..=line_end);

			if line.is_empty() {
				continue;
			}

			let Some(data) = line.strip_prefix("data: ") else {
				continue;
			};

			if data == "[DONE]" {
				if thinking_open {
					let _ = tx.send("\n</think>\n".into());
					thinking_open = false;
				}
				break;
			}

			let Ok(chunk) = serde_json::from_str::<StreamChunk>(data) else {
				continue;
			};
			let Some(choice) = chunk.choices.first() else {
				continue;
			};

			if let Some(ref fr) = choice.finish_reason {
				finish_reason = Some(fr.clone());
			}

			// Capture token usage from the final chunk
			if let Some(ref usage) = chunk.usage {
				token_usage = TokenUsage {
					prompt_tokens: usage.prompt_tokens,
					completion_tokens: usage.completion_tokens,
					total_tokens: usage.total_tokens,
				};
			}

			let delta = &choice.delta;

			// Reasoning
			let reason = delta.reasoning_content.as_deref().or(delta.reasoning.as_deref()).unwrap_or("");
			if !reason.is_empty() {
				if !thinking_open {
					let _ = tx.send("<think>\n".into());
					thinking_open = true;
				}
				let _ = tx.send(reason.to_string());
			}

			// Text content
			if let Some(ref content) = delta.content
				&& !content.is_empty()
			{
				if thinking_open {
					let _ = tx.send("\n</think>\n".into());
					thinking_open = false;
				}
				text.push_str(content);
				let _ = tx.send(content.clone());
			}

			// Tool call deltas
			if let Some(ref tcs) = delta.tool_calls {
				for tc in tcs {
					let entry = tool_acc
						.entry(tc.index)
						.or_insert_with(|| (String::new(), String::new(), String::new()));
					if let Some(ref id) = tc.id
						&& !id.is_empty()
					{
						entry.0 = id.clone();
					}
					if let Some(ref f) = tc.function {
						if let Some(ref n) = f.name
							&& !n.is_empty()
						{
							entry.1.push_str(n);
						}
						if let Some(ref a) = f.arguments {
							entry.2.push_str(a);
						}
					}
				}
			}
		}
	}

	if thinking_open {
		let _ = tx.send("\n</think>\n".into());
	}

	let mut tool_calls: Vec<ToolCall> = tool_acc
		.into_iter()
		.map(|(idx, (id, name, arguments))| ToolCall {
			id: if id.is_empty() { format!("call_{idx}") } else { id },
			name,
			arguments,
		})
		.filter(|c| !c.name.is_empty())
		.collect();
	// Stable order by id
	tool_calls.sort_by(|a, b| a.id.cmp(&b.id));

	Ok(ChatTurn { text, tool_calls, finish_reason, token_usage, tools_disabled: false })
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn api_message_serializes_tool() {
		let m = ApiMessage {
			role: "assistant".into(),
			content: Some(String::new()),
			tool_call_id: None,
			tool_calls: Some(vec![ToolCallDelta {
				id: "c1".into(),
				name: "shell".into(),
				arguments: r#"{"command":"git status"}"#.into(),
			}]),
			name: None,
		};
		let v = api_message_to_value(&m);
		assert_eq!(v["role"], "assistant");
		assert!(v["tool_calls"].as_array().unwrap().len() == 1);
		assert_eq!(v["tool_calls"][0]["function"]["name"], "shell");
	}

	#[test]
	fn strip_prefixes() {
		assert_eq!(strip_model_id("zen/big-pickle"), "big-pickle");
		assert_eq!(strip_model_id("omniroute/x"), "x");
	}
}
