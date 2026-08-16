//! Unified provider registry: models.dev ∪ Zen ∪ OmniRoute ∪ connected store.

use crate::{
	modes::ModelEntry,
	omniroute,
	providers::{CatalogProvider, ModelsDevCatalog, ProviderStore},
	zen,
};

/// OpenAI-compatible endpoints for providers whose models.dev record may omit
/// `api`. These are only used when the corresponding credential is present.
pub fn compatible_provider_endpoint(id: &str) -> Option<&'static str> {
	match id {
		"cerebras" => Some("https://api.cerebras.ai/v1"),
		"cohere" => Some("https://api.cohere.com/compatibility/v1"),
		"deepseek" => Some("https://api.deepseek.com"),
		"github-models" => Some("https://models.github.ai/inference"),
		"google" => Some("https://generativelanguage.googleapis.com/v1beta/openai"),
		"groq" => Some("https://api.groq.com/openai/v1"),
		"huggingface" => Some("https://router.huggingface.co/v1"),
		"mistral" => Some("https://api.mistral.ai/v1"),
		"openai" => Some("https://api.openai.com/v1"),
		"openrouter" => Some("https://openrouter.ai/api/v1"),
		"sambanova" => Some("https://api.sambanova.ai/v1"),
		"togetherai" => Some("https://api.together.xyz/v1"),
		_ => None,
	}
}

fn compatible_env_names(provider: &CatalogProvider) -> Vec<&str> {
	let fallbacks: &[&str] = match provider.id.as_str() {
		"github-models" => &["GITHUB_MODELS_API_KEY", "GITHUB_TOKEN"],
		"google" => &["GOOGLE_GENERATIVE_AI_API_KEY", "GOOGLE_API_KEY", "GEMINI_API_KEY"],
		"huggingface" => &["HUGGINGFACE_API_KEY", "HF_TOKEN"],
		"replicate" => &["REPLICATE_API_TOKEN"],
		"sambanova" => &["SAMBANOVA_API_KEY"],
		_ => &[],
	};
	let mut names: Vec<&str> = provider.env.iter().map(String::as_str).collect();
	names.extend(fallbacks.iter().copied());
	names
}

fn provider_is_configured(provider: &CatalogProvider) -> bool {
	compatible_env_names(provider)
		.into_iter()
		.any(|name| std::env::var(name).ok().is_some_and(|value| !value.trim().is_empty()))
}

fn chat_completions_url(base: &str) -> String {
	let base = base.trim_end_matches('/');
	if base.ends_with("/chat/completions") {
		base.to_owned()
	} else {
		format!("{base}/chat/completions")
	}
}

/// Resolve the request endpoint for a catalog model only when its provider is
/// configured in this process. The model id is namespaced in the menu, so
/// duplicate ids from different providers cannot select the wrong endpoint.
pub fn model_chat_endpoint(catalog: &ModelsDevCatalog, model_id: &str) -> Option<String> {
	let (provider_id, raw_id) = model_id.split_once('/').unwrap_or(("", model_id));
	let provider = catalog.providers.iter().find(|provider| {
		(provider_id.is_empty() || provider.id == provider_id)
			&& provider.models.iter().any(|model| model.id == raw_id || model.id == model_id)
			&& provider_is_configured(provider)
	})?;
	let base = provider.api.as_deref().or_else(|| compatible_provider_endpoint(&provider.id))?;
	Some(chat_completions_url(base))
}

/// One provider row for menus / doctor.
#[derive(Debug, Clone)]
pub struct ProviderRow {
	pub id: String,
	pub name: String,
	pub source: &'static str,
	pub connected: bool,
	pub model_count: usize,
	pub health: Health,
}

impl ProviderRow {
	/// Menu / doctor label.
	pub fn label(&self) -> String {
		format!("{} {} · {} · {} models", self.health.glyph(), self.name, self.source, self.model_count)
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Health {
	Unknown,
	Ok,
	MissingKey,
}

impl Health {
	pub fn glyph(self) -> &'static str {
		match self {
			Self::Unknown => "·",
			Self::Ok => "✓",
			Self::MissingKey => "!",
		}
	}
}

/// Merge all sources into a de-duplicated provider list.
pub fn list_providers(catalog: &ModelsDevCatalog, store: &ProviderStore) -> Vec<ProviderRow> {
	let mut rows: Vec<ProviderRow> = Vec::new();

	// Zen always present
	rows.push(ProviderRow {
		id: zen::DEFAULT_PROVIDER.to_string(),
		name: "OpenCode Zen".into(),
		source: "zen",
		connected: true,
		model_count: zen::MODELS.len(),
		health: Health::Ok,
	});

	// OmniRoute if configured
	if omniroute::proxy_enabled() {
		let hints = omniroute::discover_omniroute_provider_hints();
		rows.push(ProviderRow {
			id: "omniroute".into(),
			name: "OmniRoute".into(),
			source: "omniroute",
			connected: true,
			model_count: hints.len().max(1),
			health: Health::Ok,
		});
	}

	// Connected store
	for conn in store.enabled() {
		let model_count = catalog.find_provider(&conn.id).map(|p| p.models.len()).unwrap_or(0);
		let health = if conn.api_key_env.as_ref().map(|e| std::env::var(e).is_ok()).unwrap_or(false) {
			Health::Ok
		} else if conn.api_key_env.is_some() {
			Health::MissingKey
		} else {
			// OpenCode Zen and keyless proxies
			Health::Ok
		};
		if !rows.iter().any(|r| r.id == conn.id) {
			rows.push(ProviderRow {
				id: conn.id.clone(),
				name: conn.name.clone(),
				source: "connected",
				connected: true,
				model_count,
				health,
			});
		}
	}

	// models.dev providers (sample top by model count when not already listed)
	let mut catalog_rows: Vec<_> =
		catalog.providers.iter().map(|p| (p.id.clone(), p.name.clone(), p.models.len())).collect();
	catalog_rows.sort_unstable_by_key(|a| std::cmp::Reverse(a.2));
	for (id, name, count) in catalog_rows.into_iter().take(80) {
		if rows.iter().any(|r| r.id == id) {
			continue;
		}
		rows.push(ProviderRow {
			id,
			name,
			source: "models.dev",
			connected: false,
			model_count: count,
			health: Health::Unknown,
		});
	}

	rows
}

/// Build the remote model menu: Zen + OmniRoute + connected catalog models.
#[allow(dead_code)]
pub fn unified_remote_models(catalog: &ModelsDevCatalog, store: &ProviderStore) -> Vec<ModelEntry> {
	let mut models = crate::modes::remote_models();
	for entry in omniroute::omniroute_model_entries() {
		if !models.iter().any(|m| m.model_id == entry.model_id) {
			models.push(entry);
		}
	}
	for conn in store.enabled() {
		if let Some(p) = catalog.find_provider(&conn.id) {
			for m in p.models.iter().take(40) {
				let id = m.id.clone();
				if models.iter().any(|x| x.model_id == id) {
					continue;
				}
				models.push(ModelEntry::remote(&m.name, &id, &p.name));
			}
		}
	}
	// When OmniRoute proxy is on, also expose a few top catalog models as routable
	if omniroute::proxy_enabled() {
		for p in catalog.providers.iter().take(12) {
			for m in p.models.iter().take(3) {
				let id = format!("route/{}/{}", p.id, m.id);
				if models.iter().any(|x| x.model_id == id) {
					continue;
				}
				models.push(ModelEntry::remote(&format!("{} (via OmniRoute)", m.name), &id, "OmniRoute"));
			}
		}
	}
	models
}

/// Production model catalog: **Flow (real GGUFs) → Zen (verified free) → authenticated providers**.
///
/// Used by key `0` / model picker. Flow never includes STT/TTS/Vosk/wake-word assets.
pub fn build_production_model_menu(
	catalog: &ModelsDevCatalog,
	store: &ProviderStore,
) -> Vec<ModelEntry> {
	use crate::modes::model_menu;

	let mut out: Vec<ModelEntry> = Vec::new();

	// ── Flow: only chat LLM .gguf under models/ + models/llm/ ────────────
	out.push(ModelEntry::section("Flow · local GGUF"));
	out.push(ModelEntry::action(
		model_menu::ACT_REFRESH_FLOW,
		"Refresh local (Flow)",
		"scan models/llm",
	));
	let local = crate::flow_backend::discover_local_models();
	for m in local {
		if m.is_local && m.available {
			out.push(m);
		}
	}

	// ── OpenCode Zen free (verified allowlist) ────────────────────────────
	out.push(ModelEntry::section("OpenCode Zen · free"));
	for m in crate::modes::remote_models() {
		out.push(m);
	}

	// ── OmniRoute (optional) ─────────────────────────────────────────────
	let omni = omniroute::omniroute_model_entries();
	if !omni.is_empty() {
		out.push(ModelEntry::section("OmniRoute"));
		for m in omni {
			out.push(m);
		}
	}

	// ── Connected providers (user-configured) ────────────────────────────
	let connected: Vec<_> = store.enabled().collect();
	if !connected.is_empty() {
		// out.push(ModelEntry::section("Connected"));
		for conn in connected {
			if conn.id == "opencode-zen" || conn.name.eq_ignore_ascii_case("OpenCode Zen") {
				continue;
			}
			if let Some(p) = catalog.find_provider(&conn.id) {
				for m in &p.models {
					let model_id = format!("{}/{}", p.id, m.id);
					if out
						.iter()
						.any(|x| x.model_id == m.id || x.model_id == model_id)
					{
						continue;
					}
					out.push(ModelEntry::remote(&m.name, &model_id, &p.name));
				}
			} else if let Some(mid) = conn.default_model.as_ref()
				&& !out.iter().any(|x| x.model_id == *mid)
			{
				out.push(ModelEntry::remote(mid, mid, &conn.name));
			}
		}
	}

	// ── Environment-backed providers ─────────────────────────────────────
	// A key declared by models.dev is enough to make a provider usable: the
	// request layer resolves the actual secret from the named environment
	// variable. Never copy the value into the catalog or persistence layer.
	// This makes providers such as OpenRouter, Groq, DeepSeek, and Together
	// visible without requiring a second manual connection entry.
	for provider in &catalog.providers {
		if !provider_is_configured(provider) {
			continue;
		}
		// A key without a known endpoint would silently route to Zen and produce
		// misleading provider/auth errors. Only expose routable providers.
		if provider.api.is_none() && compatible_provider_endpoint(&provider.id).is_none() {
			continue;
		}
		if connected.iter().any(|conn| conn.id == provider.id) {
			continue;
		}
		for model in &provider.models {
			let model_id = format!("{}/{}", provider.id, model.id);
			if out
				.iter()
				.any(|entry| entry.model_id == model.id || entry.model_id == model_id)
			{
				continue;
			}
			out.push(ModelEntry::remote(&model.name, &model_id, &provider.name));
		}
	}

	out.push(ModelEntry::action(model_menu::ACT_CONNECT, "Connect provider…", "models.dev"));

	out
}

/// Short doctor line for `/status`.
pub fn doctor_summary(catalog: &ModelsDevCatalog, store: &ProviderStore) -> String {
	let rows = list_providers(catalog, store);
	let connected = rows.iter().filter(|r| r.connected).count();
	let missing_key = rows.iter().filter(|r| r.health == Health::MissingKey).count();
	format!(
		"providers: {} listed · {connected} connected · {missing_key} missing key · models.dev {} models · omni {}",
		rows.len(),
		catalog.model_count(),
		if omniroute::proxy_enabled() { "on" } else { "off" }
	)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn zen_always_listed() {
		let cat = ModelsDevCatalog::default();
		let store = ProviderStore::default();
		let rows = list_providers(&cat, &store);
		assert!(rows.iter().any(|r| r.source == "zen"));
	}

	#[test]
	fn production_menu_orders_flow_then_zen_then_connect() {
		let cat = ModelsDevCatalog::default();
		let store = ProviderStore::default();
		let menu = build_production_model_menu(&cat, &store);
		assert!(menu.len() >= 5, "menu too thin: {}", menu.len());

		let flow_sec = menu
			.iter()
			.position(|m| m.is_section() && m.display_name.contains("Flow"))
			.expect("Flow section");
		let zen_sec = menu
			.iter()
			.position(|m| m.is_section() && m.display_name.contains("OpenCode Zen"))
			.expect("Zen section");
		let connect = menu
			.iter()
			.position(|m| m.model_id == crate::modes::model_menu::ACT_CONNECT)
			.expect("Connect action");
		assert!(flow_sec < zen_sec, "Flow must come before Zen");
		assert!(zen_sec < connect, "Zen must come before Connect action");

		// Every currently advertised free Zen model is present
		for (_, id) in crate::zen::MODELS {
			assert!(menu.iter().any(|m| m.model_id == *id), "missing zen model {id}");
		}
	}
}
