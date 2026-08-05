//! Hermes-style background update check for DX TUI.

#![allow(dead_code)]
//!
//! Cached under `~/.config/dx/.update_check` for 6 hours.
//! Compares git commits behind `origin/main` when a checkout exists,
//! otherwise compares package version to GitHub latest release tag when possible.

use std::{
	fs,
	path::PathBuf,
	process::Command,
	time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

/// Workspace package version (from Cargo).
pub const DX_VERSION: &str = env!("CARGO_PKG_VERSION");

const CACHE_SECS: u64 = 6 * 3600;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Cache {
	ts: u64,
	ver: String,
	/// Commits behind, or -1 if behind but unknown, 0 up-to-date.
	behind: Option<i64>,
	/// Optional latest remote version string.
	latest: Option<String>,
}

#[derive(Debug, Clone)]
pub struct UpdateStatus {
	/// `None` = check N/A or failed silently.
	pub commits_behind: Option<i64>,
	pub current: String,
	pub latest: Option<String>,
	pub message: String,
}

fn config_dir() -> PathBuf {
	dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("dx")
}

fn cache_path() -> PathBuf {
	config_dir().join(".update_check")
}

fn now_secs() -> u64 {
	SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Non-blocking-friendly check (sync; call from spawn_blocking if needed).
pub fn check_for_updates() -> UpdateStatus {
	if std::env::var("DX_NO_AUTO_UPDATE").ok().as_deref() == Some("1") {
		return UpdateStatus {
			commits_behind: None,
			current: DX_VERSION.into(),
			latest: None,
			message: "auto-update disabled (DX_NO_AUTO_UPDATE=1)".into(),
		};
	}

	// Cache
	if let Ok(raw) = fs::read_to_string(cache_path())
		&& let Ok(c) = serde_json::from_str::<Cache>(&raw)
		&& c.ver == DX_VERSION
		&& now_secs().saturating_sub(c.ts) < CACHE_SECS
	{
		return status_from_cache(&c);
	}

	let mut behind: Option<i64> = None;
	let mut latest: Option<String> = None;

	// Prefer git checkout of this source tree
	if let Some(repo) = find_git_repo()
		&& let Some(n) = git_commits_behind(&repo)
	{
		behind = Some(n);
	}

	// Optional: GitHub latest release for essence-dx/cli
	if behind.is_none()
		&& let Some(tag) = github_latest_tag()
	{
		latest = Some(tag.clone());
		behind = Some(if version_newer(&tag, DX_VERSION) { -1 } else { 0 });
	}

	let cache = Cache { ts: now_secs(), ver: DX_VERSION.into(), behind, latest: latest.clone() };
	let _ = fs::create_dir_all(config_dir());
	if let Ok(j) = serde_json::to_string(&cache) {
		let _ = fs::write(cache_path(), j);
	}

	status_from_cache(&cache)
}

fn status_from_cache(c: &Cache) -> UpdateStatus {
	let message = match c.behind {
		None => "update check unavailable".into(),
		Some(0) => format!("dx {DX_VERSION} up to date"),
		Some(-1) => format!(
			"update available · current {DX_VERSION}{}",
			c.latest.as_ref().map(|l| format!(" · latest {l}")).unwrap_or_default()
		),
		Some(n) if n > 0 => format!("{n} commits behind origin/main · dx {DX_VERSION}"),
		Some(n) => format!("update status {n}"),
	};
	UpdateStatus {
		commits_behind: c.behind,
		current: c.ver.clone(),
		latest: c.latest.clone(),
		message,
	}
}

fn find_git_repo() -> Option<PathBuf> {
	// Walk up from CARGO_MANIFEST_DIR / cwd
	let start = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
	let mut dir = start;
	for _ in 0..8 {
		if dir.join(".git").exists() {
			return Some(dir);
		}
		if !dir.pop() {
			break;
		}
	}
	std::env::current_dir().ok().and_then(|mut d| {
		for _ in 0..8 {
			if d.join(".git").exists() {
				return Some(d);
			}
			if !d.pop() {
				break;
			}
		}
		None
	})
}

fn git_commits_behind(repo: &PathBuf) -> Option<i64> {
	// fetch is too slow/networky for default — only count vs existing origin/main
	let out = Command::new("git")
		.args(["rev-list", "--count", "HEAD..origin/main"])
		.current_dir(repo)
		.output()
		.ok()?;
	if !out.status.success() {
		// try master
		let out = Command::new("git")
			.args(["rev-list", "--count", "HEAD..origin/master"])
			.current_dir(repo)
			.output()
			.ok()?;
		if !out.status.success() {
			return None;
		}
		let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
		return s.parse().ok();
	}
	let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
	s.parse().ok()
}

fn github_latest_tag() -> Option<String> {
	// Lightweight: use git ls-remote if git available
	let out = Command::new("git")
		.args(["ls-remote", "--tags", "--refs", "https://github.com/essence-dx/cli.git"])
		.output()
		.ok()?;
	if !out.status.success() {
		return None;
	}
	let text = String::from_utf8_lossy(&out.stdout);
	let mut tags: Vec<String> = text
		.lines()
		.filter_map(|l| l.split_whitespace().nth(1))
		.filter_map(|r| r.strip_prefix("refs/tags/"))
		.map(|t| t.trim_start_matches('v').to_string())
		.collect();
	tags.sort_by(|a, b| cmp_version(a, b));
	tags.pop()
}

fn version_newer(remote: &str, local: &str) -> bool {
	cmp_version(remote, local) == std::cmp::Ordering::Greater
}

fn cmp_version(a: &str, b: &str) -> std::cmp::Ordering {
	let pa: Vec<u32> =
		a.split(|c: char| !c.is_ascii_digit()).filter_map(|s| s.parse().ok()).collect();
	let pb: Vec<u32> =
		b.split(|c: char| !c.is_ascii_digit()).filter_map(|s| s.parse().ok()).collect();
	for i in 0..pa.len().max(pb.len()) {
		let x = pa.get(i).copied().unwrap_or(0);
		let y = pb.get(i).copied().unwrap_or(0);
		match x.cmp(&y) {
			std::cmp::Ordering::Equal => {}
			o => return o,
		}
	}
	std::cmp::Ordering::Equal
}

/// Background prefetch (Hermes `prefetch_update_check`).
pub fn spawn_prefetch(tx: std::sync::mpsc::Sender<String>) {
	std::thread::spawn(move || {
		let st = check_for_updates();
		if st.commits_behind.is_some_and(|n| n != 0) {
			let _ = tx.send(format!("\n__UPDATE_STATUS__\n{}\n", st.message));
		}
	});
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn version_cmp() {
		assert_eq!(cmp_version("26.2.3", "26.2.2"), std::cmp::Ordering::Greater);
		assert_eq!(cmp_version("26.2.2", "26.2.2"), std::cmp::Ordering::Equal);
	}
}
