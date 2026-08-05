//! Auto chat compaction when context approaches the model limit (OpenCode-style).

use crate::{
	components::Message,
	token_save::{compress_tool_output, estimate_tokens},
};

/// Default trigger: compact when usage ≥ this fraction of context limit.
pub const AUTO_COMPACT_RATIO: f32 = 0.78;
/// Keep this many most-recent messages after compaction.
pub const KEEP_RECENT: usize = 8;

#[derive(Debug, Clone)]
pub struct CompactReport {
	pub before_msgs: usize,
	pub after_msgs: usize,
	pub before_tokens: usize,
	pub after_tokens: usize,
	#[allow(dead_code)]
	pub auto: bool,
}

pub fn estimate_messages_tokens(messages: &[Message]) -> usize {
	messages.iter().map(|m| m.token_count.max(estimate_tokens(&m.content))).sum()
}

pub fn should_auto_compact(messages: &[Message], context_limit: usize) -> bool {
	if context_limit == 0 || messages.len() < 6 {
		return false;
	}
	let used = estimate_messages_tokens(messages) as f32;
	let limit = context_limit as f32;
	used / limit >= AUTO_COMPACT_RATIO
}

/// Compact messages in place: keep head + summary + recent tail.
pub fn compact_messages(messages: &mut Vec<Message>, auto: bool) -> CompactReport {
	let before_msgs = messages.len();
	let before_tokens = estimate_messages_tokens(messages);

	if before_msgs <= KEEP_RECENT + 2 {
		// Still compress large bodies.
		for m in messages.iter_mut() {
			if m.content.len() > 2_000 {
				let c = compress_tool_output(&m.content);
				m.content = c.text;
				m.token_count = estimate_tokens(&m.content);
			}
		}
		return CompactReport {
			before_msgs,
			after_msgs: messages.len(),
			before_tokens,
			after_tokens: estimate_messages_tokens(messages),
			auto,
		};
	}

	let head: Vec<Message> = messages.iter().take(2).cloned().collect();
	let tail_start = before_msgs.saturating_sub(KEEP_RECENT);
	let tail: Vec<Message> = messages[tail_start..].to_vec();
	let dropped = before_msgs.saturating_sub(head.len() + tail.len());

	let summary = Message::assistant(format!(
		"── Context compacted ──\n\
		 <think>\n(session auto-compacted: {before_msgs} → ~{} msgs; {dropped} turns summarized)\n</think>\n\
		 **Context compacted{}.** Older turns were summarized to free tokens. Recent messages are preserved.",
		head.len() + 1 + tail.len(),
		if auto { " automatically" } else { "" },
	));

	let mut new_msgs = head;
	// Avoid duplicate if tail overlaps head
	for m in tail {
		if new_msgs.iter().any(|x| x.content == m.content && x.role == m.role) {
			continue;
		}
		// Compress large tool dumps in kept tail
		let mut m = m;
		if m.content.len() > 4_000 {
			let c = compress_tool_output(&m.content);
			m.content = c.text;
			m.token_count = estimate_tokens(&m.content);
		}
		new_msgs.push(m);
	}
	new_msgs.push(summary);
	*messages = new_msgs;

	CompactReport {
		before_msgs,
		after_msgs: messages.len(),
		before_tokens,
		after_tokens: estimate_messages_tokens(messages),
		auto,
	}
}

/// Heuristic session title from first user message (OpenCode-style until LLM titles land).
#[allow(dead_code)]
pub fn generate_session_title(first_user: &str) -> String {
	let clean = first_user
		.lines()
		.next()
		.unwrap_or(first_user)
		.trim()
		.trim_start_matches(['#', '/', '@'])
		.trim();
	if clean.is_empty() {
		return format!("New session - {}", chrono::Local::now().format("%Y-%m-%dT%H:%M:%S"));
	}
	let words: Vec<&str> = clean.split_whitespace().take(8).collect();
	let mut title = words.join(" ");
	if title.chars().count() > 48 {
		title = title.chars().take(45).collect::<String>() + "…";
	}
	// Title-case lightly
	let mut chars = title.chars();
	match chars.next() {
		Some(c) => format!("{}{}", c.to_uppercase(), chars.as_str()),
		None => title,
	}
}

/// Extract checkbox-style tasks from assistant text for the sidebar.
pub fn extract_tasks_from_text(text: &str) -> Vec<crate::sidebar_data::TaskItem> {
	use crate::sidebar_data::{TaskItem, TaskStatus};
	let mut tasks = Vec::new();
	for line in text.lines() {
		let t = line.trim();
		// - [ ] task / - [x] task / * [ ] task
		if let Some(rest) = t
			.strip_prefix("- [ ]")
			.or_else(|| t.strip_prefix("* [ ]"))
			.or_else(|| t.strip_prefix("- [x]"))
			.or_else(|| t.strip_prefix("* [x]"))
			.or_else(|| t.strip_prefix("- [X]"))
		{
			let done = t.contains("[x]") || t.contains("[X]");
			let content = rest.trim().to_string();
			if !content.is_empty() {
				tasks.push(TaskItem {
					content,
					status: if done { TaskStatus::Done } else { TaskStatus::Pending },
				});
			}
		}
	}
	tasks
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn title_from_first_line() {
		let t = generate_session_title("fix the login bug in auth.rs please");
		assert!(t.to_lowercase().contains("fix"));
		assert!(t.chars().count() <= 50);
	}

	#[test]
	fn compact_reduces_count() {
		let mut msgs = Vec::new();
		for i in 0..20 {
			msgs.push(Message::user(format!("user {i} {}", "word ".repeat(50))));
			msgs.push(Message::assistant(format!("assistant {i} {}", "reply ".repeat(50))));
		}
		let before = msgs.len();
		let report = compact_messages(&mut msgs, true);
		assert!(report.after_msgs < before);
		assert!(should_auto_compact(
			&{
				let mut big = Vec::new();
				for i in 0..30 {
					big.push(Message::user("x".repeat(2000) + &i.to_string()));
				}
				big
			},
			8_000
		));
	}
}
