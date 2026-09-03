//! Prompt queue system for sequential (other profiles) and concurrent (Multi profile) dispatch.

#![allow(dead_code)]
//!
//! - **Other profiles** (Ask/Write/Plan/Goal/Agent): prompts are queued if one is already running.
//!   The queue sends one at a time, drain-style.
//! - **Multi profile**: multiple prompts are sent to DIFFERENT models simultaneously.
//!   Each prompt gets its own `tokio::spawn` with an independent LLM call.
//!
//! The queue is integrated into ChatState's `add_user_message` and `drain_agent_response_chunks`.

use std::sync::mpsc::Sender;

use crate::zen;

/// A queued prompt with its target model.
#[derive(Debug, Clone)]
pub struct QueuedPrompt {
	pub content: String,
	pub model: String,
	pub id: String,
}

/// Handle a queued prompt: if Multi mode, launch ALL as concurrent tokio tasks.
/// If other mode, send one at a time (queue is drained by caller).
pub fn handle_multi_concurrent(
	queued: Vec<QueuedPrompt>,
	messages: Vec<Vec<(String, String)>>,
	system: String,
	api_url: Option<String>,
	tx: Sender<String>,
) {
	for (i, prompt) in queued.into_iter().enumerate() {
		let tx_c = tx.clone();
		let system_c = system.clone();
		let _api_url_c = api_url.clone();
		let history = messages.get(i).cloned().unwrap_or_default();
		let model = prompt.model.clone();

		tokio::spawn(async move {
			let _ = tx_c.send(format!("\n<subagent name=\"{model}\">\n"));
			let header =
				format!("**Multi query {}** — model: {model}\n{}\n\n", prompt.id, prompt.content);
			let _ = tx_c.send(header);

			let result =
				zen::stream_chat_with_system(&model, history, tx_c.clone(), Some(system_c)).await;

			match result {
				Ok(()) => {
					let _ = tx_c.send("\n</subagent>\n".to_string());
				}
				Err(e) => {
					let _ = tx_c.send(format!("\n*{model} error: {e}*\n"));
					let _ = tx_c.send("\n</subagent>\n".to_string());
				}
			}
		});
	}
}
