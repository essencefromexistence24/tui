//! Diff review: multi-file unified-diff accept / reverse / open.

use std::path::{Path, PathBuf};

/// One file section inside a multi-file unified diff.
#[derive(Debug, Clone)]
pub struct DiffFile {
	pub old_path: Option<PathBuf>,
	pub new_path: Option<PathBuf>,
	pub hunks: Vec<DiffHunk>,
}

#[derive(Debug, Clone)]
pub struct DiffHunk {
	/// Lines as in the patch: prefix + content (prefix is ' ', '+', '-', '\\')
	pub lines: Vec<String>,
}

/// Extract primary file path from a unified-diff body or tool preview.
pub fn extract_diff_path(body: &str, preview: &str) -> Option<PathBuf> {
	parse_diff_files(body).into_iter().find_map(|f| f.new_path.or(f.old_path)).or_else(|| {
		let p = preview.trim();
		if !p.is_empty() && (p.contains('/') || p.contains('\\') || p.contains('.')) {
			let token = p.split_whitespace().last().unwrap_or(p);
			if token.len() > 1 {
				return Some(PathBuf::from(token));
			}
		}
		None
	})
}

/// Parse multi-file unified diffs into structured files+hunks.
pub fn parse_diff_files(body: &str) -> Vec<DiffFile> {
	let mut files = Vec::new();
	let mut cur: Option<DiffFile> = None;
	let mut cur_hunk: Option<DiffHunk> = None;

	let flush_hunk = |cur: &mut Option<DiffFile>, hunk: &mut Option<DiffHunk>| {
		if let (Some(f), Some(h)) = (cur.as_mut(), hunk.take())
			&& !h.lines.is_empty()
		{
			f.hunks.push(h);
		}
	};
	let flush_file =
		|files: &mut Vec<DiffFile>, cur: &mut Option<DiffFile>, hunk: &mut Option<DiffHunk>| {
			flush_hunk(cur, hunk);
			if let Some(f) = cur.take()
				&& (f.new_path.is_some() || f.old_path.is_some() || !f.hunks.is_empty())
			{
				files.push(f);
			}
		};

	for line in body.lines() {
		if line.starts_with("diff --git ") {
			flush_file(&mut files, &mut cur, &mut cur_hunk);
			cur = Some(DiffFile { old_path: None, new_path: None, hunks: Vec::new() });
			// diff --git a/foo b/foo
			let parts: Vec<&str> = line.split_whitespace().collect();
			if parts.len() >= 4
				&& let Some(f) = cur.as_mut()
			{
				let a = parts[2].trim_start_matches("a/");
				let b = parts[3].trim_start_matches("b/");
				if a != "/dev/null" {
					f.old_path = Some(PathBuf::from(a));
				}
				if b != "/dev/null" {
					f.new_path = Some(PathBuf::from(b));
				}
			}
			continue;
		}
		if let Some(rest) = line.strip_prefix("--- ") {
			if cur.is_none() {
				cur = Some(DiffFile { old_path: None, new_path: None, hunks: Vec::new() });
			}
			let p = rest.trim();
			let p = p.strip_prefix("a/").unwrap_or(p);
			if p != "/dev/null"
				&& let Some(f) = cur.as_mut()
			{
				f.old_path = Some(PathBuf::from(p.split('\t').next().unwrap_or(p)));
			}
			continue;
		}
		if let Some(rest) = line.strip_prefix("+++ ") {
			if cur.is_none() {
				cur = Some(DiffFile { old_path: None, new_path: None, hunks: Vec::new() });
			}
			let p = rest.trim();
			let p = p.strip_prefix("b/").unwrap_or(p);
			if p != "/dev/null"
				&& let Some(f) = cur.as_mut()
			{
				f.new_path = Some(PathBuf::from(p.split('\t').next().unwrap_or(p)));
			}
			continue;
		}
		if line.starts_with("@@") {
			flush_hunk(&mut cur, &mut cur_hunk);
			cur_hunk = Some(DiffHunk { lines: Vec::new() });
			continue;
		}
		if (line.starts_with('+')
			|| line.starts_with('-')
			|| line.starts_with(' ')
			|| line.starts_with('\\'))
			&& let Some(h) = cur_hunk.as_mut()
		{
			h.lines.push(line.to_string());
		}
	}
	flush_file(&mut files, &mut cur, &mut cur_hunk);
	files
}

/// Reverse-apply all files in a unified diff onto disk.
/// Returns (files_touched, errors).
pub fn reject_unified_diff(cwd: &Path, body: &str) -> anyhow::Result<bool> {
	let files = parse_diff_files(body);
	if files.is_empty() {
		// legacy single-pass reconstruction
		return reject_single_file_legacy(cwd, body);
	}
	let mut any = false;
	let mut last_err: Option<anyhow::Error> = None;
	for f in &files {
		match reverse_apply_file(cwd, f) {
			Ok(true) => any = true,
			Ok(false) => {}
			Err(e) => last_err = Some(e),
		}
	}
	if any || try_git_checkout(cwd, &files) {
		Ok(true)
	} else if let Some(e) = last_err {
		Err(e)
	} else {
		Ok(false)
	}
}

/// Last-resort: `git checkout -- <paths>` for messy/binary reverts.
fn try_git_checkout(cwd: &Path, files: &[DiffFile]) -> bool {
	let paths: Vec<PathBuf> =
		files.iter().filter_map(|f| f.old_path.clone().or_else(|| f.new_path.clone())).collect();
	if paths.is_empty() {
		return false;
	}
	let mut cmd = std::process::Command::new("git");
	cmd.arg("checkout").arg("--").current_dir(cwd);
	for p in &paths {
		cmd.arg(p);
	}
	cmd
		.stdout(std::process::Stdio::null())
		.stderr(std::process::Stdio::null())
		.status()
		.map(|s| s.success())
		.unwrap_or(false)
}

fn reverse_apply_file(cwd: &Path, file: &DiffFile) -> anyhow::Result<bool> {
	// Prefer old path for restore; if deleted (new is /dev/null conceptually), write old_path
	let path = file
		.old_path
		.clone()
		.or_else(|| file.new_path.clone())
		.ok_or_else(|| anyhow::anyhow!("no path"))?;
	let full = if path.is_absolute() { path.clone() } else { cwd.join(&path) };

	// If we have hunks, reverse-apply onto current file content when possible
	if full.exists()
		&& !file.hunks.is_empty()
		&& let Ok(current) = std::fs::read_to_string(&full)
		&& let Some(restored) = reverse_apply_hunks_to_text(&current, &file.hunks)
	{
		std::fs::write(&full, restored)?;
		return Ok(true);
	}

	// Reconstruct pure "old" from hunks (works for full-file style patches)
	let mut old_lines: Vec<String> = Vec::new();
	for h in &file.hunks {
		for line in &h.lines {
			if let Some(rest) = line.strip_prefix('-') {
				old_lines.push(rest.to_string());
			} else if let Some(rest) = line.strip_prefix(' ') {
				old_lines.push(rest.to_string());
			}
			// skip '+'
		}
	}
	if old_lines.is_empty() {
		// New file only (all +): reject = delete new_path
		if let Some(ref np) = file.new_path {
			let full_new = if np.is_absolute() { np.clone() } else { cwd.join(np) };
			if full_new.exists() {
				std::fs::remove_file(&full_new)?;
				return Ok(true);
			}
		}
		return Ok(false);
	}
	let content = old_lines.join("\n") + "\n";
	if let Some(parent) = full.parent() {
		std::fs::create_dir_all(parent)?;
	}
	std::fs::write(&full, content)?;
	Ok(true)
}

/// Reverse-apply hunks onto existing text by matching context and '-' lines.
fn reverse_apply_hunks_to_text(current: &str, hunks: &[DiffHunk]) -> Option<String> {
	let mut lines: Vec<String> = current.lines().map(|s| s.to_string()).collect();
	// Apply hunks bottom-up so indices stay stable
	for hunk in hunks.iter().rev() {
		// Build expected "new" side sequence and "old" replacement
		let mut new_side = Vec::new();
		let mut old_side = Vec::new();
		for line in &hunk.lines {
			if let Some(rest) = line.strip_prefix('+') {
				new_side.push(rest.to_string());
				// not in old
			} else if let Some(rest) = line.strip_prefix('-') {
				old_side.push(rest.to_string());
				// not in new
			} else if let Some(rest) = line.strip_prefix(' ') {
				new_side.push(rest.to_string());
				old_side.push(rest.to_string());
			}
		}
		if new_side.is_empty() && old_side.is_empty() {
			continue;
		}
		// Find new_side as contiguous slice in lines
		if let Some(pos) = find_slice(&lines, &new_side) {
			lines.splice(pos..pos + new_side.len(), old_side);
		} else {
			// cannot safely apply this hunk
			return None;
		}
	}
	let mut out = lines.join("\n");
	if current.ends_with('\n') {
		out.push('\n');
	}
	Some(out)
}

fn find_slice(hay: &[String], needle: &[String]) -> Option<usize> {
	if needle.is_empty() {
		return Some(0);
	}
	if needle.len() > hay.len() {
		return None;
	}
	for i in 0..=hay.len() - needle.len() {
		if hay[i..i + needle.len()] == *needle {
			return Some(i);
		}
	}
	None
}

fn reject_single_file_legacy(cwd: &Path, body: &str) -> anyhow::Result<bool> {
	let path = extract_diff_path(body, "").ok_or_else(|| anyhow::anyhow!("no path in diff"))?;
	let full = if path.is_absolute() { path } else { cwd.join(path) };
	let mut old_lines: Vec<String> = Vec::new();
	let mut in_hunk = false;
	for line in body.lines() {
		if line.starts_with("@@") {
			in_hunk = true;
			continue;
		}
		if !in_hunk {
			continue;
		}
		if line.starts_with("diff ") || line.starts_with("--- ") || line.starts_with("+++ ") {
			in_hunk = false;
			continue;
		}
		if let Some(rest) = line.strip_prefix('-') {
			if !line.starts_with("---") {
				old_lines.push(rest.to_string());
			}
		} else if let Some(rest) = line.strip_prefix(' ') {
			old_lines.push(rest.to_string());
		}
	}
	if old_lines.is_empty() {
		return Ok(false);
	}
	if let Some(parent) = full.parent() {
		std::fs::create_dir_all(parent)?;
	}
	std::fs::write(&full, old_lines.join("\n") + "\n")?;
	Ok(true)
}

pub fn accept_diff_path(body: &str, preview: &str) -> Option<PathBuf> {
	extract_diff_path(body, preview)
}

/// All paths touched by a multi-file diff.
#[allow(dead_code)]
pub fn extract_all_diff_paths(body: &str) -> Vec<PathBuf> {
	parse_diff_files(body).into_iter().filter_map(|f| f.new_path.or(f.old_path)).collect()
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn extracts_plus_plus_path() {
		let body = "--- a/src/foo.rs\n+++ b/src/foo.rs\n@@ -1 +1 @@\n-old\n+new\n";
		let p = extract_diff_path(body, "").unwrap();
		assert!(p.ends_with("foo.rs"));
	}

	#[test]
	fn reverse_apply_simple() {
		let text = "a\nb\nc\n";
		let hunk = DiffHunk { lines: vec![" a".into(), "-b".into(), "+B".into(), " c".into()] };
		// new side is a B c
		let current = "a\nB\nc\n";
		let restored = reverse_apply_hunks_to_text(current, &[hunk]).unwrap();
		assert!(restored.contains('b') || restored.contains("a\nb\nc"));
	}

	#[test]
	fn multi_file_parse() {
		let body = "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1 +1 @@\n-old\n+new\ndiff --git a/b.rs b/b.rs\n--- a/b.rs\n+++ b/b.rs\n@@ -1 +1 @@\n-x\n+y\n";
		let files = parse_diff_files(body);
		assert_eq!(files.len(), 2);
	}
}
