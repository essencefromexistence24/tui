//! Hermes-inspired LLM-powered background review.
//!
//! After every N assistant turns, spawns a mini agent (with only `memory` + `skill_manage`
//! tools) to analyze the recent conversation and decide what to save:
//! - User preferences / conventions → `memory` tool → USER.md
//! - Environment facts / tool quirks → `memory` tool → MEMORY.md
//! - Reusable workflows / patterns → `skill_manage` tool → SKILL.md
//! - Error recoveries / pitfalls → `skill_manage` tool → patch existing skill
//!
//! This replaces the old regex-based text matching with an actual LLM judgement.

use std::time::Instant;

use crate::modes::AgentMode;
use crate::tools::openai_tool_schemas;
use crate::zen;

/// How often to trigger a review (in assistant turns).
const REVIEW_INTERVAL: usize = 3;
/// Minimum conversation length before first review.
const MIN_MESSAGES: usize = 4;

#[derive(Debug, Clone)]
pub struct ReviewTrigger {
	last_review_at: Instant,
	turns_since_review: usize,
}

impl Default for ReviewTrigger {
	fn default() -> Self {
		Self { last_review_at: Instant::now(), turns_since_review: 0 }
	}
}

impl ReviewTrigger {
	pub fn should_review(&self, total_assistant_turns: usize) -> bool {
		total_assistant_turns >= MIN_MESSAGES && self.turns_since_review >= REVIEW_INTERVAL
	}

	pub fn tick(&mut self) {
		self.turns_since_review = self.turns_since_review.saturating_add(1);
	}

	pub fn reset(&mut self) {
		self.turns_since_review = 0;
		self.last_review_at = Instant::now();
	}
}

/// The review prompt shown to the mini-agent.
/// Instructs it to examine recent conversation and produce memory + skill updates.
const REVIEW_SYSTEM_PROMPT: &str = r#"You are a self-improvement review agent for a coding assistant.
Your ONLY job is to analyze the recent conversation and decide if anything should be saved.
You have access to two tools:

1. `memory` — save short factual entries:
   - target="memory": agent's notes about the project, environment, tool behavior
   - target="user": user preferences, coding style, workflow habits
   Use action=memory_add to add a new entry.

2. `skill_manage` — save reusable workflows:
   - action=create name=<slug> description=<1 line> content=<full markdown>
   - action=patch name=<existing-slug> content=<new section to append>

Rules:
- Save USER preferences when the user states how they like things done
- Save MEMORY facts when you learn about project structure, tool quirks, conventions
- Create a SKILL when you see a non-trivial multi-step workflow repeated or explained
- Patch an existing skill when you learn a better way to do something already documented
- Be conservative: one-off fixes are NOT skill-worthy
- Maximum 3 actions per review
- If nothing worth saving, do nothing — that's the right answer too"#;

/// Run LLM-powered background review.
/// Spawns a mini agent call with memory + skill_manage tools.
pub async fn run_llm_review(
	messages: &[String],
	model: &str,
	api_url: Option<&str>,
	tx: std::sync::mpsc::Sender<String>,
) {
	if messages.len() < 2 {
		return;
	}

	// Build the conversation for the review agent
	let conversation = messages
		.iter()
		.rev()
		.take(10)
		.collect::<Vec<_>>()
		.into_iter()
		.rev()
		.enumerate()
		.map(|(i, m)| if i % 2 == 0 { format!("[user]\n{m}\n") } else { format!("[assistant]\n{m}\n") })
		.collect::<Vec<_>>()
		.join("\n");

	let review_prompt = format!(
		"Review the following recent conversation and decide what to save:\n\n{conversation}\n\n---\n\
		 Current skills: {}\n---\n\
		 Review the conversation above. Use the tools available to save memories or skills if appropriate. \
		 If nothing needs saving, simply respond with 'No updates needed.'",
		crate::skills::list_skills()
			.iter()
			.map(|s| format!("- {}: {}", s.name, s.description))
			.collect::<Vec<_>>()
			.join("\n")
	);

	// Build messages for the LLM call
	let system_msg = zen::ApiMessage::system(REVIEW_SYSTEM_PROMPT);
	let user_msg = zen::ApiMessage {
		role: "user".into(),
		content: Some(review_prompt),
		tool_call_id: None,
		tool_calls: None,
		name: None,
	};
	let zen_messages = vec![system_msg, user_msg];

	// Only expose memory + skill_manage tools to the review agent
	let tools: Vec<serde_json::Value> = openai_tool_schemas(AgentMode::Agent)
		.into_iter()
		.filter(|t| {
			t.get("function")
				.and_then(|f| f.get("name"))
				.and_then(|n| n.as_str())
				.map(|name| name == "memory" || name == "skill_manage" || name == "todowrite")
				.unwrap_or(false)
		})
		.collect();

	let tools_ref: Option<&[serde_json::Value]> = if tools.is_empty() { None } else { Some(&tools) };

	let turn = zen::stream_chat_messages(model, &zen_messages, tools_ref, api_url, tx.clone()).await;

	let Ok(turn) = turn else {
		return;
	};

	// Process tool calls from the review agent
	for call in &turn.tool_calls {
		let kind = crate::tools::ToolKind::from_name(&call.name);
		let args: serde_json::Value =
			serde_json::from_str(&call.arguments).unwrap_or(serde_json::json!({}));

		match kind {
			Some(crate::tools::ToolKind::Memory) => {
				let result = crate::memory_tool::MemoryStore::execute_tool(call, &args);
				if result.ok {
					let _ = tx.send(format!("\n__UPDATE_STATUS__\nBackground review: {}\n", result.title));
				}
			}
			Some(crate::tools::ToolKind::SkillManage) => {
				let result = crate::skills::execute_skill_manage(call);
				if result.ok {
					let _ = tx.send(format!("\n__UPDATE_STATUS__\nBackground review: {}\n", result.title));
				}
			}
			_ => {}
		}
	}
}

/// Spawn LLM-powered background review as a tokio task.
pub fn spawn_llm_review(
	messages: Vec<String>,
	model: String,
	api_url: Option<String>,
	tx: std::sync::mpsc::Sender<String>,
) {
	tokio::spawn(async move {
		run_llm_review(&messages, &model, api_url.as_deref(), tx).await;
	});
}
