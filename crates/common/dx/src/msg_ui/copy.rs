//! Clean clipboard text from assistant message content (no protocol soup).

use super::parse::{PartStatus, StreamPart, parse_stream_parts};

/// Human-readable copy of a message: answers + tool summaries, no fences/XML.
pub fn clean_copy_text(content: &str) -> String {
	let parts = parse_stream_parts(content, None);
	if parts.is_empty() {
		return strip_protocol_fallback(content);
	}
	let mut out = String::new();
	for part in parts {
		match part {
			StreamPart::Text { body } => {
				if !out.is_empty() {
					out.push_str("\n\n");
				}
				out.push_str(body.trim());
			}
			StreamPart::Thinking { body, duration, .. } => {
				if body.trim().is_empty() {
					continue;
				}
				if !out.is_empty() {
					out.push('\n');
				}
				let dur = duration.map(|d| format!(" ({})", format_dur(d))).unwrap_or_default();
				out.push_str(&format!("[Thought{dur}]\n{}", body.trim()));
			}
			StreamPart::Tool { title, status, preview, body, duration, .. } => {
				if !out.is_empty() {
					out.push('\n');
				}
				let st = match status {
					PartStatus::Running => "running",
					PartStatus::Done => "ok",
					PartStatus::Error => "error",
				};
				let dur = duration.map(|d| format!(" · {}", format_dur(d))).unwrap_or_default();
				let prev = if preview.is_empty() { String::new() } else { format!(" — {preview}") };
				out.push_str(&format!("[{title} · {st}{dur}]{prev}"));
				if !body.trim().is_empty() {
					out.push('\n');
					// Cap huge tool dumps in clipboard
					let t = body.trim();
					if t.len() > 12_000 {
						out.push_str(&t[..12_000]);
						out.push_str("\n…");
					} else {
						out.push_str(t);
					}
				}
			}
			StreamPart::Subagent { name, body, .. } => {
				if !out.is_empty() {
					out.push('\n');
				}
				out.push_str(&format!("[Subagent · {name}]"));
				if !body.trim().is_empty() {
					out.push('\n');
					out.push_str(body.trim());
				}
			}
			StreamPart::Approval { tool, body, decision, .. } => {
				if !out.is_empty() {
					out.push('\n');
				}
				out.push_str(&format!("[Approval · {tool} · {decision}]"));
				if !body.is_empty() {
					out.push('\n');
					out.push_str(&body);
				}
			}
			StreamPart::Question { prompt, options, answer, .. } => {
				if !out.is_empty() {
					out.push('\n');
				}
				out.push_str(&format!("[Question] {prompt}"));
				if !answer.is_empty() {
					out.push_str(&format!("\n→ {answer}"));
				} else {
					for (i, o) in options.iter().enumerate() {
						out.push_str(&format!("\n  {}. {o}", i + 1));
					}
				}
			}
			StreamPart::Compaction { label, summary } => {
				if !out.is_empty() {
					out.push('\n');
				}
				out.push_str(&format!("— {label} —"));
				if !summary.is_empty() {
					out.push('\n');
					out.push_str(&summary);
				}
			}
			StreamPart::Error { body } => {
				if !out.is_empty() {
					out.push('\n');
				}
				out.push_str(&format!("Error: {body}"));
			}
			StreamPart::Retry { body } => {
				if !out.is_empty() {
					out.push('\n');
				}
				out.push_str(&format!("Retry: {body}"));
			}
			StreamPart::ContextGroup { label, .. } => {
				if !out.is_empty() {
					out.push('\n');
				}
				out.push_str(&label);
			}
			StreamPart::Plan { title, body, steps } => {
				if !out.is_empty() {
					out.push('\n');
				}
				out.push_str(&format!("[{title}]\n{body}"));
				for s in steps {
					out.push_str(&format!("\n{} {}", if s.done { "[x]" } else { "[ ]" }, s.text));
				}
			}
			StreamPart::Pty { title, lines, attached, alive, .. } => {
				if !out.is_empty() {
					out.push('\n');
				}
				out.push_str(&format!(
					"[Terminal · {title}{}{}]\n",
					if attached { " · attached" } else { "" },
					if alive { "" } else { " · ended" }
				));
				for l in lines.iter().rev().take(40).collect::<Vec<_>>().into_iter().rev() {
					out.push_str(l);
					out.push('\n');
				}
			}
			StreamPart::Interrupted => {
				if !out.is_empty() {
					out.push('\n');
				}
				out.push_str("[Interrupted]");
			}
		}
	}
	out.trim().to_string()
}

fn format_dur(d: std::time::Duration) -> String {
	let ms = d.as_millis();
	if ms < 1000 {
		format!("{ms}ms")
	} else if ms < 60_000 {
		format!("{:.1}s", d.as_secs_f32())
	} else {
		format!("{}m{:02}s", d.as_secs() / 60, d.as_secs() % 60)
	}
}

fn strip_protocol_fallback(content: &str) -> String {
	let mut s = content.to_string();
	for tag in ["think", "thinking", "tool_call", "tool_result", "subagent"] {
		// crude strip
		while let Some(start) = s.find(&format!("<{tag}")) {
			if let Some(end) = s[start..].find('>') {
				let close = format!("</{tag}>");
				if let Some(c) = s[start + end..].find(&close) {
					s.replace_range(start..start + end + c + close.len(), "");
				} else {
					s.replace_range(start..start + end + 1, "");
				}
			} else {
				break;
			}
		}
	}
	// Strip command fences markers
	s = s
		.lines()
		.filter(|l| {
			let t = l.trim();
			!(t.starts_with("```command") || t == "```approval")
		})
		.collect::<Vec<_>>()
		.join("\n");
	s.trim().to_string()
}
