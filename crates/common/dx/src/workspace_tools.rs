//! Workspace tooling: formatter, linter, LSP diagnostics, VCS, subagent inventory.
//!
//! Prefer real project signals (cargo check JSON, git status, rust-analyzer on PATH)
//! over hard-coded drive scans. Tools are discovered via PATH + project markers.

use std::{path::Path, process::Command, time::Instant};

use crate::components::{Message, MessageBlock, parse_message_blocks};

// ── Shared ──────────────────────────────────────────────────────────────

/// Parallelism for every `cargo check|test|build|run|clippy` we spawn.
pub const CARGO_JOBS: &str = "12";

pub fn which_bin(name: &str) -> bool {
	std::env::var_os("PATH")
		.map(|paths| {
			std::env::split_paths(&paths).any(|dir| {
				let p = dir.join(name);
				p.is_file()
					|| p.with_extension("exe").is_file()
					|| dir.join(format!("{name}.exe")).is_file()
			})
		})
		.unwrap_or(false)
}

/// `cargo <subcmd> -j12 …` — always inject jobs before trailing args.
fn cargo_args(subcmd: &str, rest: &[&str]) -> Vec<String> {
	let mut v = vec![subcmd.to_string(), "-j".into(), CARGO_JOBS.into()];
	v.extend(rest.iter().map(|s| (*s).to_string()));
	v
}

/// Run a command and capture output (public for use by state.rs).
#[allow(dead_code)]
pub fn run_capture_direct(bin: &str, args: &[&str], cwd: &Path) -> ToolRunResult {
	run_capture(bin, args, cwd, "direct")
}

fn run_capture(bin: &str, args: &[&str], cwd: &Path, timeout_hint: &str) -> ToolRunResult {
	let started = Instant::now();
	let out = Command::new(bin).args(args).current_dir(cwd).output();
	let elapsed = started.elapsed();
	match out {
		Ok(o) => {
			let code = o.status.code().unwrap_or(-1);
			let mut stdout = String::from_utf8_lossy(&o.stdout).into_owned();
			let mut stderr = String::from_utf8_lossy(&o.stderr).into_owned();
			// Cap for UI / model context
			const CAP: usize = 8_000;
			if stdout.chars().count() > CAP {
				stdout = stdout.chars().take(CAP).collect::<String>() + "…";
			}
			if stderr.chars().count() > CAP {
				stderr = stderr.chars().take(CAP).collect::<String>() + "…";
			}
			let summary = if o.status.success() {
				format!("{bin} ok · {}ms", elapsed.as_millis())
			} else {
				let hint = first_useful_line(&stderr).or_else(|| first_useful_line(&stdout));
				format!(
					"{bin} exit {code} · {}ms · {}",
					elapsed.as_millis(),
					hint.unwrap_or_else(|| timeout_hint.into())
				)
			};
			ToolRunResult {
				tool: bin.to_string(),
				ok: o.status.success(),
				exit_code: code,
				stdout,
				stderr,
				summary,
			}
		}
		Err(e) => ToolRunResult {
			tool: bin.to_string(),
			ok: false,
			exit_code: -1,
			stdout: String::new(),
			stderr: e.to_string(),
			summary: format!("{bin}: spawn failed ({e})"),
		},
	}
}

fn first_useful_line(s: &str) -> Option<String> {
	s.lines()
		.map(str::trim)
		.find(|l| !l.is_empty() && !l.starts_with("warning:"))
		.map(|l| l.chars().take(100).collect())
}

#[derive(Debug, Clone)]
pub struct ToolRunResult {
	pub tool: String,
	pub ok: bool,
	pub exit_code: i32,
	pub stdout: String,
	pub stderr: String,
	pub summary: String,
}

impl ToolRunResult {
	pub fn body(&self) -> String {
		let mut b = String::new();
		if !self.stdout.trim().is_empty() {
			b.push_str(&self.stdout);
		}
		if !self.stderr.trim().is_empty() {
			if !b.is_empty() {
				b.push('\n');
			}
			b.push_str(&self.stderr);
		}
		if b.is_empty() {
			b = self.summary.clone();
		}
		b
	}

	pub fn fence(&self, kind: &str) -> String {
		format!("```command name=\"{kind}:{}\"\n{}\n```", self.tool, self.body())
	}
}

// ── Project detection ───────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectKind {
	Rust,
	Node,
	Python,
	Go,
	Mixed,
	Unknown,
}

pub fn detect_project(cwd: &Path) -> ProjectKind {
	let rust = cwd.join("Cargo.toml").is_file();
	let node = cwd.join("package.json").is_file();
	let py = cwd.join("pyproject.toml").is_file()
		|| cwd.join("requirements.txt").is_file()
		|| cwd.join("setup.py").is_file();
	let go = cwd.join("go.mod").is_file();
	match (rust, node, py, go) {
		(true, false, false, false) => ProjectKind::Rust,
		(false, true, false, false) => ProjectKind::Node,
		(false, false, true, false) => ProjectKind::Python,
		(false, false, false, true) => ProjectKind::Go,
		(false, false, false, false) => ProjectKind::Unknown,
		_ => ProjectKind::Mixed,
	}
}

// ── Formatter ───────────────────────────────────────────────────────────

/// Run the best available formatter in check mode (non-destructive).
pub fn run_formatter(cwd: &Path) -> ToolRunResult {
	let kind = detect_project(cwd);
	match kind {
		ProjectKind::Rust | ProjectKind::Mixed if which_bin("cargo") => {
			// Prefer check so we don't rewrite during plan (`cargo fmt` has no -j).
			let r = run_capture("cargo", &["fmt", "--", "--check"], cwd, "fmt issues");
			if r.ok || r.exit_code != -1 {
				return r;
			}
			run_capture("cargo", &["fmt", "--version"], cwd, "rustfmt missing")
		}
		ProjectKind::Node if which_bin("prettier") => {
			run_capture("prettier", &["--check", "."], cwd, "prettier issues")
		}
		ProjectKind::Node if which_bin("biome") => {
			run_capture("biome", &["check", "."], cwd, "biome issues")
		}
		ProjectKind::Python if which_bin("ruff") => {
			run_capture("ruff", &["format", "--check", "."], cwd, "ruff format")
		}
		ProjectKind::Python if which_bin("black") => {
			run_capture("black", &["--check", "."], cwd, "black issues")
		}
		ProjectKind::Go if which_bin("gofmt") => {
			// gofmt -l lists unformatted files
			run_capture("gofmt", &["-l", "."], cwd, "gofmt")
		}
		_ => {
			// Fallbacks in priority order
			if which_bin("cargo") {
				return run_capture("cargo", &["fmt", "--", "--check"], cwd, "fmt");
			}
			if which_bin("prettier") {
				return run_capture("prettier", &["--check", "."], cwd, "prettier");
			}
			if which_bin("biome") {
				return run_capture("biome", &["check", "."], cwd, "biome");
			}
			ToolRunResult {
				tool: "formatter".into(),
				ok: false,
				exit_code: -1,
				stdout: String::new(),
				stderr: "no formatter on PATH for this project".into(),
				summary: "formatter: none available".into(),
			}
		}
	}
}

/// Apply formatter (writes files) — only when user explicitly asks `/fmt apply`.
pub fn apply_formatter(cwd: &Path) -> ToolRunResult {
	let kind = detect_project(cwd);
	match kind {
		ProjectKind::Rust | ProjectKind::Mixed if which_bin("cargo") => {
			run_capture("cargo", &["fmt"], cwd, "fmt apply")
		}
		ProjectKind::Node if which_bin("prettier") => {
			run_capture("prettier", &["--write", "."], cwd, "prettier write")
		}
		ProjectKind::Python if which_bin("ruff") => {
			run_capture("ruff", &["format", "."], cwd, "ruff format apply")
		}
		ProjectKind::Go if which_bin("gofmt") => run_capture("gofmt", &["-w", "."], cwd, "gofmt write"),
		_ if which_bin("cargo") => run_capture("cargo", &["fmt"], cwd, "fmt"),
		_ => run_formatter(cwd),
	}
}

// ── Linter ──────────────────────────────────────────────────────────────

pub fn run_linter(cwd: &Path) -> ToolRunResult {
	let kind = detect_project(cwd);
	match kind {
		ProjectKind::Rust | ProjectKind::Mixed if which_bin("cargo") => {
			let args = cargo_args("clippy", &["-q", "--message-format=short", "--", "-W", "clippy::all"]);
			let args_ref: Vec<&str> = args.iter().map(String::as_str).collect();
			run_capture("cargo", &args_ref, cwd, "clippy warnings")
		}
		ProjectKind::Node if which_bin("eslint") => run_capture("eslint", &["."], cwd, "eslint issues"),
		ProjectKind::Node if which_bin("biome") => {
			run_capture("biome", &["lint", "."], cwd, "biome lint")
		}
		ProjectKind::Python if which_bin("ruff") => {
			run_capture("ruff", &["check", "."], cwd, "ruff check")
		}
		ProjectKind::Python if which_bin("pylint") => run_capture("pylint", &["."], cwd, "pylint"),
		ProjectKind::Go if which_bin("staticcheck") => {
			run_capture("staticcheck", &["./..."], cwd, "staticcheck")
		}
		ProjectKind::Go if which_bin("golint") => run_capture("golint", &["./..."], cwd, "golint"),
		_ => {
			if which_bin("cargo") {
				let args = cargo_args("clippy", &["-q", "--message-format=short"]);
				let args_ref: Vec<&str> = args.iter().map(String::as_str).collect();
				return run_capture("cargo", &args_ref, cwd, "clippy");
			}
			if which_bin("eslint") {
				return run_capture("eslint", &["."], cwd, "eslint");
			}
			if which_bin("ruff") {
				return run_capture("ruff", &["check", "."], cwd, "ruff");
			}
			ToolRunResult {
				tool: "linter".into(),
				ok: false,
				exit_code: -1,
				stdout: String::new(),
				stderr: "no linter on PATH for this project".into(),
				summary: "linter: none available".into(),
			}
		}
	}
}

// ── Smart dev runner ────────────────────────────────────────────────────

/// Read custom commands from dx config (dx file or ~/.config/dx/config.toml).
pub fn read_dx_config_run(cwd: &Path) -> Option<String> {
	// Try project dx file
	let mut path = cwd.join("dx");
	if !path.is_file() {
		// Walk up looking for dx
		for ancestor in cwd.ancestors() {
			let candidate = ancestor.join("dx");
			if candidate.is_file() {
				path = candidate;
				break;
			}
		}
		if !path.is_file() {
			path = dirs::home_dir()?.join(".config/dx/config.toml");
			if !path.is_file() {
				return None;
			}
		}
	}
	let text = std::fs::read_to_string(&path).ok()?;

	// Try TOML format: [tui] run = "cmd"
	if let Ok(v) = text.parse::<toml::Value>()
		&& let Some(cmd) = v.get("tui").and_then(|t| t.get("run")).and_then(|r| r.as_str())
		&& !cmd.is_empty()
	{
		return Some(cmd.to_string());
	}

	// Try dx-serializer format: tui(run = "cmd") using simple line scan
	for line in text.lines() {
		let t = line.trim();
		if t.starts_with("tui(") || t.starts_with("tui ") {
			// Extract run = "..." from within the tui block
			let in_block = text
				.lines()
				.skip_while(|l| !l.trim().starts_with("tui"))
				.skip(1)
				.take_while(|l| !l.trim().eq(")") && !l.trim().starts_with(')'));
			for bl in in_block {
				let bt = bl.trim();
				if let Some(rest) = bt.strip_prefix("run ")
					&& let Some(val) = rest.trim().strip_prefix('=')
				{
					let cmd = val.trim().trim_matches('"').trim().to_string();
					if !cmd.is_empty() {
						return Some(cmd);
					}
				}
			}
		}
	}

	None
}

/// Read custom fmt command from dx config.
#[allow(dead_code)]
pub fn read_dx_config_fmt(cwd: &Path) -> Option<String> {
	read_dx_config_key(cwd, "fmt")
}

/// Read custom lint command from dx config.
#[allow(dead_code)]
pub fn read_dx_config_lint(cwd: &Path) -> Option<String> {
	read_dx_config_key(cwd, "lint")
}

#[allow(dead_code)]
fn read_dx_config_key(cwd: &Path, key: &str) -> Option<String> {
	let mut path = cwd.join("dx");
	if !path.is_file() {
		for ancestor in cwd.ancestors() {
			let candidate = ancestor.join("dx");
			if candidate.is_file() {
				path = candidate;
				break;
			}
		}
		if !path.is_file() {
			path = dirs::home_dir()?.join(".config/dx/config.toml");
			if !path.is_file() {
				return None;
			}
		}
	}
	let text = std::fs::read_to_string(&path).ok()?;

	if let Ok(v) = text.parse::<toml::Value>()
		&& let Some(cmd) = v.get("tui").and_then(|t| t.get(key)).and_then(|r| r.as_str())
		&& !cmd.is_empty()
	{
		return Some(cmd.to_string());
	}

	for line in text.lines() {
		let t = line.trim();
		if t.starts_with("tui(") || t.starts_with("tui ") {
			let in_block = text
				.lines()
				.skip_while(|l| !l.trim().starts_with("tui"))
				.skip(1)
				.take_while(|l| !l.trim().eq(")") && !l.trim().starts_with(')'));
			for bl in in_block {
				let bt = bl.trim();
				let pattern = format!("{} ", key);
				if let Some(rest) = bt.strip_prefix(&pattern)
					&& let Some(val) = rest.trim().strip_prefix('=')
				{
					let cmd = val.trim().trim_matches('"').trim().to_string();
					if !cmd.is_empty() {
						return Some(cmd);
					}
				}
			}
		}
	}

	None
}

/// Detect the most obvious dev command for the project.
pub fn detect_dev_command(cwd: &Path) -> Option<Vec<String>> {
	// Check dx config first
	if let Some(custom) = read_dx_config_run(cwd) {
		let parts: Vec<String> = shlex_split(&custom);
		if !parts.is_empty() {
			return Some(parts);
		}
	}

	let kind = detect_project(cwd);
	match kind {
		ProjectKind::Node => {
			if cwd.join("package.json").is_file()
				&& let Ok(text) = std::fs::read_to_string(cwd.join("package.json"))
				&& let Ok(v) = text.parse::<serde_json::Value>()
			{
				let scripts = v.get("scripts");
				let has_dev = scripts.and_then(|s| s.get("dev")).is_some();
				let has_start = scripts.and_then(|s| s.get("start")).is_some();
				let has_bun = which_bin("bun");
				let _has_npm = which_bin("npm");
				let has_pnpm = which_bin("pnpm");
				let has_yarn = which_bin("yarn");
				let runner = if has_bun {
					"bun"
				} else if has_pnpm {
					"pnpm"
				} else if has_yarn {
					"yarn"
				} else {
					"npm"
				};
				if has_dev {
					return Some(vec![runner.into(), "run".into(), "dev".into()]);
				}
				if has_start {
					return Some(vec![runner.into(), "start".into()]);
				}
				if cwd.join("next.config").is_file()
					|| cwd.join("next.config.ts").is_file()
					|| cwd.join("next.config.mjs").is_file()
				{
					return Some(vec![runner.into(), "run".into(), "dev".into()]);
				}
			}
			None
		}
		ProjectKind::Rust => {
			if cwd.join("Cargo.toml").is_file() {
				Some(vec!["cargo".into(), "run".into()])
			} else {
				None
			}
		}
		ProjectKind::Python => {
			if cwd.join("manage.py").is_file() {
				Some(vec!["python".into(), "manage.py".into(), "runserver".into()])
			} else if cwd.join("app.py").is_file() {
				Some(vec!["python".into(), "app.py".into()])
			} else if cwd.join("main.py").is_file() {
				Some(vec!["python".into(), "main.py".into()])
			} else if which_bin("uvicorn") {
				let main = cwd.file_name().and_then(|n| n.to_str()).unwrap_or("main");
				Some(vec!["uvicorn".into(), format!("{main}.app:app"), "--reload".into()])
			} else {
				None
			}
		}
		ProjectKind::Go => {
			if cwd.join("go.mod").is_file() {
				Some(vec!["go".into(), "run".into(), ".".into()])
			} else {
				None
			}
		}
		ProjectKind::Mixed | ProjectKind::Unknown => {
			// Try common commands in order
			let candidates: Vec<(&str, Vec<&str>)> = vec![
				("Cargo.toml", vec!["cargo", "run"]),
				("package.json", vec!["npm", "run", "dev"]),
				("go.mod", vec!["go", "run", "."]),
				("manage.py", vec!["python", "manage.py", "runserver"]),
				("Makefile", vec!["make"]),
				("justfile", vec!["just"]),
			];
			for (marker, cmd) in &candidates {
				if cwd.join(marker).is_file() {
					if *marker == "package.json" {
						let has_bun = which_bin("bun");
						if has_bun {
							return Some(vec!["bun".into(), "run".into(), "dev".into()]);
						}
					}
					return Some(cmd.iter().map(|s| s.to_string()).collect());
				}
			}
			None
		}
	}
}

fn shlex_split(s: &str) -> Vec<String> {
	// Simple shell-like split respecting quotes
	let mut out = Vec::new();
	let mut current = String::new();
	let mut in_quote = false;
	for ch in s.chars() {
		if ch == '"' {
			in_quote = !in_quote;
		} else if ch == ' ' && !in_quote {
			if !current.is_empty() {
				out.push(std::mem::take(&mut current));
			}
		} else {
			current.push(ch);
		}
	}
	if !current.is_empty() {
		out.push(current);
	}
	out
}

/// Run the dev server command and return output (captured, non-blocking).
#[allow(dead_code)]
pub fn run_dev(cwd: &Path) -> ToolRunResult {
	let cmd_parts = detect_dev_command(cwd);
	let (bin, args) = match cmd_parts {
		Some(p) if !p.is_empty() => (p[0].clone(), p[1..].to_vec()),
		_ => {
			return ToolRunResult {
				tool: "dev-runner".into(),
				ok: false,
				exit_code: -1,
				stdout: String::new(),
				stderr: "no dev command detected for this project".into(),
				summary: "dev-runner: no command".into(),
			};
		}
	};
	let args_ref: Vec<&str> = args.iter().map(String::as_str).collect();
	run_capture(&bin, &args_ref, cwd, "dev server")
}

// ── Diagnostics (LSP-like without full protocol) ────────────────────────

#[derive(Debug, Clone)]
pub struct Diagnostic {
	pub path: String,
	pub line: u32,
	pub col: u32,
	pub severity: DiagSeverity,
	pub message: String,
	#[allow(dead_code)]
	pub source: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagSeverity {
	Error,
	Warning,
	Info,
	#[allow(dead_code)]
	Hint,
}

impl DiagSeverity {
	pub fn glyph(self) -> &'static str {
		match self {
			Self::Error => "E",
			Self::Warning => "W",
			Self::Info => "I",
			Self::Hint => "H",
		}
	}
}

/// Collect diagnostics via cargo/tsc/ruff (LSP-quality signals without a long-lived client).
pub fn collect_diagnostics(cwd: &Path) -> (Vec<Diagnostic>, String) {
	let mut diags = Vec::new();
	let kind = detect_project(cwd);
	let mut summary_parts = Vec::new();

	if matches!(kind, ProjectKind::Rust | ProjectKind::Mixed | ProjectKind::Unknown)
		&& which_bin("cargo")
	{
		let args = cargo_args("check", &["--message-format=json", "-q"]);
		let args_ref: Vec<&str> = args.iter().map(String::as_str).collect();
		let r = run_capture("cargo", &args_ref, cwd, "cargo check");
		let n_before = diags.len();
		parse_cargo_json_diagnostics(&r.stdout, &mut diags);
		// Also scrape short form from stderr if JSON empty
		if diags.len() == n_before {
			parse_short_rustc(&r.stderr, &mut diags);
			parse_short_rustc(&r.stdout, &mut diags);
		}
		summary_parts.push(if r.ok {
			"cargo check: ok".into()
		} else {
			format!("cargo check: {} issues", diags.len().saturating_sub(n_before).max(1))
		});
	}

	if matches!(kind, ProjectKind::Node | ProjectKind::Mixed) && which_bin("tsc") {
		let r = run_capture("tsc", &["--noEmit", "--pretty", "false"], cwd, "tsc");
		let n_before = diags.len();
		parse_tsc_diagnostics(&r.stdout, &mut diags);
		parse_tsc_diagnostics(&r.stderr, &mut diags);
		summary_parts.push(format!("tsc: +{} diags", diags.len() - n_before));
	}

	if matches!(kind, ProjectKind::Python | ProjectKind::Mixed) && which_bin("ruff") {
		let r = run_capture("ruff", &["check", ".", "--output-format", "concise"], cwd, "ruff");
		let n_before = diags.len();
		parse_ruff_concise(&r.stdout, &mut diags);
		summary_parts.push(format!("ruff: +{} diags", diags.len() - n_before));
	}

	// rust-analyzer presence as capability (not full analysis)
	if which_bin("rust-analyzer") {
		summary_parts.push("rust-analyzer: on PATH".into());
	}
	if which_bin("typescript-language-server") {
		summary_parts.push("ts-ls: on PATH".into());
	}
	if which_bin("pyright") {
		summary_parts.push("pyright: on PATH".into());
	}

	// Cap
	if diags.len() > 80 {
		diags.truncate(80);
	}

	let errors = diags.iter().filter(|d| d.severity == DiagSeverity::Error).count();
	let warnings = diags.iter().filter(|d| d.severity == DiagSeverity::Warning).count();
	let summary = if summary_parts.is_empty() {
		format!("diagnostics: {errors}E {warnings}W")
	} else {
		format!("{} · {errors}E {warnings}W", summary_parts.join(" · "))
	};
	(diags, summary)
}

fn parse_cargo_json_diagnostics(stdout: &str, out: &mut Vec<Diagnostic>) {
	for line in stdout.lines() {
		let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
			continue;
		};
		if v.get("reason").and_then(|r| r.as_str()) != Some("compiler-message") {
			continue;
		}
		let msg = match v.get("message") {
			Some(m) => m,
			None => continue,
		};
		let level = msg.get("level").and_then(|l| l.as_str()).unwrap_or("error");
		let severity = match level {
			"error" => DiagSeverity::Error,
			"warning" => DiagSeverity::Warning,
			"note" | "help" => DiagSeverity::Info,
			_ => DiagSeverity::Warning,
		};
		let message = msg.get("message").and_then(|m| m.as_str()).unwrap_or("").to_string();
		let (path, line, col) = msg
			.get("spans")
			.and_then(|s| s.as_array())
			.and_then(|arr| {
				arr.iter().find(|s| s.get("is_primary").and_then(|p| p.as_bool()) == Some(true))
			})
			.map(|span| {
				(
					span.get("file_name").and_then(|f| f.as_str()).unwrap_or("?").to_string(),
					span.get("line_start").and_then(|l| l.as_u64()).unwrap_or(0) as u32,
					span.get("column_start").and_then(|c| c.as_u64()).unwrap_or(0) as u32,
				)
			})
			.unwrap_or_else(|| ("?".into(), 0, 0));
		if message.is_empty() {
			continue;
		}
		out.push(Diagnostic { path, line, col, severity, message, source: "rustc".into() });
	}
}

fn parse_short_rustc(text: &str, out: &mut Vec<Diagnostic>) {
	// src/foo.rs:12:5: error: ...
	for line in text.lines() {
		let Some((loc, rest)) = line.split_once(": ") else {
			continue;
		};
		let severity = if rest.starts_with("error") {
			DiagSeverity::Error
		} else if rest.starts_with("warning") {
			DiagSeverity::Warning
		} else {
			continue;
		};
		let parts: Vec<&str> = loc.split(':').collect();
		if parts.len() < 2 {
			continue;
		}
		let path = parts[0].to_string();
		let line_n = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
		let col = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
		let message = rest.trim_start_matches("error: ").trim_start_matches("warning: ").to_string();
		out.push(Diagnostic { path, line: line_n, col, severity, message, source: "rustc".into() });
	}
}

fn parse_tsc_diagnostics(text: &str, out: &mut Vec<Diagnostic>) {
	// file.ts(10,5): error TS2322: ...
	for line in text.lines() {
		let Some((loc, rest)) = line.split_once("): ") else {
			continue;
		};
		let severity = if rest.contains("error") {
			DiagSeverity::Error
		} else if rest.contains("warning") {
			DiagSeverity::Warning
		} else {
			DiagSeverity::Error
		};
		let (path_part, line_col) = loc.rsplit_once('(').unwrap_or((loc, "0,0"));
		let mut lc = line_col.split(',');
		let line_n = lc.next().and_then(|s| s.parse().ok()).unwrap_or(0);
		let col = lc.next().and_then(|s| s.parse().ok()).unwrap_or(0);
		out.push(Diagnostic {
			path: path_part.trim().to_string(),
			line: line_n,
			col,
			severity,
			message: rest.chars().take(160).collect(),
			source: "tsc".into(),
		});
	}
}

fn parse_ruff_concise(text: &str, out: &mut Vec<Diagnostic>) {
	// path:line:col: CODE message
	for line in text.lines() {
		let parts: Vec<&str> = line.splitn(4, ':').collect();
		if parts.len() < 4 {
			continue;
		}
		let path = parts[0].to_string();
		let line_n = parts[1].parse().unwrap_or(0);
		let col = parts[2].parse().unwrap_or(0);
		out.push(Diagnostic {
			path,
			line: line_n,
			col,
			severity: DiagSeverity::Warning,
			message: parts[3].trim().to_string(),
			source: "ruff".into(),
		});
	}
}

// ── VCS ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct VcsStatus {
	pub kind: &'static str,
	pub available: bool,
	pub branch: String,
	pub dirty: bool,
	pub staged: u32,
	pub unstaged: u32,
	pub untracked: u32,
	pub ahead: u32,
	pub behind: u32,
	pub last_commit: String,
	pub summary: String,
	pub short_status: Vec<String>,
}

pub fn collect_vcs(cwd: &Path) -> VcsStatus {
	if !which_bin("git") {
		return VcsStatus {
			kind: "none",
			available: false,
			summary: "git not on PATH".into(),
			..Default::default()
		};
	}
	// Confirm repo
	let top = Command::new("git").args(["rev-parse", "--show-toplevel"]).current_dir(cwd).output();
	if !matches!(top, Ok(ref o) if o.status.success()) {
		return VcsStatus {
			kind: "git",
			available: false,
			summary: "not a git repo".into(),
			..Default::default()
		};
	}

	let branch =
		git_out(cwd, &["rev-parse", "--abbrev-ref", "HEAD"]).unwrap_or_else(|| "HEAD".into());
	let status = git_out(cwd, &["status", "--porcelain=v1"]).unwrap_or_default();
	let mut staged = 0u32;
	let mut unstaged = 0u32;
	let mut untracked = 0u32;
	let mut short_status = Vec::new();
	for line in status.lines() {
		if line.len() < 2 {
			continue;
		}
		let b0 = line.as_bytes()[0] as char;
		let b1 = line.as_bytes()[1] as char;
		if b0 == '?' {
			untracked += 1;
		} else {
			if b0 != ' ' {
				staged += 1;
			}
			if b1 != ' ' {
				unstaged += 1;
			}
		}
		if short_status.len() < 12 {
			short_status.push(line.chars().take(48).collect());
		}
	}
	let dirty = staged + unstaged + untracked > 0;

	// ahead/behind
	let mut ahead = 0u32;
	let mut behind = 0u32;
	if let Some(ab) = git_out(cwd, &["rev-list", "--left-right", "--count", "HEAD...@{upstream}"]) {
		let mut parts = ab.split_whitespace();
		ahead = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
		behind = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
	}

	let last_commit =
		git_out(cwd, &["log", "-1", "--oneline"]).unwrap_or_else(|| "(no commits)".into());

	let summary = format!(
		"{branch}{} · +{staged} ~{unstaged} ?{untracked} · ↑{ahead} ↓{behind}",
		if dirty { "*" } else { "" },
	);

	VcsStatus {
		kind: "git",
		available: true,
		branch,
		dirty,
		staged,
		unstaged,
		untracked,
		ahead,
		behind,
		last_commit,
		summary,
		short_status,
	}
}

fn git_out(cwd: &Path, args: &[&str]) -> Option<String> {
	let o = Command::new("git").args(args).current_dir(cwd).output().ok()?;
	if !o.status.success() && args.first() != Some(&"rev-list") {
		// rev-list fails without upstream — ok
		if !matches!(args, ["status", ..] | ["log", ..] | ["rev-parse", ..]) {
			return None;
		}
	}
	let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
	if s.is_empty() { None } else { Some(s) }
}

// ── Subagents ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubagentPhase {
	Running,
	Done,
}

impl SubagentPhase {
	pub fn glyph(self) -> &'static str {
		match self {
			Self::Running => "◐",
			Self::Done => "●",
		}
	}
}

#[derive(Debug, Clone)]
pub struct SubagentRecord {
	pub name: String,
	pub phase: SubagentPhase,
	pub preview: String,
	pub line_count: usize,
}

/// Extract subagent blocks from chat messages (newest first, de-dup by name+preview).
pub fn extract_subagents(messages: &[Message]) -> Vec<SubagentRecord> {
	let mut out = Vec::new();
	for msg in messages.iter().rev() {
		let blocks = parse_message_blocks(&msg.content);
		for b in blocks {
			if let MessageBlock::Subagent { name, lines } = b {
				let preview: String = lines
					.iter()
					.map(|l| l.trim())
					.find(|l| !l.is_empty())
					.unwrap_or("")
					.chars()
					.take(40)
					.collect();
				// Open tag without close in streaming = running
				let open_count = msg.content.matches(&format!("<subagent name=\"{name}\"")).count()
					+ msg.content.matches("<subagent").count();
				let close_count = msg.content.matches("</subagent>").count();
				// Heuristic: if this is the last message and more opens than closes overall, running
				let phase = if open_count > close_count
					|| (msg.content.contains(&format!("<subagent name=\"{name}\""))
						&& !msg.content.contains("</subagent>"))
				{
					SubagentPhase::Running
				} else {
					SubagentPhase::Done
				};
				let rec = SubagentRecord { name: name.clone(), phase, preview, line_count: lines.len() };
				if !out.iter().any(|r: &SubagentRecord| r.name == rec.name && r.preview == rec.preview) {
					out.push(rec);
				}
			}
		}
		if out.len() >= 24 {
			break;
		}
	}
	// Also detect unclosed streaming subagent on last assistant
	if let Some(last) = messages.last()
		&& last.content.contains("<subagent")
		&& !last.content.contains("</subagent>")
		&& let Some(rest) = last.content.rfind("<subagent")
	{
		let slice = &last.content[rest..];
		let name = slice
			.split_once('>')
			.and_then(|(tag, _)| tag.split("name=\"").nth(1).and_then(|s| s.split('"').next()))
			.unwrap_or("subagent");
		if !out.iter().any(|r| r.phase == SubagentPhase::Running && r.name == name) {
			out.insert(
				0,
				SubagentRecord {
					name: name.into(),
					phase: SubagentPhase::Running,
					preview: "streaming…".into(),
					line_count: 0,
				},
			);
		}
	}
	out
}

/// Optional agent aliases from env only (no stack marketing names).
#[allow(dead_code)]
pub fn known_agent_aliases() -> Vec<String> {
	std::env::var("DX_TUI_AGENT").ok().filter(|a| !a.is_empty()).into_iter().collect()
}

// ── Tool inventory for sidebar ──────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ToolInventoryItem {
	pub name: String,
	pub kind: &'static str,
	pub available: bool,
	pub detail: String,
}

pub fn tool_inventory(cwd: &Path) -> Vec<ToolInventoryItem> {
	let kind = detect_project(cwd);
	let mut items = Vec::new();

	let push =
		|items: &mut Vec<ToolInventoryItem>, name: &str, k: &'static str, bin: &str, detail: &str| {
			let available = which_bin(bin);
			items.push(ToolInventoryItem {
				name: name.into(),
				kind: k,
				available,
				detail: if available { detail.into() } else { format!("{bin} missing") },
			});
		};

	push(&mut items, "cargo fmt", "fmt", "cargo", "Rust formatter");
	push(&mut items, "clippy", "lint", "cargo", "Rust linter");
	push(&mut items, "rust-analyzer", "lsp", "rust-analyzer", "Rust LSP");
	push(&mut items, "prettier", "fmt", "prettier", "JS/TS format");
	push(&mut items, "eslint", "lint", "eslint", "JS/TS lint");
	push(&mut items, "tsc", "lsp", "tsc", "TypeScript check");
	push(&mut items, "ruff", "lint", "ruff", "Python lint/format");
	push(&mut items, "pyright", "lsp", "pyright", "Python LSP");
	push(&mut items, "gofmt", "fmt", "gofmt", "Go format");
	push(&mut items, "gopls", "lsp", "gopls", "Go LSP");
	push(&mut items, "git", "vcs", "git", "Version control");
	push(&mut items, "biome", "fmt", "biome", "Web toolchain");

	// Project kind first
	items.sort_by_key(|i| match (kind, i.kind) {
		(ProjectKind::Rust, "fmt" | "lint" | "lsp")
			if i.name.contains("cargo") || i.name.contains("rust") =>
		{
			0
		}
		(ProjectKind::Node, _)
			if i.name.contains("eslint") || i.name.contains("prettier") || i.name.contains("tsc") =>
		{
			0
		}
		(ProjectKind::Python, _) if i.name.contains("ruff") || i.name.contains("pyright") => 0,
		(ProjectKind::Go, _) if i.name.contains("go") => 0,
		_ if i.available => 1,
		_ => 2,
	});
	items
}

/// Combined workspace doctor report for `/doctor` / plan attach.
pub fn workspace_doctor(cwd: &Path) -> String {
	let kind = detect_project(cwd);
	let vcs = collect_vcs(cwd);
	let tools = tool_inventory(cwd);
	let ready = tools.iter().filter(|t| t.available).count();
	let (diags, diag_sum) = collect_diagnostics(cwd);
	format!(
		"Workspace doctor\n\
		 project: {kind:?}\n\
		 cwd: {}\n\
		 vcs: {}\n\
		 tools: {ready}/{} on PATH\n\
		 {}\n\
		 diagnostics: {} entries\n\
		 fmt: {}\n\
		 lint: {}",
		cwd.display(),
		vcs.summary,
		tools.len(),
		diag_sum,
		diags.len(),
		run_formatter(cwd).summary,
		run_linter(cwd).summary,
	)
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::path::PathBuf;

	#[test]
	fn which_git_or_not() {
		// Just ensure function runs
		let _ = which_bin("git");
		let _ = which_bin("definitely-not-a-real-bin-xyz");
	}

	#[test]
	fn parse_short_rustc_line() {
		let mut d = Vec::new();
		parse_short_rustc("src/main.rs:10:5: error: cannot find value `x`\n", &mut d);
		assert_eq!(d.len(), 1);
		assert_eq!(d[0].severity, DiagSeverity::Error);
		assert_eq!(d[0].line, 10);
	}

	#[test]
	fn parse_cargo_json_sample() {
		let sample = r#"{"reason":"compiler-message","message":{"level":"error","message":"boom","spans":[{"file_name":"src/a.rs","line_start":3,"column_start":1,"is_primary":true}]}}"#;
		let mut d = Vec::new();
		parse_cargo_json_diagnostics(sample, &mut d);
		assert_eq!(d.len(), 1);
		assert_eq!(d[0].path, "src/a.rs");
	}

	#[test]
	fn extract_subagent_from_message() {
		let m =
			Message::assistant("<subagent name=\"explore\">\nsearching…\n</subagent>\nDone.".into());
		let recs = extract_subagents(&[m]);
		assert!(!recs.is_empty());
		assert_eq!(recs[0].name, "explore");
	}

	#[test]
	fn detect_rust_project() {
		// This crate's root is Rust
		let cwd = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
		assert_eq!(detect_project(&cwd), ProjectKind::Rust);
	}
}
