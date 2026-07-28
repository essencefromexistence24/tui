//! Unified provider registry: models.dev ∪ Zen ∪ OmniRoute ∪ connected store.

use crate::{
	modes::ModelEntry,
	omniroute,
	providers::{ModelsDevCatalog, ProviderStore},
	zen,
};

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

/// Production model catalog: **Flow (real GGUFs) → Zen (6 free) → models.dev (75+ providers)**.
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
		if m.is_local {
			out.push(m);
		}
	}

	// ── OpenCode Zen free (exactly 6) ────────────────────────────────────
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
				for m in p.models.iter().take(12) {
					if out.iter().any(|x| x.model_id == m.id) {
						continue;
					}
					out.push(ModelEntry::remote(&m.name, &m.id, &p.name));
				}
			} else if let Some(mid) = conn.default_model.as_ref()
				&& !out.iter().any(|x| x.model_id == *mid)
			{
				out.push(ModelEntry::remote(mid, mid, &conn.name));
			}
		}
	}

	// ── models.dev: 75+ providers (real remote catalog) ──────────────────
	if catalog.provider_count() > 0 {
		// out.push(ModelEntry::section(format!(
		// 	"models.dev · {} providers",
		// 	catalog.provider_count()
		// ).as_str()));
		// All providers; a few models each so the menu stays usable
		let mut providers: Vec<_> = catalog.providers.iter().collect();
		providers.sort_by_key(|a| a.name.to_ascii_lowercase());
		const PER_PROVIDER: usize = 3;
		const MAX_CATALOG: usize = 240;
		let mut added = 0usize;
		for p in providers {
			if p.name.to_ascii_lowercase().contains("opencode zen") {
				continue;
			}
			for m in p.models.iter().take(PER_PROVIDER) {
				if out.iter().any(|x| x.model_id == m.id) {
					continue;
				}
				out.push(ModelEntry::remote(&m.name, &m.id, &p.name));
				added += 1;
				if added >= MAX_CATALOG {
					break;
				}
			}
			if added >= MAX_CATALOG {
				break;
			}
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
		assert!(menu.len() >= 8, "menu too thin: {}", menu.len());

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

		// All six free Zen models present
		for (_, id) in crate::zen::MODELS {
			assert!(menu.iter().any(|m| m.model_id == *id), "missing zen model {id}");
		}
	}
}
