//! RTK-inspired token saving for tool / shell / agent context (OmniRoute techniques).
//!
//! Applies strip-ANSI, line dedupe, head/tail truncation, and category heuristics
//! so the agent sends less noise while keeping signal.

use std::collections::HashSet;

/// Result of compressing a text blob for the model context.
#[derive(Debug, Clone)]
pub struct CompressResult {
	pub text: String,
	pub original_chars: usize,
	pub compressed_chars: usize,
	pub techniques: Vec<&'static str>,
}

impl CompressResult {
	pub fn saved_ratio(&self) -> f32 {
		if self.original_chars == 0 {
			return 0.0;
		}
		1.0 - (self.compressed_chars as f32 / self.original_chars as f32)
	}

	/// One-line status for `/status` / sidebar.
	pub fn report_line(&self) -> String {
		let tech = techniques_summary(self.techniques.as_slice());
		format!(
			"{}→{} chars (~{:.0}%) · {tech}",
			self.original_chars,
			self.compressed_chars,
			self.saved_ratio() * 100.0
		)
	}
}

/// Compress command / tool output before it enters the model context.
pub fn compress_tool_output(input: &str) -> CompressResult {
	compress_tool_output_categorized(input, detect_category(input))
}

/// Category-aware compression (RTK-style filter catalog).
pub fn compress_tool_output_categorized(input: &str, category: OutputCategory) -> CompressResult {
	let original_chars = input.chars().count();
	let mut techniques = Vec::new();
	let mut text = strip_ansi(input);
	if text.len() != input.len() {
		techniques.push("strip_ansi");
	}

	text = filter_stderr_prefixes(&text);
	text = apply_category_filters(&text, category, &mut techniques);
	// Project `.rtk/filters.json` drop substrings (trust: local file only).
	let extra = load_project_rtk_extra_noise(None);
	if !extra.is_empty() {
		let before = text.len();
		text = text
			.lines()
			.filter(|line| {
				let lower = line.to_ascii_lowercase();
				!extra.iter().any(|n| lower.contains(&n.to_ascii_lowercase()))
			})
			.collect::<Vec<_>>()
			.join("\n");
		if text.len() != before {
			techniques.push("rtk_project");
		}
	}
	text = drop_noise_lines(&text, &mut techniques);
	text = dedupe_consecutive_lines(&text, 3, &mut techniques);

	let (max_lines, max_line_chars) = category.truncate_budget();
	text = smart_truncate(&text, max_lines, max_line_chars, &mut techniques);

	// Optional second-stage: caveman-style prose condense for huge assistant blobs
	if category == OutputCategory::Prose && text.chars().count() > 2_000 {
		text = caveman_condense(&text, &mut techniques);
	}

	let compressed_chars = text.chars().count();
	CompressResult { text, original_chars, compressed_chars, techniques }
}

/// Output categories mirroring OmniRoute / RTK filter sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputCategory {
	Generic,
	Git,
	Test,
	Build,
	Package,
	Shell,
	Docker,
	Prose,
}

impl OutputCategory {
	fn truncate_budget(self) -> (usize, usize) {
		match self {
			Self::Git => (80, 120),
			Self::Test => (100, 100),
			Self::Build => (90, 100),
			Self::Package => (60, 100),
			Self::Shell => (100, 120),
			Self::Docker => (80, 100),
			Self::Prose => (200, 200),
			Self::Generic => (120, 80),
		}
	}
}

/// Heuristic category detection from content.
pub fn detect_category(input: &str) -> OutputCategory {
	let head: String = input.chars().take(800).collect::<String>().to_ascii_lowercase();
	if head.contains("git ") || head.contains("diff --git") || head.contains("@@ ") {
		return OutputCategory::Git;
	}
	if head.contains("passed") && head.contains("failed")
		|| head.contains("test result")
		|| head.contains("running ") && head.contains("test")
		|| head.contains("jest")
		|| head.contains("pytest")
	{
		return OutputCategory::Test;
	}
	if head.contains("compiling ")
		|| head.contains("cargo ")
		|| head.contains("webpack")
		|| head.contains("tsc ")
		|| head.contains("error[e")
	{
		return OutputCategory::Build;
	}
	if head.contains("npm ")
		|| head.contains("pnpm ")
		|| head.contains("yarn ")
		|| head.contains("pip install")
		|| head.contains("cargo install")
	{
		return OutputCategory::Package;
	}
	if head.contains("docker ") || head.contains("container ") || head.contains("image id") {
		return OutputCategory::Docker;
	}
	if head.starts_with("$ ") || head.contains("\n$ ") || head.contains("exit code") {
		return OutputCategory::Shell;
	}
	// Long paragraphs without path-like tokens → prose
	let lines = input.lines().count();
	let avg = input.len().checked_div(lines).unwrap_or(0);
	if avg > 80 && lines < 40 {
		return OutputCategory::Prose;
	}
	OutputCategory::Generic
}

fn apply_category_filters(
	s: &str,
	category: OutputCategory,
	techniques: &mut Vec<&'static str>,
) -> String {
	let mut dropped = 0usize;
	let kept: Vec<&str> = s
		.lines()
		.filter(|line| {
			let lower = line.to_ascii_lowercase();
			let drop = match category {
				OutputCategory::Git => {
					lower.starts_with("index ")
						|| lower.starts_with("similarity index")
						|| lower.contains("create mode ")
						|| lower.contains("delete mode ")
				}
				OutputCategory::Test => {
					lower.contains("console.log")
						|| lower.contains("slow test")
						|| lower.contains("coverage provider")
						|| (lower.contains("✓") && lower.contains("ms"))
				}
				OutputCategory::Build => {
					lower.contains("downloading")
						|| lower.contains("fresh crates.io")
						|| lower.starts_with("   ")
							&& (lower.contains("compiling") || lower.contains("checking"))
				}
				OutputCategory::Package => {
					lower.contains("npm warn")
						|| lower.contains("added ") && lower.contains("packages")
						|| lower.contains("progress:")
						|| lower.contains("http fetch")
				}
				OutputCategory::Docker => {
					lower.contains("sha256:") && lower.len() > 40
						|| lower.starts_with(" --->")
						|| lower.contains("pull complete")
				}
				OutputCategory::Shell => {
					lower.starts_with("total ") && lower.contains("drwx") || lower == "ls:"
				}
				OutputCategory::Prose | OutputCategory::Generic => false,
			};
			if drop {
				dropped += 1;
				false
			} else {
				true
			}
		})
		.collect();
	if dropped > 0 {
		techniques.push(match category {
			OutputCategory::Git => "rtk_git",
			OutputCategory::Test => "rtk_test",
			OutputCategory::Build => "rtk_build",
			OutputCategory::Package => "rtk_package",
			OutputCategory::Shell => "rtk_shell",
			OutputCategory::Docker => "rtk_docker",
			_ => "rtk_generic",
		});
	}
	kept.join("\n")
}

/// Caveman-style: keep first + last sentences of long paragraphs; drop filler.
fn caveman_condense(s: &str, techniques: &mut Vec<&'static str>) -> String {
	let mut out = Vec::new();
	for para in s.split("\n\n") {
		let words: Vec<&str> = para.split_whitespace().collect();
		if words.len() <= 60 {
			out.push(para.to_string());
			continue;
		}
		let head = words.iter().take(28).copied().collect::<Vec<_>>().join(" ");
		let tail = words.iter().rev().take(20).copied().collect::<Vec<_>>();
		let tail: Vec<_> = tail.into_iter().rev().collect();
		out.push(format!("{head} … {}", tail.join(" ")));
	}
	techniques.push("caveman");
	out.join("\n\n")
}

/// Load optional project `.rtk/filters.json` (trust-gated: only if file exists under cwd).
pub fn load_project_rtk_extra_noise(cwd: Option<&std::path::Path>) -> Vec<String> {
	let base = cwd
		.map(|p| p.to_path_buf())
		.or_else(|| std::env::current_dir().ok())
		.unwrap_or_else(|| std::path::PathBuf::from("."));
	let path = base.join(".rtk").join("filters.json");
	let Ok(text) = std::fs::read_to_string(path) else {
		return Vec::new();
	};
	// Expect `{"drop_substrings":["..."]}` — ignore malformed.
	#[derive(serde::Deserialize)]
	struct Filters {
		#[serde(default)]
		drop_substrings: Vec<String>,
	}
	serde_json::from_str::<Filters>(&text).map(|f| f.drop_substrings).unwrap_or_default()
}

/// Telemetry string for `/status` and sidebar.
pub fn telemetry_line(last_report: &str, enabled: bool) -> String {
	if !enabled {
		return "token-save: off".into();
	}
	if last_report.is_empty() {
		"token-save: on · waiting for first compress".into()
	} else {
		format!("token-save: on · {last_report}")
	}
}

/// Compress an entire multi-turn history for token budget (message-level).
pub fn compress_history_messages(
	messages: &[(String, String)],
	max_chars: usize,
) -> Vec<(String, String)> {
	if messages.is_empty() {
		return Vec::new();
	}
	// Always keep first user + last N messages; compress middles.
	let keep_tail = 6.min(messages.len());
	let head = 1.min(messages.len());
	if messages.len() <= keep_tail + head {
		return messages.iter().map(|(r, c)| (r.clone(), compress_tool_output(c).text)).collect();
	}

	let mut out = Vec::new();
	// Head
	for (r, c) in messages.iter().take(head) {
		out.push((r.clone(), compress_tool_output(c).text));
	}
	// Summary stub for middle
	let middle = &messages[head..messages.len() - keep_tail];
	let mid_chars: usize = middle.iter().map(|(_, c)| c.len()).sum();
	out.push((
		"assistant".into(),
		format!(
			"[context compacted: {} earlier turns ~{mid_chars} chars omitted; details available on request]",
			middle.len()
		),
	));
	// Tail
	for (r, c) in messages.iter().skip(messages.len() - keep_tail) {
		out.push((r.clone(), compress_tool_output(c).text));
	}

	// If still over budget, hard-trim oldest compressed bodies.
	let total: usize = out.iter().map(|(_, c)| c.len()).sum();
	if total > max_chars && out.len() > 2 {
		let mut budget = max_chars;
		let mut trimmed = Vec::new();
		// Keep from the end
		for (r, c) in out.into_iter().rev() {
			if budget == 0 {
				break;
			}
			if c.len() <= budget {
				budget = budget.saturating_sub(c.len());
				trimmed.push((r, c));
			} else {
				let take = budget.min(c.len());
				let short: String = c.chars().skip(c.chars().count().saturating_sub(take)).collect();
				trimmed.push((r, format!("…{short}")));
				budget = 0;
			}
		}
		trimmed.reverse();
		return trimmed;
	}
	out
}

fn strip_ansi(s: &str) -> String {
	// Minimal CSI stripper: \x1b[ ... m / letter
	let bytes = s.as_bytes();
	let mut out = String::with_capacity(s.len());
	let mut i = 0;
	while i < bytes.len() {
		if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
			i += 2;
			while i < bytes.len() {
				let b = bytes[i];
				i += 1;
				if (0x40..=0x7e).contains(&b) {
					break;
				}
			}
			continue;
		}
		out.push(bytes[i] as char);
		i += 1;
	}
	out
}

fn filter_stderr_prefixes(s: &str) -> String {
	s.lines()
		.map(|l| {
			let t = l.trim_start();
			t.strip_prefix("stderr: ").or_else(|| t.strip_prefix("STDERR: ")).unwrap_or(l)
		})
		.collect::<Vec<_>>()
		.join("\n")
}

fn drop_noise_lines(s: &str, techniques: &mut Vec<&'static str>) -> String {
	let noise = [
		"npm warn",
		"npm notice",
		"deprecated",
		"download progress",
		"loading ",
		"compiling ",
		"checking ",
		"warning: unused",
	];
	let mut kept = Vec::new();
	let mut dropped = 0usize;
	for line in s.lines() {
		let lower = line.to_ascii_lowercase();
		if noise.iter().any(|n| lower.contains(n)) {
			dropped += 1;
			continue;
		}
		kept.push(line);
	}
	if dropped > 0 {
		techniques.push("drop_noise");
	}
	kept.join("\n")
}

fn dedupe_consecutive_lines(
	s: &str,
	threshold: usize,
	techniques: &mut Vec<&'static str>,
) -> String {
	let mut out = Vec::new();
	let mut prev: Option<&str> = None;
	let mut run = 0usize;
	let mut collapsed = false;
	for line in s.lines() {
		if Some(line) == prev {
			run += 1;
			if run < threshold {
				out.push(line.to_string());
			} else if run == threshold {
				out.push(format!("… (+{} identical lines)", threshold - 1));
				collapsed = true;
			}
		} else {
			prev = Some(line);
			run = 1;
			out.push(line.to_string());
		}
	}
	if collapsed {
		techniques.push("dedupe_lines");
	}
	out.join("\n")
}

fn smart_truncate(
	s: &str,
	max_lines: usize,
	max_line_chars: usize,
	techniques: &mut Vec<&'static str>,
) -> String {
	let lines: Vec<&str> = s.lines().collect();
	let mut truncated_lines = false;
	let mut truncated_body = false;

	let mut process = |line: &str| -> String {
		let count = line.chars().count();
		if count > max_line_chars {
			truncated_lines = true;
			let head: String = line.chars().take(max_line_chars.saturating_sub(1)).collect();
			format!("{head}…")
		} else {
			line.to_string()
		}
	};

	let result = if lines.len() <= max_lines {
		lines.into_iter().map(process).collect::<Vec<_>>().join("\n")
	} else {
		truncated_body = true;
		let head_n = max_lines / 2;
		let tail_n = max_lines - head_n;
		let mut parts: Vec<String> = lines.iter().take(head_n).map(|l| process(l)).collect();
		parts.push(format!("… [{} lines omitted] …", lines.len() - head_n - tail_n));
		parts.extend(lines.iter().rev().take(tail_n).rev().map(|l| process(l)));
		parts.join("\n")
	};

	if truncated_lines {
		techniques.push("truncate_line");
	}
	if truncated_body {
		techniques.push("head_tail");
	}
	result
}

/// Token estimate using cl100k_base tokenizer. Falls back to char-count/4 on error.
pub fn estimate_tokens(text: &str) -> usize {
	match tiktoken_rs::cl100k_base() {
		Ok(bpe) => bpe.encode_with_special_tokens(text).len(),
		Err(_) => text.chars().count().div_ceil(4),
	}
}

/// Estimate total tokens for a history.
pub fn estimate_history_tokens(messages: &[(String, String)]) -> usize {
	messages.iter().map(|(_, c)| estimate_tokens(c)).sum()
}

/// Unique technique labels for UI.
pub fn techniques_summary(techniques: &[&str]) -> String {
	let set: HashSet<_> = techniques.iter().copied().collect();
	let mut v: Vec<_> = set.into_iter().collect();
	v.sort_unstable();
	if v.is_empty() { "—".into() } else { v.join(", ") }
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn strip_ansi_removes_csi() {
		let s = "\x1b[31mred\x1b[0m ok";
		assert_eq!(strip_ansi(s), "red ok");
	}

	#[test]
	fn compress_saves_noise() {
		let mut noise = String::new();
		for _ in 0..50 {
			noise.push_str("npm warn deprecated foo@1.0.0\n");
			noise.push_str("actual error here\n");
		}
		let r = compress_tool_output(&noise);
		assert!(r.compressed_chars < r.original_chars);
		assert!(r.text.contains("actual error"));
	}

	#[test]
	fn history_keeps_tail() {
		let msgs: Vec<_> =
			(0..20).map(|i| ("user".into(), format!("message {i} {}", "x".repeat(100)))).collect();
		let out = compress_history_messages(&msgs, 5000);
		assert!(out.len() < msgs.len());
		assert!(out.last().unwrap().1.contains("message 19"));
	}

	#[test]
	fn detects_git_category() {
		let sample = "diff --git a/foo.rs b/foo.rs\nindex abc..def\n@@ -1 +1 @@\n-old\n+new\n";
		assert_eq!(detect_category(sample), OutputCategory::Git);
		let r = compress_tool_output(sample);
		assert!(r.techniques.iter().any(|t| t.starts_with("rtk_")
			|| *t == "strip_ansi"
			|| *t == "head_tail"
			|| *t == "drop_noise"
			|| t.starts_with("rtk")));
	}

	#[test]
	fn caveman_shortens_prose() {
		let long = "word ".repeat(200);
		let mut tech = Vec::new();
		let out = caveman_condense(&long, &mut tech);
		assert!(out.len() < long.len());
		assert!(tech.contains(&"caveman"));
	}
}
