//! dx-flow local model backend (llama.cpp under the hood).
//!
//! When `dx-flow` feature is disabled, all public items become no-ops.

#[cfg(feature = "dx-flow")]
mod inner {
	use anyhow::{Context, Result, bail};

	use crate::modes::ModelEntry;

	/// Shared handle for local inference via dx-flow (always in-process).
	pub struct FlowBackend {
		runtime: Option<dx_flow::runtime::FlowLocalRuntime>,
		/// Model key last selected for local runs (flow catalog key).
		selected_model_key: Option<String>,
		/// Cached menu entries from the last discovery.
		cached_models: Vec<ModelEntry>,
	}

	impl FlowBackend {
		pub fn new() -> Self {
			Self { runtime: None, selected_model_key: None, cached_models: Vec::new() }
		}

		/// Initialize dx-flow runtime (detect device + warm llama.cpp default text model).
		pub async fn init(&mut self) -> Result<()> {
			// Default to 8 llama.cpp threads unless overridden by env
			if std::env::var("FLOW_LLAMA_THREADS").is_err() {
				// SAFETY: single-threaded at startup before any other code reads this env var
				unsafe {
					std::env::set_var("FLOW_LLAMA_THREADS", "8");
				}
			}
			let runtime = dx_flow::runtime::FlowLocalRuntime::detect()
				.context("dx-flow FlowLocalRuntime::detect failed")?;
			if let Err(e) = runtime.warm_text_model().await {
				tracing::debug!("dx-flow warm_text_model: {e}");
			}
			self.runtime = Some(runtime);
			self.cached_models = discover_via_flow_runtime();
			Ok(())
		}

		pub fn is_ready(&self) -> bool {
			self.runtime.is_some()
		}

		pub fn set_selected_model(&mut self, model_key: impl Into<String>) {
			self.selected_model_key = Some(model_key.into());
		}

		pub fn selected_model_key(&self) -> Option<&str> {
			self.selected_model_key.as_deref()
		}

		pub fn cached_models(&self) -> &[ModelEntry] {
			&self.cached_models
		}

		/// Refresh local model list (flow/models + env; auto all-drives if library empty).
		pub fn refresh_models(&mut self) {
			self.cached_models = discover_local_models();
		}

		/// Run a prompt through dx-flow / llama.cpp.
		pub async fn generate(&self, prompt: &str) -> Result<String> {
			let runtime =
				self.runtime.as_ref().ok_or_else(|| anyhow::anyhow!("dx-flow runtime not initialized"))?;
			let key = self.selected_model_key.as_deref().unwrap_or("");
			let text = if key == "qwen35-4b-revised-q4km"
				|| (key.contains("coding") && !key.contains("tooluse"))
			{
				runtime.generate_coding_text_with_metrics(prompt).await?.0
			} else if key == "ministral3-3b-instruct-q4km" || key.contains("quality") {
				runtime.generate_quality_chat_with_metrics(prompt).await?.0
			} else if key == "minicpm5-1b-tooluse"
				|| key == "xlam2-3b-fc-r-q4km"
				|| key.contains("tooluse")
				|| key.contains("tool")
			{
				runtime.generate_tool_agent_with_metrics(prompt).await?.0
			} else if key == "smolchat"
				|| key == "smollm2-135m-instruct"
				|| key == "qwen3-0.6b"
				|| key.contains("helper")
				|| key.contains("smol")
			{
				runtime.generate_helper_text_with_metrics(prompt).await?.0
			} else {
				runtime.generate_text(prompt).await?
			};
			Ok(text)
		}
	}

	impl Default for FlowBackend {
		fn default() -> Self {
			Self::new()
		}
	}

	/// Canonical on-disk model library for dx-flow (Windows default: `G:\Dx\flow\models`).
	pub fn flow_models_dir() -> std::path::PathBuf {
		if let Ok(p) = std::env::var("DX_FLOW_MODELS_DIR") {
			let path = std::path::PathBuf::from(p.trim());
			if !path.as_os_str().is_empty() {
				return path;
			}
		}
		// Prefer sibling checkout / known monorepo path
		for cand in [
			std::path::PathBuf::from(r"G:\Dx\flow\models"),
			std::path::PathBuf::from("G:/Dx/flow/models"),
			std::path::PathBuf::from("../flow/models"),
			std::path::PathBuf::from("../../flow/models"),
		] {
			if cand.is_dir() || cand.parent().is_some_and(|p| p.is_dir()) {
				return cand;
			}
		}
		std::path::PathBuf::from(r"G:\Dx\flow\models")
	}

	/// Discover local models for the AI models menu (normal path).
	///
	/// 1. dx-flow runtime / CLI catalog  
	/// 2. GGUF under `G:\Dx\flow\models` + configured dirs  
	/// 3. If **flow models dir has no GGUF**, automatic all-drives scan (C–Z)  
	///
	/// Use [`discover_local_models_full_scan`] for an explicit full-drive scan
	/// even when models already exist.
	pub fn discover_local_models() -> Vec<ModelEntry> {
		discover_local_models_impl(false)
	}

	/// Force a full multi-drive GGUF scan and merge with normal discovery.
	/// Intended for the "Scan all drives" menu action when models already exist.
	pub fn discover_local_models_full_scan() -> Vec<ModelEntry> {
		discover_local_models_impl(true)
	}

	fn discover_local_models_impl(force_full_drive_scan: bool) -> Vec<ModelEntry> {
		let mut out: Vec<ModelEntry> = Vec::new();

		// ONLY real chat LLM GGUFs under flow/models (and llm/), never stt/tts/vosk/wake/init.
		for entry in discover_chat_gguf_only() {
			push_local_unique(&mut out, entry);
		}

		// Optional: merge flow CLI keys that match a file we already found (don't invent STT/TTS).
		if let Ok(list) = discover_via_cli() {
			for entry in list {
				if out.iter().any(|m| m.model_id.eq_ignore_ascii_case(&entry.model_id)) {
					// Prefer CLI "available" flag if it marks ready
					if entry.available
						&& let Some(slot) =
							out.iter_mut().find(|m| m.model_id.eq_ignore_ascii_case(&entry.model_id))
					{
						slot.available = true;
					}
				}
			}
		}

		let flow_empty = out.is_empty();
		if force_full_drive_scan || flow_empty {
			let found = discover_gguf_all_drives();
			if flow_empty && !found.is_empty() {
				relocate_gguf_paths_to_flow_models(&found);
				for entry in discover_chat_gguf_only() {
					push_local_unique(&mut out, entry);
				}
			}
			// Full-drive: only actual .gguf files (already filtered in collect_gguf)
			for entry in found {
				if is_plausible_local_model(&entry) {
					push_local_unique(&mut out, entry);
				}
			}
		}

		out.retain(|m| is_plausible_local_model(m) || m.model_id == "dx-flow-pending");
		let has_real =
			out.iter().any(|m| m.is_selectable_model() && m.available && m.model_id != "dx-flow-pending");
		if has_real {
			out.retain(|m| m.model_id != "dx-flow-pending");
		}

		if out.is_empty() {
			out.push(ModelEntry::local(
				"No local GGUF · put .gguf files in G:\\Dx\\flow\\models\\llm",
				"dx-flow-pending",
				false,
			));
		}

		out
	}

	/// Chat LLM GGUFs only — ignore STT / TTS / Vosk / wake-words / init trees.
	fn discover_chat_gguf_only() -> Vec<ModelEntry> {
		use std::path::PathBuf;

		let mut roots: Vec<PathBuf> = Vec::new();
		let primary = flow_models_dir();
		roots.push(primary.clone());

		if let Ok(p) = std::env::var("DX_TUI_MODEL_PATH") {
			let path = PathBuf::from(p.trim());
			if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("gguf") {
				// Single file override
				if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
					return vec![ModelEntry {
						display_name: humanize_model_key(stem),
						model_id: stem.to_string(),
						provider: "dx-flow".into(),
						is_local: true,
						available: true,
						reasoning_capable: false,
					}];
				}
			} else if path.is_dir() {
				roots.push(path);
			}
		}
		if let Ok(list) = std::env::var("DX_FLOW_MODEL_DIRS") {
			for part in list.split([';', '|']) {
				let part = part.trim();
				if part.is_empty() {
					continue;
				}
				let path = PathBuf::from(part);
				if path.is_dir() {
					roots.push(path);
				}
			}
		}

		roots.sort();
		roots.dedup();

		let mut out = Vec::new();
		for root in roots {
			if !root.is_dir() {
				continue;
			}
			// 1) *.gguf directly in models/
			collect_gguf_in_flat_dir(&root, &mut out, 32);
			// 2) models/llm/** only (chat weights) — never stt/tts/vosk/wake_words
			for sub in ["llm", "chat", "text", "gguf"] {
				let p = root.join(sub);
				if p.is_dir() {
					collect_gguf(&p, 0, 4, &mut out, 32);
				}
			}
		}
		out
	}

	/// Collect `*.gguf` in one directory only (no recursion into stt/tts/…).
	fn collect_gguf_in_flat_dir(dir: &std::path::Path, out: &mut Vec<ModelEntry>, cap: usize) {
		let Ok(rd) = std::fs::read_dir(dir) else {
			return;
		};
		for ent in rd.flatten() {
			if out.len() >= cap {
				break;
			}
			let path = ent.path();
			if !path.is_file() {
				continue;
			}
			if !path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("gguf"))
			{
				continue;
			}
			let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
			if size > 0 && size < 1_000_000 {
				continue;
			}
			let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("model").to_string();
			if out.iter().any(|m| m.model_id == stem) {
				continue;
			}
			let entry = ModelEntry {
				display_name: humanize_model_key(&stem),
				model_id: stem,
				provider: "dx-flow".into(),
				is_local: true,
				available: true,
				reasoning_capable: false,
			};
			if is_plausible_local_model(&entry) {
				out.push(entry);
			}
		}
	}

	fn push_local_unique(out: &mut Vec<ModelEntry>, entry: ModelEntry) {
		if out.iter().any(|m| m.model_id == entry.model_id) {
			return;
		}
		out.push(entry);
	}

	/// Reject cargo/path junk that is not a real local model entry.
	fn is_plausible_local_model(m: &ModelEntry) -> bool {
		if !m.is_local {
			return false;
		}
		if m.model_id == "dx-flow-pending" {
			return true;
		}
		let id = m.model_id.to_ascii_lowercase();
		let name = m.display_name.to_ascii_lowercase();
		// Reject path-like / build-system noise
		if id.contains('\\')
			|| id.contains('/')
			|| id.contains("target")
			|| id.contains("cargo")
			|| id.contains("node_modules")
			|| id.contains(".dll")
			|| id.contains(".exe")
			|| id.contains(".rlib")
			|| id.contains(".rmeta")
			|| name.contains("cargo")
			|| name.contains("target\\")
			|| name.contains("target/")
		{
			return false;
		}
		// Model keys are short-ish stems
		if id.len() > 96 || id.is_empty() {
			return false;
		}
		// Must look like a model id (alnum / - _ .)
		if !id
			.chars()
			.all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '+')
		{
			return false;
		}
		true
	}

	/// Walk fixed drives C:–Z: for `*.gguf` (skips Windows/system dirs). Model ids stay as stems.
	fn discover_gguf_all_drives() -> Vec<ModelEntry> {
		use std::path::PathBuf;

		let mut out = Vec::new();
		// Windows volume roots
		for letter in b'C'..=b'Z' {
			let root = PathBuf::from(format!("{}:\\", letter as char));
			if !root.is_dir() {
				continue;
			}
			collect_gguf(&root, 0, 5, &mut out, 80);
			if out.len() >= 80 {
				break;
			}
		}
		// Unix-ish fallback roots when not on Windows volumes
		#[cfg(not(windows))]
		{
			for root in ["/", "/home", "/opt", "/usr/local"] {
				let p = PathBuf::from(root);
				if p.is_dir() {
					collect_gguf(&p, 0, 4, &mut out, 80);
				}
			}
		}
		out
	}

	/// Copy/move discovered GGUF files into `flow_models_dir()` when they live elsewhere.
	/// Prefer rename; fall back to copy. Skips if same size already present.
	fn relocate_gguf_paths_to_flow_models(entries: &[ModelEntry]) {
		use std::path::PathBuf;

		let dest_root = flow_models_dir();
		let _ = std::fs::create_dir_all(&dest_root);

		// We only know stems in ModelEntry — re-scan drives lightly for paths matching ids
		// Prefer a second targeted collect that records paths.
		let mut paths: Vec<PathBuf> = Vec::new();
		for letter in b'C'..=b'Z' {
			let root = PathBuf::from(format!("{}:\\", letter as char));
			if root.is_dir() {
				collect_gguf_paths(&root, 0, 5, &mut paths, 80);
			}
		}

		for path in paths {
			// Skip already under flow models
			if path.starts_with(&dest_root) {
				continue;
			}
			let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
				continue;
			};
			// Only relocate if this stem was in the empty-flow discovery set
			let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
			if !entries.iter().any(|e| e.model_id == stem) {
				continue;
			}
			let dest = dest_root.join(name);
			if dest.exists()
				&& let (Ok(a), Ok(b)) = (std::fs::metadata(&path), std::fs::metadata(&dest))
				&& a.len() == b.len()
			{
				continue;
			}
			// Prefer rename within same volume; else copy
			if std::fs::rename(&path, &dest).is_err() {
				let _ = std::fs::copy(&path, &dest);
			}
		}
	}

	fn collect_gguf_paths(
		dir: &std::path::Path,
		depth: u8,
		max_depth: u8,
		out: &mut Vec<std::path::PathBuf>,
		cap: usize,
	) {
		if depth > max_depth || out.len() >= cap {
			return;
		}
		let Ok(rd) = std::fs::read_dir(dir) else {
			return;
		};
		for ent in rd.flatten() {
			if out.len() >= cap {
				break;
			}
			let path = ent.path();
			if path.is_dir() {
				let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
				if should_skip_dir(name) {
					continue;
				}
				collect_gguf_paths(&path, depth + 1, max_depth, out, cap);
			} else if path
				.extension()
				.and_then(|e| e.to_str())
				.is_some_and(|e| e.eq_ignore_ascii_case("gguf"))
			{
				out.push(path);
			}
		}
	}

	fn should_skip_dir(name: &str) -> bool {
		if name.starts_with('.') {
			return true;
		}
		matches!(
			name.to_ascii_lowercase().as_str(),
			// OS / build noise
			"windows"
			| "system volume information"
			| "$recycle.bin"
			| "recycle.bin"
			| "program files"
			| "program files (x86)"
			| "programdata"
			| "node_modules"
			| ".git"
			| "target"
			| "debug"
			| "release"
			| "deps"
			| "incremental"
			| "build"
			| "appdata"
			| "recovery"
			| "msocache"
			| "perflogs"
			| "config.msi"
			| "cargo"
			| ".cargo"
			| "rustup"
			| "pkg"
			| "site-packages"
			| "__pycache__"
			| ".cache"
			// Non-chat modality trees inside flow/models (these are NOT LLM GGUFs)
			| "stt"
			| "tts"
			| "vosk"
			| "wake_words"
			| "wake"
			| "wake-words"
			| "init"
			| "asr"
			| "speech"
			| "embedding"
			| "embeddings"
			| "embed"
			| "vision"
			| "image"
			| "images"
			| "audio"
		)
	}

	fn collect_gguf(
		dir: &std::path::Path,
		depth: u8,
		max_depth: u8,
		out: &mut Vec<ModelEntry>,
		cap: usize,
	) {
		if depth > max_depth || out.len() >= cap {
			return;
		}
		// Never walk cargo/build trees even if nested under a models path
		let path_s = dir.to_string_lossy().to_ascii_lowercase();
		if path_s.contains("\\target\\")
			|| path_s.contains("/target/")
			|| path_s.contains("\\.cargo\\")
			|| path_s.contains("/.cargo/")
			|| path_s.contains("\\node_modules\\")
		{
			return;
		}
		let Ok(rd) = std::fs::read_dir(dir) else {
			return;
		};
		for ent in rd.flatten() {
			if out.len() >= cap {
				break;
			}
			let path = ent.path();
			if path.is_dir() {
				let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
				if should_skip_dir(name) {
					continue;
				}
				collect_gguf(&path, depth + 1, max_depth, out, cap);
			} else if path
				.extension()
				.and_then(|e| e.to_str())
				.is_some_and(|e| e.eq_ignore_ascii_case("gguf"))
			{
				// Skip tiny / non-model files
				let meta = std::fs::metadata(&path).ok();
				let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
				if size > 0 && size < 1_000_000 {
					// < 1MB almost never a real chat GGUF
					continue;
				}
				let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("model").to_string();
				let id = stem.clone();
				if out.iter().any(|m| m.model_id == id) {
					continue;
				}
				let entry = ModelEntry {
					display_name: humanize_model_key(&stem),
					model_id: id,
					provider: "dx-flow".into(),
					is_local: true,
					available: path.is_file(),
					reasoning_capable: false,
				};
				if is_plausible_local_model(&entry) {
					out.push(entry);
				}
			}
		}
	}

	/// In-process: use flow's own catalog + path readiness (relative to flow CWD / env).
	fn discover_via_flow_runtime() -> Vec<ModelEntry> {
		use dx_flow::runtime::{Modality, RuntimeBroker};
		use std::path::Path;

		let broker = RuntimeBroker::detect();
		broker
			.models_for(Modality::Chat)
			.into_iter()
			.map(|m| {
				let available = m.local_path.as_deref().map(|p| Path::new(p).is_file()).unwrap_or(false);
				ModelEntry {
					display_name: m.display_name.clone(),
					model_id: m.key.clone(),
					provider: "dx-flow".to_string(),
					is_local: true,
					available,
					reasoning_capable: m.tags.iter().any(|t| t == "reasoning"),
				}
			})
			.collect()
	}

	/// CLI: `flow models` (or `dx-flow models`) — flow decides roots, not the TUI.
	fn discover_via_cli() -> Result<Vec<ModelEntry>> {
		let cmd = if which_cmd("flow") {
			"flow"
		} else if which_cmd("dx-flow") {
			"dx-flow"
		} else {
			bail!("flow CLI not found");
		};

		let output = std::process::Command::new(cmd)
			.args(["models", "chat"])
			.output()
			.or_else(|_| std::process::Command::new(cmd).args(["models"]).output())
			.with_context(|| format!("failed to run `{cmd} models`"))?;

		if !output.status.success() {
			let err = String::from_utf8_lossy(&output.stderr);
			bail!("{cmd} models failed: {}", err.trim());
		}

		let stdout = String::from_utf8_lossy(&output.stdout);
		Ok(parse_flow_models_cli(&stdout))
	}

	/// Parse plain-text `flow models` output lines like:
	/// `  - qwen3-0.6b [local]` / `  - foo [missing]` / `key  ready`
	fn parse_flow_models_cli(stdout: &str) -> Vec<ModelEntry> {
		let mut out = Vec::new();
		for line in stdout.lines() {
			let trimmed = line.trim().trim_start_matches('-').trim();
			if trimmed.is_empty()
				|| trimmed.starts_with("Local")
				|| trimmed.starts_with("Model")
				|| trimmed.starts_with("STT")
				|| trimmed.starts_with("TTS")
				|| trimmed.starts_with("Vosk")
				|| trimmed.starts_with("Wake")
				|| trimmed.starts_with("Init")
				|| trimmed.to_ascii_lowercase().contains("speech")
				|| trimmed.to_ascii_lowercase().contains("parakeet")
				|| trimmed.to_ascii_lowercase().contains("kokoro")
				|| trimmed.to_ascii_lowercase().contains("vosk")
			{
				continue;
			}
			// "qwen3-0.6b [local]" or "qwen3-0.6b [missing]"
			let (key_part, status) = if let Some((k, rest)) = trimmed.split_once('[') {
				(k.trim(), rest.trim_end_matches(']').trim())
			} else {
				let parts: Vec<_> = trimmed.split_whitespace().collect();
				if parts.is_empty() {
					continue;
				}
				(parts[0], parts.get(1).copied().unwrap_or(""))
			};
			let key_l = key_part.to_ascii_lowercase();
			// Reject non-chat modalities from flow catalog
			if key_l.contains("stt")
				|| key_l.contains("tts")
				|| key_l.contains("vosk")
				|| key_l.contains("wake")
				|| key_l.contains("parakeet")
				|| key_l.contains("kokoro")
				|| key_l.contains("moonshine")
				|| key_l.contains("whisper")
				|| key_l == "init"
				|| key_l.starts_with("init-")
			{
				continue;
			}
			if (key_part.is_empty() || key_part.contains(' ') && !key_part.contains('-'))
				&& !key_part.contains('-')
				&& !key_part.contains('.')
			{
				continue;
			}
			if !key_part.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.' || c == '_') {
				continue;
			}
			let available = status.eq_ignore_ascii_case("local")
				|| status.eq_ignore_ascii_case("ready")
				|| status.eq_ignore_ascii_case("ok");
			let display = humanize_model_key(key_part);
			let entry = ModelEntry::local(&display, key_part, available);
			if is_plausible_local_model(&entry) {
				out.push(entry);
			}
		}
		out
	}

	fn humanize_model_key(key: &str) -> String {
		key
			.split(['-', '_'])
			.map(|part| {
				let mut chars = part.chars();
				match chars.next() {
					Some(c) => format!("{}{}", c.to_uppercase(), chars.as_str()),
					None => String::new(),
				}
			})
			.collect::<Vec<_>>()
			.join(" ")
	}

	#[allow(dead_code)]
	async fn generate_via_cli(prompt: &str, model_key: Option<&str>) -> Result<String> {
		let cmd = if which_cmd("flow") {
			"flow"
		} else if which_cmd("dx-flow") {
			"dx-flow"
		} else {
			bail!("flow CLI not found");
		};

		let mut args = vec!["chat".to_string(), "--once".to_string()];
		if let Some(key) = model_key
			&& !key.is_empty()
			&& key != "dx-flow-pending"
		{
			args.push("--model".to_string());
			args.push(key.to_string());
		}
		args.push(prompt.to_string());

		let output = tokio::process::Command::new(cmd)
			.args(&args)
			.output()
			.await
			.with_context(|| format!("failed to spawn {cmd}"))?;

		if !output.status.success() {
			// Fallback without --model if the CLI rejected it
			if model_key.is_some() {
				let output = tokio::process::Command::new(cmd)
					.args(["chat", "--once", prompt])
					.output()
					.await
					.with_context(|| format!("failed to spawn {cmd}"))?;
				if output.status.success() {
					return Ok(String::from_utf8_lossy(&output.stdout).trim().to_string());
				}
			}
			let err = String::from_utf8_lossy(&output.stderr);
			bail!("{cmd} failed: {}", err.trim());
		}
		Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
	}

	fn which_cmd(name: &str) -> bool {
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

	#[cfg(test)]
	mod tests {
		use super::*;

		#[test]
		fn parse_flow_models_cli_local_missing() {
			let text = r#"
Local chat candidates:
  - qwen3-0.6b [local]
  - vibethinker-3b [missing]
"#;
			let list = parse_flow_models_cli(text);
			assert_eq!(list.len(), 2);
			assert_eq!(list[0].model_id, "qwen3-0.6b");
			assert!(list[0].available);
			assert!(!list[1].available);
			assert_eq!(list[0].provider, "dx-flow");
		}

		#[test]
		fn discover_local_models_ids_are_stems_not_drive_paths() {
			// Even if a full-drive scan runs, menu ids stay as file stems.
			let list = discover_local_models();
			for m in list {
				if !m.is_selectable_model() {
					continue;
				}
				assert!(!m.model_id.contains(":\\"), "id must not be a drive path: {}", m.model_id);
				assert!(!m.model_id.contains('/'), "id must be a stem: {}", m.model_id);
				assert!(
					is_plausible_local_model(&m) || m.model_id == "dx-flow-pending",
					"junk model leaked: {} / {}",
					m.model_id,
					m.display_name
				);
			}
		}

		#[test]
		fn chat_discovery_skips_non_llm_modality_names() {
			// CLI-style noise that used to pollute the menu
			let text = r#"
Local chat candidates:
  - Qwen3-0.6B-Q4_K_M [local]
  - stt-parakeet [local]
  - tts-kokoro [ready]
  - vosk-small [local]
  - wake-dx [ok]
  - init [local]
"#;
			let list = parse_flow_models_cli(text);
			assert_eq!(list.len(), 1, "only chat GGUF key should remain: {list:?}");
			assert_eq!(list[0].model_id, "Qwen3-0.6B-Q4_K_M");
		}

		#[test]
		fn rejects_cargo_junk_ids() {
			let junk = ModelEntry {
				model_id: "cargo-metadata".into(),
				display_name: "Cargo Metadata".into(),
				provider: "dx-flow".into(),
				is_local: true,
				available: true,
				reasoning_capable: false,
			};
			assert!(!is_plausible_local_model(&junk));
			let path_junk = ModelEntry {
				model_id: r"target\debug\foo".into(),
				display_name: "Foo".into(),
				provider: "dx-flow".into(),
				is_local: true,
				available: true,
				reasoning_capable: false,
			};
			assert!(!is_plausible_local_model(&path_junk));
			let good = ModelEntry::local("Qwen3 0.6B", "Qwen3-0.6B-Q4_K_M", true);
			assert!(is_plausible_local_model(&good));
		}

		#[test]
		fn flow_models_dir_points_at_dx_flow_models() {
			let p = flow_models_dir();
			let s = p.to_string_lossy().to_ascii_lowercase();
			assert!(
				s.contains("flow") && s.contains("models"),
				"unexpected flow models dir: {}",
				p.display()
			);
		}
	}
}

#[cfg(feature = "dx-flow")]
pub use inner::*;

#[cfg(not(feature = "dx-flow"))]
mod stub {
	use crate::modes::ModelEntry;

	pub struct FlowBackend {
		_private: (),
	}

	impl FlowBackend {
		pub fn new() -> Self {
			Self { _private: () }
		}

		pub async fn init(&mut self) -> anyhow::Result<()> {
			anyhow::bail!("dx-flow feature not enabled")
		}

		pub fn is_ready(&self) -> bool {
			false
		}

		pub fn set_selected_model(&mut self, _model_key: impl Into<String>) {}

		pub fn selected_model_key(&self) -> Option<&str> {
			None
		}

		pub fn cached_models(&self) -> &[ModelEntry] {
			&[]
		}

		pub fn refresh_models(&mut self) {}

		pub async fn generate(&self, _prompt: &str) -> anyhow::Result<String> {
			anyhow::bail!("dx-flow feature not enabled")
		}
	}

	impl Default for FlowBackend {
		fn default() -> Self {
			Self::new()
		}
	}

	pub fn flow_models_dir() -> std::path::PathBuf {
		std::path::PathBuf::from(r"G:\Dx\flow\models")
	}

	pub fn discover_local_models() -> Vec<ModelEntry> {
		Vec::new()
	}

	pub fn discover_local_models_full_scan() -> Vec<ModelEntry> {
		Vec::new()
	}
}

#[cfg(not(feature = "dx-flow"))]
pub use stub::*;
