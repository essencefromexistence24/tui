//! First-turn session meta: title + todos (DX-branded).

use crate::sidebar_data::{TaskItem, TaskStatus};

/// Remove `<think>` / `<thinking>` blocks so TITLE is found in the real answer.
pub fn strip_think_blocks(text: &str) -> String {
	let mut out = String::with_capacity(text.len());
	let mut rest = text;
	loop {
		let open = rest.find("<think>").into_iter().chain(rest.find("<thinking>")).min();
		let Some(start) = open else {
			out.push_str(rest);
			break;
		};
		out.push_str(&rest[..start]);
		let after_open = &rest[start..];
		let close = if after_open.starts_with("<thinking>") {
			after_open.find("</thinking>").map(|i| i + "</thinking>".len())
		} else {
			after_open.find("</think>").map(|i| i + "</think>".len())
		};
		match close {
			Some(end) => rest = &after_open[end..],
			None => {
				// Unclosed think — drop the rest (still streaming)
				break;
			}
		}
	}
	out
}

/// Parse `TITLE: …` from assistant text (prefers content outside thinking blocks).
pub fn parse_title_line(text: &str) -> Option<String> {
	// 1) Prefer answer body after thinking is stripped
	if let Some(t) = parse_title_in_region(&strip_think_blocks(text)) {
		return Some(t);
	}
	// 2) Full raw text (TITLE sometimes appears before tools)
	parse_title_in_region(text)
}

fn parse_title_in_region(text: &str) -> Option<String> {
	for line in text.lines() {
		if let Some(title) = extract_title_from_line(line) {
			return Some(title);
		}
	}
	// Inline: "... TITLE: Foo bar\n" mid-stream without clean line breaks
	for part in text.split('\n') {
		if let Some(idx) = part.to_ascii_uppercase().find("TITLE:") {
			let after = part[idx + "TITLE:".len()..].trim();
			if let Some(t) = clean_title_candidate(after) {
				return Some(t);
			}
		}
	}
	None
}

fn extract_title_from_line(line: &str) -> Option<String> {
	let mut t = line.trim();
	// Strip common list / quote / markdown wrappers
	while let Some(c) = t.chars().next() {
		if matches!(c, '*' | '-' | '>' | '#' | '`' | ' ' | '\t') {
			t = t[c.len_utf8()..].trim_start();
		} else {
			break;
		}
	}
	// **TITLE:** / __TITLE__: / TITLE:
	let upper = t.to_ascii_uppercase();
	let rest = strip_title_prefix(t, &upper)?;
	clean_title_candidate(rest)
}

fn strip_title_prefix<'a>(original: &'a str, upper: &str) -> Option<&'a str> {
	// Work on original with case-insensitive "TITLE" detection
	let u = upper.trim_start();
	let skip = original.len().saturating_sub(u.len());
	let orig = original.get(skip..).unwrap_or(original).trim_start();

	// Patterns (case-insensitive on TITLE)
	let patterns = [
		"TITLE:",
		"TITLE :",
		"**TITLE:**",
		"**TITLE**:",
		"__TITLE__:",
		"# TITLE:",
		"## TITLE:",
		"TITLE -",
		"TITLE —",
		"TITLE –",
	];
	let ou = orig.to_ascii_uppercase();
	for pat in patterns {
		if ou.starts_with(pat) {
			return Some(orig[pat.len()..].trim());
		}
	}
	// "Title: foo" mixed case already covered by uppercase compare on prefix length
	if ou.starts_with("TITLE") {
		// TITLE then optional markup then colon
		if let Some(colon) = orig.find(':') {
			let head = orig[..colon].trim();
			let head_plain: String = head.chars().filter(|c| c.is_alphabetic()).collect();
			if head_plain.eq_ignore_ascii_case("title") {
				return Some(orig[colon + 1..].trim());
			}
		}
	}
	None
}

fn clean_title_candidate(rest: &str) -> Option<String> {
	let title = rest
		.trim()
		.trim_matches('`')
		.trim_matches('*')
		.trim_matches('_')
		.trim()
		.trim_matches('"')
		.trim_matches('\'')
		.trim()
		// Drop trailing markdown junk
		.trim_end_matches(['*', '_', '`', '#'])
		.trim();
	if title.is_empty() {
		return None;
	}
	// Reject lines that look like instructions / not a name
	let lower = title.to_ascii_lowercase();
	if lower.starts_with("http") || lower.starts_with('<') {
		return None;
	}
	// Prefer long sentence-length names; the sidebar wraps them across ≥3 lines.
	let words = title.split_whitespace().count();
	if words == 0 || words > 48 {
		return None;
	}
	// Reject short stubs — need enough words to wrap across multiple sidebar lines.
	if words < 6 {
		return None;
	}
	if title.chars().count() < 36 {
		return None;
	}
	// Cap length while keeping enough text for multi-line sidebar wrap (~3–5 rows).
	let mut titled = if title.chars().count() > 200 {
		let mut t: String = title.chars().take(198).collect();
		t = t.trim().to_string();
		if !t.ends_with('…') {
			t.push('…');
		}
		t
	} else {
		title.to_string()
	};
	// Title-case first char; keep full phrase (not one truncated token)
	let mut chars = titled.chars();
	titled = match chars.next() {
		Some(c) => format!("{}{}", c.to_uppercase(), chars.as_str()),
		None => return None,
	};
	Some(titled)
}

/// Fallback session name from the first user message when the model omits TITLE:.
#[allow(dead_code)]
pub fn heuristic_session_title(user_text: &str) -> String {
	compact_session_title(user_text)
}

/// Filler openers stripped so the sidebar name is a topic, not a raw prompt dump.
const TITLE_FILLERS: &[&str] = &[
	"please",
	"pls",
	"can you",
	"could you",
	"would you",
	"will you",
	"i want you to",
	"i need you to",
	"i need",
	"i want",
	"help me",
	"help me to",
	"hey",
	"hi",
	"hello",
	"ok so",
	"okay so",
	"so ",
	"now ",
	"just ",
	"kindly",
	"wow so",
	"um",
	"uh",
];

/// Chat name for the sidebar: a long topic-style title (not a raw prompt paste).
/// Sized to wrap across **at least ~3** sidebar rows (~40-col panel).
pub fn compact_session_title(user_text: &str) -> String {
	let source = user_text
		.lines()
		.map(str::trim)
		.filter(|line| {
			!line.is_empty()
				&& !line.starts_with("![")
				&& !line.starts_with("file:")
				&& !line.starts_with("```")
				&& !line.starts_with('#')
		})
		.collect::<Vec<_>>()
		.join(" ");
	let cleaned = source.trim_start_matches('/').trim();
	// Keep letters/digits/spaces/hyphen only for a clean name
	let mut buf = String::new();
	for c in cleaned.chars() {
		if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' {
			buf.push(c);
		} else if c == '\n' || c == '\t' || matches!(c, ',' | '.' | ':' | ';' | '!' | '?') {
			buf.push(' ');
		}
	}
	let mut s = buf.split_whitespace().collect::<Vec<_>>().join(" ");
	// Strip chatty openers repeatedly.
	loop {
		let lower = s.to_ascii_lowercase();
		let mut stripped = false;
		for f in TITLE_FILLERS {
			if lower.starts_with(f) {
				let rest = s[f.len()..].trim_start();
				if !rest.is_empty() {
					s = rest.to_string();
					stripped = true;
					break;
				}
			}
		}
		if !stripped {
			break;
		}
	}
	// Enough words/chars for ≥3 wrapped sidebar lines (~36–40 cols each → ~110+ chars).
	let mut words: Vec<&str> = s.split_whitespace().take(56).collect();
	if words.is_empty() {
		return "Chat".into();
	}
	// Soft topic framing when the remainder still reads like a bare command.
	let body = words.join(" ");
	let framed = if body.chars().count() < 48 {
		// Pad short prompts into a fuller session label.
		format!("Working through: {body} — full chat goals and follow-up context")
	} else if !body.to_ascii_lowercase().starts_with("working")
		&& !body.to_ascii_lowercase().starts_with("fix")
		&& !body.to_ascii_lowercase().starts_with("build")
		&& !body.to_ascii_lowercase().starts_with("implement")
		&& !body.to_ascii_lowercase().starts_with("investigate")
		&& !body.to_ascii_lowercase().starts_with("diagnose")
		&& !body.to_ascii_lowercase().starts_with("review")
	{
		format!("Chat about {body}")
	} else {
		body
	};
	words = framed.split_whitespace().take(56).collect();
	let mut s = words.join(" ");

	// Target ~3–5 lines at ~40-col sidebar width (~120–180 chars).
	if s.chars().count() > 180 {
		s = s.chars().take(178).collect::<String>().trim().to_string();
		if !s.ends_with('…') {
			s.push('…');
		}
	}
	s = s.trim().trim_end_matches(['.', ',', ';', ':']).trim().to_string();
	if s.is_empty() {
		return "Chat".into();
	}
	// Title-case first char
	let mut chars = s.chars();
	match chars.next() {
		Some(c) => format!("{}{}", c.to_uppercase(), chars.as_str()),
		None => "Chat".into(),
	}
}

/// Single-line ellipsize by Unicode display width (no wrap).
pub fn ellipsize_one_line(name: &str, max_cols: usize) -> String {
	use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};
	if max_cols == 0 {
		return String::new();
	}
	let n = name.trim();
	if n.is_empty() {
		return String::new();
	}
	if n.width() <= max_cols {
		return n.to_string();
	}
	if max_cols <= 1 {
		return "…".into();
	}
	let keep = max_cols.saturating_sub(1);
	let mut out = String::new();
	let mut cols = 0usize;
	for ch in n.chars() {
		let w = ch.width().unwrap_or(0);
		if cols + w > keep {
			break;
		}
		out.push(ch);
		cols += w;
	}
	out.push('…');
	out
}

/// Remove TITLE meta lines from displayed assistant content.
pub fn strip_title_lines(text: &str) -> String {
	text
		.lines()
		.filter(|line| !should_hide_title_display_line(line, /*streaming*/ false))
		.collect::<Vec<_>>()
		.join("\n")
		.trim()
		.to_string()
}

/// True when a line is session-meta TITLE (complete or incomplete while streaming).
/// Used so the chat paint never flashes `TITLE:` / partial title tokens.
pub fn should_hide_title_display_line(line: &str, streaming: bool) -> bool {
	if extract_title_from_line(line).is_some() {
		return true;
	}
	if !streaming {
		return false;
	}
	// Incomplete stream prefixes: "T", "TI", "TITLE", "TITLE:", "**TITLE", etc.
	let t = line.trim();
	if t.is_empty() {
		return false;
	}
	let stripped = t.trim_start_matches(['*', '_', '#', '>', '-', ' ']);
	let upper = stripped.to_ascii_uppercase();
	if upper == "T"
		|| upper == "TI"
		|| upper == "TIT"
		|| upper == "TITL"
		|| upper == "TITLE"
		|| upper.starts_with("TITLE:")
		|| upper.starts_with("TITLE ")
		|| upper.starts_with("TITLE-")
		|| upper.starts_with("TITLE—")
		|| upper.starts_with("TITLE–")
	{
		return true;
	}
	// "**TITLE" without colon yet
	let plain: String = stripped.chars().filter(|c| c.is_alphabetic()).collect();
	plain.eq_ignore_ascii_case("title")
}

/// Strip TITLE meta (and incomplete TITLE prefixes while streaming) for paint.
pub fn sanitize_assistant_display_text(text: &str, streaming: bool) -> String {
	text
		.lines()
		.filter(|line| !should_hide_title_display_line(line, streaming))
		.collect::<Vec<_>>()
		.join("\n")
}

/// Extract checklist todos (max 12).
pub fn extract_todos(text: &str) -> Vec<TaskItem> {
	let mut out = Vec::new();
	for line in text.lines() {
		let t = line.trim();
		let (done, rest) = if let Some(r) = t.strip_prefix("- [ ]").or_else(|| t.strip_prefix("* [ ]"))
		{
			(false, r)
		} else if let Some(r) = t
			.strip_prefix("- [x]")
			.or_else(|| t.strip_prefix("- [X]"))
			.or_else(|| t.strip_prefix("* [x]"))
			.or_else(|| t.strip_prefix("* [X]"))
		{
			(true, r)
		} else {
			continue;
		};
		let content = rest.trim().to_string();
		if content.is_empty() {
			continue;
		}
		out.push(TaskItem {
			content,
			status: if done { TaskStatus::Done } else { TaskStatus::Pending },
		});
		if out.len() >= 12 {
			break;
		}
	}
	out
}

/// Apply AI-generated title + todos.
///
/// AI `TITLE:` **always wins** while the session name is still provisional
/// (including heuristic fallbacks from the user prompt).
pub fn apply_first_turn_meta(
	content: &str,
	session_name: &mut String,
	title_from_ai: &mut bool,
) -> (Option<String>, Vec<TaskItem>) {
	let todos = extract_todos(content);
	let mut new_title = None;
	if let Some(title) = parse_title_line(content) {
		// Prefer AI title over provisional / heuristic names.
		if !*title_from_ai || is_provisional_session_name(session_name) {
			*session_name = title.clone();
			*title_from_ai = true;
			new_title = Some(title);
		}
	}
	(new_title, todos)
}

/// Names we treat as placeholders until a real AI title is applied.
pub fn is_provisional_session_name(name: &str) -> bool {
	let n = name.trim();
	n.is_empty()
		|| n.starts_with("Session ")
		|| n.starts_with("New session")
		|| n == "…"
		|| n == "..."
		|| n == "Chat"
		|| n.eq_ignore_ascii_case("generating...")
		|| n.eq_ignore_ascii_case("generating…")
		// Trailing ellipsis only when the name is a cut-off stub (very short)
		|| ((n.ends_with('…') || n.ends_with("...")) && n.chars().count() < 8)
}

/// True while the sidebar should shimmer "Generating..." for the chat name.
/// Once we have a real name (AI TITLE: or settled fallback), stop shimmering.
pub fn should_show_generating_title(name: &str, title_from_ai: bool, is_loading: bool) -> bool {
	if title_from_ai {
		return false;
	}
	if is_loading {
		return true;
	}
	// Only the waiting placeholder — not "Chat" or a real title
	let n = name.trim();
	n.is_empty() || n == "…" || n == "..." || n.eq_ignore_ascii_case("generating...")
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn parse_title() {
		let t = "TITLE: Fix the auth bug blocking password resets after deploy\n\nHere is the plan.";
		assert_eq!(
			parse_title_line(t).as_deref(),
			Some("Fix the auth bug blocking password resets after deploy")
		);
		assert!(!strip_title_lines(t).contains("TITLE:"));
	}

	#[test]
	fn parse_title_markdownish() {
		assert_eq!(
			parse_title_line(
				"**TITLE:** Ship multi-line sidebar titles correctly for every session\nbody"
			)
			.as_deref(),
			Some("Ship multi-line sidebar titles correctly for every session")
		);
	}

	#[test]
	fn parse_title_after_long_think() {
		let mut think = String::from("<think>\n");
		for i in 0..60 {
			think.push_str(&format!("reasoning line {i}\n"));
		}
		think.push_str(
			"</think>\nTITLE: Count repository files and summarize the project layout clearly\n\nYou have 12 files.",
		);
		assert_eq!(
			parse_title_line(&think).as_deref(),
			Some("Count repository files and summarize the project layout clearly"),
			"must find TITLE after long think block"
		);
	}

	#[test]
	fn parse_title_flexible_colon() {
		assert_eq!(
			parse_title_line("Title : Hello World from the session header spanning three lines\nbody")
				.as_deref(),
			Some("Hello World from the session header spanning three lines")
		);
	}

	#[test]
	fn reject_too_short_titles() {
		assert!(parse_title_line("TITLE: Auth\nbody").is_none());
		assert!(parse_title_line("TITLE: Bug fix\nbody").is_none());
		assert!(parse_title_line("TITLE: Short name only here\nbody").is_none()); // < 6 words
	}

	#[test]
	fn hide_title_lines_from_display_including_incomplete_stream() {
		assert!(should_hide_title_display_line(
			"TITLE: Diagnose and fix the login timeout that blocks users after password reset",
			false
		));
		assert!(should_hide_title_display_line("TITLE:", true));
		assert!(should_hide_title_display_line("TITLE", true));
		assert!(should_hide_title_display_line("TITL", true));
		assert!(!should_hide_title_display_line("Here is the answer", true));
		let cleaned = sanitize_assistant_display_text(
			"TITLE: Diagnose and fix the login timeout that blocks users after password reset\n\nHello **world**",
			true,
		);
		assert!(!cleaned.contains("TITLE"));
		assert!(cleaned.contains("Hello"));
	}

	#[test]
	fn extract_open_todos() {
		let t = "- [ ] one\n- [x] two\n- [ ] three";
		let todos = extract_todos(t);
		assert_eq!(todos.len(), 3);
		assert_eq!(todos[0].content, "one");
	}

	#[test]
	fn apply_meta_sets_title() {
		let mut name = "Session abc".into();
		let mut from_ai = false;
		let (t, todos) = apply_first_turn_meta(
			"TITLE: Hello World from the first assistant turn spanning the header\n- [ ] a\n- [ ] b\nDone.",
			&mut name,
			&mut from_ai,
		);
		assert_eq!(t.as_deref(), Some("Hello World from the first assistant turn spanning the header"));
		assert_eq!(name, "Hello World from the first assistant turn spanning the header");
		assert!(from_ai);
		assert_eq!(todos.len(), 2);
	}

	#[test]
	fn heuristic_title_from_user() {
		let t = heuristic_session_title("fix the login bug please\nmore");
		// Topic-style multi-word name (fillers dropped / framed)
		assert!(t.split_whitespace().count() >= 4, "got {t}");
		assert!(
			t.to_ascii_lowercase().contains("login") || t.to_ascii_lowercase().contains("fix"),
			"got {t}"
		);
	}

	#[test]
	fn compact_title_is_long_topic_not_raw_prompt() {
		let t = compact_session_title("can you please help me with the authentication flow?");
		assert!(t.chars().count() > 40, "got {t}");
		assert!(t.split_whitespace().count() >= 6, "got {t}");
		// Should not start with chatty filler
		assert!(!t.to_ascii_lowercase().starts_with("can you"), "got {t}");
	}

	#[test]
	fn long_titles_are_retained_for_wrapped_sidebar_headers() {
		let title =
			"TITLE: Diagnose the Windows build failure and repair the streaming chat response renderer";
		assert_eq!(
			parse_title_line(title).as_deref(),
			Some("Diagnose the Windows build failure and repair the streaming chat response renderer")
		);

		let fallback = compact_session_title(
			"diagnose the Windows build failure and repair the streaming chat response renderer",
		);
		assert!(fallback.contains("streaming chat response renderer"), "got {fallback}");
	}

	#[test]
	fn fit_title_fills_width_single_line() {
		let s = fit_title_for_width("Fix authentication timeout in middleware", 18);
		assert!(s.ends_with('…'), "got {s}");
		// Character-based: should keep more than one word when width allows.
		assert!(s.starts_with("Fix authentication") || s.starts_with("Fix a"), "got {s}");
		assert!(!s.is_empty());
		let wide = fit_title_for_width("Ship the multi agent sidebar title correctly now", 40);
		assert!(
			wide.contains("multi") && wide.contains("sidebar"),
			"long title must keep interior words on one line, got {wide}"
		);
	}

	#[test]
	fn ai_title_replaces_user_prompt_heuristic() {
		let mut name = "Wow, so can you list how many files…".into();
		let mut from_ai = false;
		let (t, _) = apply_first_turn_meta(
			"TITLE: Review git status and summarize the twelve modified files in the workspace\n\nYou have 12 modified files.",
			&mut name,
			&mut from_ai,
		);
		assert_eq!(
			t.as_deref(),
			Some("Review git status and summarize the twelve modified files in the workspace")
		);
		assert_eq!(name, "Review git status and summarize the twelve modified files in the workspace");
		assert!(from_ai);
	}

	#[test]
	fn ai_title_wins_over_prior_heuristic_flag() {
		// Even if we already set a heuristic name, AI TITLE should replace it once.
		let mut name = "List all files in the repo".into();
		let mut from_ai = false; // heuristic without from_ai flag
		let (t, _) = apply_first_turn_meta(
			"<think>\nplan\n</think>\nTITLE: Build a complete inventory of repository files for the project overview\n\nHere you go.",
			&mut name,
			&mut from_ai,
		);
		assert_eq!(
			t.as_deref(),
			Some("Build a complete inventory of repository files for the project overview")
		);
		assert_eq!(name, "Build a complete inventory of repository files for the project overview");
	}
}
