//! dx-agent runtime bridge.
//!
//! Uses the in-process `dx_agents` Agent runtime for the real agent loop.

pub const END_OF_RESPONSE: &str = "\n__END__";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentMetadata {
	pub alias: String,
	pub model_provider: String,
	pub model: String,
}

#[cfg(feature = "dx-agent")]
mod inner {
	use std::sync::atomic::{AtomicBool, Ordering};
	use std::{env, sync::Arc};

	use anyhow::{Context, Result, bail};
	use dx_agents::agent::TurnEvent;
	use tokio::sync::Mutex;

	use super::AgentMetadata;

	const DX_AGENT_ENV: &str = "DX_TUI_AGENT";
	const LEGACY_AGENT_ENV: &str = "ZEROCLAW_AGENT";
	const AGENT_EVENT_BUFFER: usize = 64;

	#[derive(Clone)]
	pub struct AgentBackend {
		state: Arc<Mutex<AgentBackendState>>,
	}

	#[derive(Default)]
	struct AgentBackendState {
		agent: Option<dx_agents::agent::Agent>,
		metadata: Option<AgentMetadata>,
		bound_session_id: Option<String>,
	}

	impl AgentBackend {
		pub fn new() -> Self {
			Self { state: Arc::new(Mutex::new(AgentBackendState::default())) }
		}

		pub async fn initialize(&self) -> Result<AgentMetadata> {
			let mut state = self.state.lock().await;
			state.ensure_agent().await
		}

		pub fn is_ready(&self) -> bool {
			self.state.try_lock().map(|s| s.is_ready()).unwrap_or(false)
		}

		pub async fn generate_stream<F>(&self, prompt: &str, callback: F) -> Result<()>
		where
			F: Fn(String) + Send + 'static,
		{
			self.generate_stream_for_session(None, prompt, &[], callback).await
		}

		pub async fn generate_stream_for_session<F>(
			&self,
			session_id: Option<&str>,
			prompt: &str,
			history: &[(String, String)],
			callback: F,
		) -> Result<()>
		where
			F: Fn(String) + Send + 'static,
		{
			let prompt = prompt.trim();
			if prompt.is_empty() {
				bail!("empty prompt");
			}

			let (event_tx, event_rx) = tokio::sync::mpsc::channel(AGENT_EVENT_BUFFER);
			let final_event_tx = event_tx.clone();
			let emitted_answer = Arc::new(AtomicBool::new(false));
			let stream_task = stream_agent_events(event_rx, emitted_answer.clone(), callback);

			let turn_result = {
				let mut state = self.state.lock().await;
				state.ensure_agent().await?;
				let mut just_rebound = false;
				if let Some(sid) = session_id {
					let need_reset = state.bound_session_id.as_deref() != Some(sid);
					if need_reset {
						if let Some(agent) = state.agent.as_mut() {
							agent.clear_history();
						}
						state.bound_session_id = Some(sid.to_string());
						just_rebound = true;
					}
				}
				let effective_prompt = if just_rebound && history.len() > 1 {
					let mut composed = String::from("Prior conversation (restored session):\n");
					for (role, content) in history.iter().rev().take(10).collect::<Vec<_>>().into_iter().rev()
					{
						let preview: String = content.chars().take(1_200).collect();
						composed.push_str(&format!("[{role}] {preview}\n\n"));
					}
					composed.push_str("---\n");
					composed.push_str(prompt);
					composed
				} else {
					prompt.to_string()
				};
				let agent = state.agent.as_mut().context("DX agent session was not initialized")?;
				agent.turn_streamed(&effective_prompt, event_tx, None).await
			};

			match turn_result {
				Ok((response, _new_messages)) => {
					if !emitted_answer.load(Ordering::Relaxed) && !response.trim().is_empty() {
						let _ = final_event_tx.send(TurnEvent::Chunk { delta: response }).await;
					}
				}
				Err(error) => {
					drop(final_event_tx);
					stream_task.await.context("DX agent stream task failed")??;
					return Err(error);
				}
			}

			drop(final_event_tx);
			stream_task.await.context("DX agent stream task failed")??;
			Ok(())
		}

		pub async fn reset_session(&self, session_id: Option<&str>) {
			let mut state = self.state.lock().await;
			if let Some(agent) = state.agent.as_mut() {
				agent.clear_history();
			}
			state.bound_session_id = session_id.map(|s| s.to_string());
		}
	}

	impl Default for AgentBackend {
		fn default() -> Self {
			Self::new()
		}
	}

	impl AgentBackendState {
		fn is_ready(&self) -> bool {
			self.agent.is_some()
		}

		async fn ensure_agent(&mut self) -> Result<AgentMetadata> {
			if let Some(metadata) = &self.metadata {
				return Ok(metadata.clone());
			}

			let config =
				dx_agents::Config::load_or_init().await.context("failed to load DX agent config")?;
			let candidates = agent_candidates(&config);
			let preferred = env::var(DX_AGENT_ENV).ok().or_else(|| env::var(LEGACY_AGENT_ENV).ok());
			let alias = select_agent_alias(preferred.as_deref(), &candidates)?;
			let session_cwd = env::current_dir().ok();
			let agent = dx_agents::agent::Agent::from_config_with_tui_env(
				&config,
				&alias,
				session_cwd.as_deref(),
				false,
				false,
				Some(env::vars().collect()),
			)
			.await
			.with_context(|| format!("failed to start DX agent '{alias}'"))?;

			let (alias, model_provider, model) = agent.attribution_fields();
			let metadata = AgentMetadata { alias, model_provider, model };
			self.agent = Some(agent);
			self.metadata = Some(metadata.clone());
			Ok(metadata)
		}
	}

	#[derive(Debug, Clone, PartialEq, Eq)]
	pub(crate) struct AgentCandidate {
		alias: String,
		enabled: bool,
		dispatchable: bool,
	}

	fn agent_candidates(config: &dx_agents::Config) -> Vec<AgentCandidate> {
		config
			.agents
			.iter()
			.map(|(alias, agent)| AgentCandidate {
				alias: alias.clone(),
				enabled: agent.enabled,
				dispatchable: agent.is_dispatchable(),
			})
			.collect()
	}

	pub(crate) fn select_agent_alias(
		preferred: Option<&str>,
		candidates: &[AgentCandidate],
	) -> Result<String> {
		if candidates.is_empty() {
			bail!(
				"No DX agents configured. Run `dx agents quickstart --agent dx` to create a default agent."
			);
		}

		if let Some(name) = preferred {
			if let Some(c) = candidates.iter().find(|c| c.alias == name) {
				if !c.enabled {
					bail!("DX agent '{name}' is disabled");
				}
				if !c.dispatchable {
					bail!("DX agent '{name}' is not dispatchable. Run `dx agents quickstart --agent dx`.");
				}
				return Ok(c.alias.clone());
			}
			bail!("DX agent '{name}' not found in config");
		}

		if let Some(c) = candidates.iter().find(|c| c.alias == "dx" && c.enabled && c.dispatchable) {
			return Ok(c.alias.clone());
		}

		let mut enabled: Vec<_> =
			candidates.iter().filter(|c| c.enabled && c.dispatchable).map(|c| c.alias.clone()).collect();
		enabled.sort();
		enabled.into_iter().next().ok_or_else(|| {
			anyhow::anyhow!("No dispatchable DX agent available. Run `dx agents quickstart --agent dx`.")
		})
	}

	fn stream_agent_events<F>(
		mut event_rx: tokio::sync::mpsc::Receiver<dx_agents::agent::TurnEvent>,
		emitted_answer: Arc<AtomicBool>,
		callback: F,
	) -> tokio::task::JoinHandle<Result<()>>
	where
		F: Fn(String) + Send + 'static,
	{
		tokio::spawn(async move {
			let mut translator = AgentStreamTranslator::default();
			while let Some(event) = event_rx.recv().await {
				for output in translator.push(event) {
					if output.answer_delta && !output.text.trim().is_empty() {
						emitted_answer.store(true, Ordering::Relaxed);
					}
					callback(output.text);
				}
			}

			for output in translator.finish() {
				callback(output.text);
			}
			Ok(())
		})
	}

	#[derive(Debug, Default)]
	pub(crate) struct AgentStreamTranslator {
		thinking_open: bool,
		command_open: bool,
		subagent_open: bool,
	}

	impl AgentStreamTranslator {
		pub(crate) fn push(&mut self, event: dx_agents::agent::TurnEvent) -> Vec<AgentStreamOutput> {
			match event {
				TurnEvent::Thinking { delta } => self.thinking(delta),
				TurnEvent::Chunk { delta } => self.answer(delta),
				TurnEvent::ToolCall { name, args, .. } => {
					let is_sub = name.contains("subagent")
						|| name.contains("spawn")
						|| name == "delegate"
						|| name.starts_with("agent_");
					if is_sub {
						let alias = args
							.as_object()
							.and_then(|o| {
								o.get("agent")
									.or_else(|| o.get("alias"))
									.or_else(|| o.get("name"))
									.and_then(|v| v.as_str())
							})
							.unwrap_or(name.as_str());
						let task = args
							.as_object()
							.and_then(|o| {
								o.get("task")
									.or_else(|| o.get("prompt"))
									.or_else(|| o.get("message"))
									.and_then(|v| v.as_str())
							})
							.unwrap_or("");
						let mut out = self.open_subagent(alias);
						if !task.is_empty() {
							let short: String = task.chars().take(120).collect();
							out.extend(self.status(format!("{short}\n")));
						}
						return out;
					}
					let summary = args
						.as_object()
						.and_then(|o| o.get("command").or_else(|| o.get("cmd")))
						.and_then(|v| v.as_str())
						.unwrap_or("")
						.to_string();
					let preview = if summary.is_empty() {
						String::new()
					} else {
						let short: String = summary.chars().take(80).collect();
						format!(" {short}")
					};
					self.open_command(&name, &preview)
				}
				TurnEvent::ToolResult { name, output, .. } => {
					let is_sub = name.contains("subagent")
						|| name.contains("spawn")
						|| name == "delegate"
						|| name.starts_with("agent_");
					if is_sub {
						let mut out = Vec::new();
						if !output.trim().is_empty() {
							let short: String = output.chars().take(8_000).collect();
							out.extend(self.status(format!("{short}\n")));
						}
						out.extend(self.close_subagent());
						return out;
					}
					self.close_command(&name, &output)
				}
				TurnEvent::ApprovalRequest { tool_name, timeout_secs, .. } => self.status(format!(
					"\n```approval\nApproval needed for {tool_name} ({timeout_secs}s timeout)\n```\n"
				)),
				TurnEvent::Usage { .. } => Vec::new(),
			}
		}

		#[allow(dead_code)]
		pub(crate) fn push_text_event(&mut self, kind: &str, payload: &str) -> Vec<AgentStreamOutput> {
			match kind {
				"thinking" => self.thinking(payload.to_string()),
				"answer" => self.answer(payload.to_string()),
				"tool_start" => self.open_command(payload, ""),
				"tool_end" => self.close_command(payload, ""),
				"subagent_start" => self.open_subagent(payload),
				"subagent_end" => self.close_subagent(),
				_ => self.status(payload.to_string()),
			}
		}

		#[allow(dead_code)]
		pub(crate) fn finish(&mut self) -> Vec<AgentStreamOutput> {
			let mut out = Vec::new();
			out.extend(self.close_command_if_open());
			out.extend(self.close_subagent());
			out.extend(self.close_thinking());
			out
		}

		fn thinking(&mut self, delta: String) -> Vec<AgentStreamOutput> {
			let mut output = Vec::new();
			if !self.thinking_open {
				self.thinking_open = true;
				output.push(AgentStreamOutput::status("<think>\n"));
			}
			if !delta.is_empty() {
				output.push(AgentStreamOutput::status(delta));
			}
			output
		}

		fn answer(&mut self, delta: String) -> Vec<AgentStreamOutput> {
			let mut output = self.close_thinking();
			output.extend(self.close_command_if_open());
			if !delta.is_empty() {
				output.push(AgentStreamOutput::answer(delta));
			}
			output
		}

		fn status(&mut self, text: String) -> Vec<AgentStreamOutput> {
			let mut output = self.close_thinking();
			if !text.is_empty() {
				output.push(AgentStreamOutput::status(text));
			}
			output
		}

		fn open_command(&mut self, name: &str, preview: &str) -> Vec<AgentStreamOutput> {
			let mut output = self.close_thinking();
			output.extend(self.close_command_if_open());
			self.command_open = true;
			let p = preview.trim().trim_start_matches(' ');
			output.push(AgentStreamOutput::status(crate::tools::format_tool_running(name, p)));
			output
		}

		fn close_command(&mut self, name: &str, output_text: &str) -> Vec<AgentStreamOutput> {
			self.command_open = false;
			let title =
				crate::tools::ToolKind::from_name(name).map(|k| k.display_title()).unwrap_or("Tool");
			let body = if output_text.is_empty() {
				"(no output)".to_string()
			} else {
				output_text.chars().take(48_000).collect()
			};
			let result = crate::tools::ToolResult {
				call_id: String::new(),
				name: name.to_string(),
				ok: true,
				title: title.to_string(),
				output: body,
				preview: String::new(),
			};
			vec![AgentStreamOutput::status(crate::tools::format_tool_result(&result))]
		}

		fn close_command_if_open(&mut self) -> Vec<AgentStreamOutput> {
			if self.command_open {
				self.command_open = false;
				Vec::new()
			} else {
				Vec::new()
			}
		}

		fn open_subagent(&mut self, name: &str) -> Vec<AgentStreamOutput> {
			let mut output = self.close_thinking();
			output.extend(self.close_command_if_open());
			output.extend(self.close_subagent());
			self.subagent_open = true;
			output.push(AgentStreamOutput::status(format!("\n<subagent name=\"{name}\">\n")));
			output
		}

		fn close_subagent(&mut self) -> Vec<AgentStreamOutput> {
			if self.subagent_open {
				self.subagent_open = false;
				vec![AgentStreamOutput::status("\n</subagent>\n")]
			} else {
				Vec::new()
			}
		}

		fn close_thinking(&mut self) -> Vec<AgentStreamOutput> {
			if self.thinking_open {
				self.thinking_open = false;
				vec![AgentStreamOutput::status("\n</think>\n")]
			} else {
				Vec::new()
			}
		}
	}

	#[derive(Debug, Clone, PartialEq, Eq)]
	pub(crate) struct AgentStreamOutput {
		text: String,
		answer_delta: bool,
	}

	impl AgentStreamOutput {
		fn answer(text: String) -> Self {
			Self { text, answer_delta: true }
		}

		fn status(text: impl Into<String>) -> Self {
			Self { text: text.into(), answer_delta: false }
		}
	}

	#[cfg(test)]
	mod tests {
		use super::*;

		#[test]
		fn stream_translator_wraps_thinking_before_answer() {
			let mut translator = AgentStreamTranslator::default();
			let mut chunks = translator.push_text_event("thinking", "checking");
			chunks.extend(translator.push_text_event("answer", "done"));

			let rendered = chunks.into_iter().map(|chunk| chunk.text).collect::<String>();
			assert_eq!(rendered, "<think>\nchecking\n</think>\ndone");
		}

		#[test]
		fn stream_translator_emits_command_fences() {
			let mut translator = AgentStreamTranslator::default();
			let mut chunks = translator.push_text_event("tool_start", "shell");
			chunks.extend(translator.push_text_event("tool_end", "shell"));
			let rendered = chunks.into_iter().map(|c| c.text).collect::<String>();
			assert!(rendered.contains("```command name=\"shell\""));
			assert!(rendered.contains("```"));
		}

		#[test]
		fn stream_translator_closes_open_thinking_on_finish() {
			let mut translator = AgentStreamTranslator::default();
			let _ = translator.push_text_event("thinking", "plan");
			let rendered = translator.finish().into_iter().map(|c| c.text).collect::<String>();
			assert_eq!(rendered, "\n</think>\n");
		}

		#[test]
		fn stream_translator_subagent_markers() {
			let mut translator = AgentStreamTranslator::default();
			let mut chunks = translator.push_text_event("subagent_start", "explore");
			chunks.extend(translator.push_text_event("subagent_end", ""));
			let rendered = chunks.into_iter().map(|c| c.text).collect::<String>();
			assert!(rendered.contains("<subagent name=\"explore\">"));
			assert!(rendered.contains("</subagent>"));
		}
	}
}

#[cfg(feature = "dx-agent")]
pub use inner::*;

#[cfg(not(feature = "dx-agent"))]
mod stub {
	use std::sync::Arc;
	use tokio::sync::Mutex;

	#[derive(Clone)]
	pub struct AgentBackend {
		_private: Arc<Mutex<()>>,
	}

	impl AgentBackend {
		pub fn new() -> Self {
			Self { _private: Arc::new(Mutex::new(())) }
		}

		pub async fn initialize(&self) -> anyhow::Result<super::AgentMetadata> {
			anyhow::bail!("dx-agent feature not enabled")
		}

		pub fn is_ready(&self) -> bool {
			false
		}

		pub async fn generate_stream<F>(&self, _prompt: &str, _callback: F) -> anyhow::Result<()>
		where
			F: Fn(String) + Send + 'static,
		{
			anyhow::bail!("dx-agent feature not enabled")
		}

		pub async fn generate_stream_for_session<F>(
			&self,
			_session_id: Option<&str>,
			_prompt: &str,
			_history: &[(String, String)],
			_callback: F,
		) -> anyhow::Result<()>
		where
			F: Fn(String) + Send + 'static,
		{
			anyhow::bail!("dx-agent feature not enabled")
		}

		pub async fn reset_session(&self, _session_id: Option<&str>) {}
	}

	impl Default for AgentBackend {
		fn default() -> Self {
			Self::new()
		}
	}
}

#[cfg(not(feature = "dx-agent"))]
pub use stub::*;
