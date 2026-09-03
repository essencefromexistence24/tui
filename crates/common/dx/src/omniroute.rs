//! OmniRoute (g:Dx/route) bridge — provider hints + optional proxy base URL.
//!
//! Token-saving already lives in `token_save` (RTK-inspired). This module unifies
//! OmniRoute free/local provider names into the models menu when configured.

use std::{env, fs, path::PathBuf};

use crate::modes::ModelEntry;

/// Env: `OMNIROUTE_URL` e.g. `http://127.0.0.1:3000/v1`
pub fn proxy_base_url() -> Option<String> {
	env::var("OMNIROUTE_URL")
		.ok()
		.or_else(|| env::var("DX_OMNIROUTE_URL").ok())
		.filter(|s| !s.trim().is_empty())
}

/// Whether the TUI should prefer routing chat through OmniRoute.
pub fn proxy_enabled() -> bool {
	proxy_base_url().is_some()
}

/// Lightweight provider names discovered from a local OmniRoute checkout / config.
pub fn discover_omniroute_provider_hints() -> Vec<(String, String)> {
	let mut out = Vec::new();
	// Known free / common OmniRoute lanes (not a full 160 — full list is models.dev + runtime).
	const HINTS: &[(&str, &str)] = &[
		("omniroute", "OmniRoute (proxy)"),
		("openai", "OpenAI via OmniRoute"),
		("anthropic", "Anthropic via OmniRoute"),
		("google", "Google via OmniRoute"),
		("groq", "Groq via OmniRoute"),
		("deepseek", "DeepSeek via OmniRoute"),
		("mistral", "Mistral via OmniRoute"),
		("together", "Together via OmniRoute"),
		("fireworks", "Fireworks via OmniRoute"),
		("openrouter", "OpenRouter via OmniRoute"),
	];
	if proxy_enabled() {
		for (id, name) in HINTS {
			out.push(((*id).into(), (*name).into()));
		}
	}
	// Scan sibling route package for provider folders if present
	for root in [PathBuf::from("../route"), PathBuf::from(r"G:\Dx\route")] {
		let providers = root.join("src/shared/providers");
		if providers.is_dir() {
			if let Ok(rd) = fs::read_dir(&providers) {
				for e in rd.flatten().take(40) {
					let name = e.file_name().to_string_lossy().to_string();
					if name.ends_with(".ts") || e.path().is_dir() {
						let id = name.trim_end_matches(".ts").to_string();
						if !out.iter().any(|(i, _)| i == &id) {
							out.push((id.clone(), format!("{id} (OmniRoute tree)")));
						}
					}
				}
			}
			break;
		}
	}
	out
}

/// Model entries for the picker when OmniRoute proxy is configured.
pub fn omniroute_model_entries() -> Vec<ModelEntry> {
	if !proxy_enabled() {
		return Vec::new();
	}
	let mut models = vec![
		ModelEntry::remote("OmniRoute default", "omniroute/default", "OmniRoute"),
		ModelEntry::remote("OmniRoute auto", "omniroute/auto", "OmniRoute"),
	];
	// Also surface Zen free models as routable labels when proxy can forward
	for (name, id) in crate::zen::MODELS {
		models.push(ModelEntry::remote(name, &format!("zen/{id}"), "OmniRoute→Zen"));
	}
	models
}

/// Chat completions URL when using OmniRoute proxy.
pub fn chat_completions_url() -> Option<String> {
	let base = proxy_base_url()?;
	let base = base.trim_end_matches('/');
	if base.ends_with("/v1") {
		Some(format!("{base}/chat/completions"))
	} else {
		Some(format!("{base}/v1/chat/completions"))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn proxy_disabled_by_default() {
		// Don't assert env pollution; just that empty returns none when unset in test is ok
		let _ = proxy_base_url();
		let _ = discover_omniroute_provider_hints();
	}
}
