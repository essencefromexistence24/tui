//! models.dev catalog loader (OpenCode-compatible).

use std::{
	fs,
	path::PathBuf,
	time::{Duration, SystemTime},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

const MODELS_DEV_URL: &str = "https://models.dev/api.json";
const CACHE_TTL: Duration = Duration::from_secs(60 * 60 * 6); // 6h

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogModel {
	pub id: String,
	pub name: String,
	#[serde(default)]
	pub family: Option<String>,
	#[serde(default)]
	pub reasoning: bool,
	#[serde(default)]
	pub tool_call: bool,
	#[serde(default)]
	pub context: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogProvider {
	pub id: String,
	pub name: String,
	#[serde(default)]
	pub env: Vec<String>,
	#[serde(default)]
	pub api: Option<String>,
	#[serde(default)]
	pub models: Vec<CatalogModel>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelsDevCatalog {
	pub providers: Vec<CatalogProvider>,
	pub fetched_at_unix: u64,
}

impl ModelsDevCatalog {
	pub fn provider_count(&self) -> usize {
		self.providers.len()
	}

	pub fn model_count(&self) -> usize {
		self.providers.iter().map(|p| p.models.len()).sum()
	}

	pub fn find_provider(&self, id_or_name: &str) -> Option<&CatalogProvider> {
		let q = id_or_name.to_ascii_lowercase();
		self
			.providers
			.iter()
			.find(|p| p.id.eq_ignore_ascii_case(&q) || p.name.to_ascii_lowercase().contains(&q))
	}

	/// Flat model entries for menus: (display, model_id, provider_id, provider_name)
	pub fn flat_models(&self) -> Vec<(String, String, String, String)> {
		let mut out = Vec::new();
		for p in &self.providers {
			for m in &p.models {
				out.push((format!("{} · {}", m.name, p.name), m.id.clone(), p.id.clone(), p.name.clone()));
			}
		}
		out
	}
}

fn cache_path() -> PathBuf {
	let base =
		dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".config").join("dx").join("cache");
	let _ = fs::create_dir_all(&base);
	base.join("models-dev.json")
}

pub fn load_cached_catalog() -> Option<ModelsDevCatalog> {
	let path = cache_path();
	let data = fs::read_to_string(path).ok()?;
	serde_json::from_str(&data).ok()
}

pub fn cache_is_fresh(cat: &ModelsDevCatalog) -> bool {
	let now =
		SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
	now.saturating_sub(cat.fetched_at_unix) < CACHE_TTL.as_secs()
}

/// Parse models.dev API JSON (object of provider_id → provider).
fn parse_models_dev_json(text: &str) -> Result<ModelsDevCatalog> {
	#[derive(Deserialize)]
	struct RawModel {
		id: Option<String>,
		name: Option<String>,
		family: Option<String>,
		#[serde(default)]
		reasoning: bool,
		#[serde(default)]
		tool_call: bool,
		limit: Option<RawLimit>,
	}
	#[derive(Deserialize)]
	struct RawLimit {
		context: Option<u64>,
	}
	#[derive(Deserialize)]
	struct RawProvider {
		id: Option<String>,
		name: Option<String>,
		#[serde(default)]
		env: Vec<String>,
		api: Option<String>,
		#[serde(default)]
		models: serde_json::Map<String, serde_json::Value>,
	}

	let map: serde_json::Map<String, serde_json::Value> =
		serde_json::from_str(text).context("parse models.dev JSON")?;
	let mut providers = Vec::new();
	for (key, val) in map {
		let raw: RawProvider = match serde_json::from_value(val) {
			Ok(r) => r,
			Err(_) => continue,
		};
		let id = raw.id.unwrap_or(key);
		let name = raw.name.unwrap_or_else(|| id.clone());
		let mut models = Vec::new();
		for (mid, mval) in raw.models {
			let m: RawModel = match serde_json::from_value(mval) {
				Ok(m) => m,
				Err(_) => continue,
			};
			let model_id = m.id.unwrap_or(mid);
			let model_name = m.name.unwrap_or_else(|| model_id.clone());
			models.push(CatalogModel {
				id: model_id,
				name: model_name,
				family: m.family,
				reasoning: m.reasoning,
				tool_call: m.tool_call,
				context: m.limit.and_then(|l| l.context),
			});
		}
		models.sort_by(|a, b| a.name.cmp(&b.name));
		providers.push(CatalogProvider { id, name, env: raw.env, api: raw.api, models });
	}
	providers.sort_by(|a, b| a.name.cmp(&b.name));
	let fetched_at_unix =
		SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
	Ok(ModelsDevCatalog { providers, fetched_at_unix })
}

/// Refresh from network (blocking HTTP). Falls back to cache on failure.
pub fn refresh_catalog() -> Result<ModelsDevCatalog> {
	let client = reqwest::blocking::Client::builder()
		.timeout(Duration::from_secs(20))
		.user_agent("dx-tui/models-dev")
		.build()
		.context("build HTTP client")?;
	let text = client
		.get(MODELS_DEV_URL)
		.send()
		.context("fetch models.dev")?
		.error_for_status()
		.context("models.dev HTTP status")?
		.text()
		.context("read models.dev body")?;
	let catalog = parse_models_dev_json(&text)?;
	if let Ok(json) = serde_json::to_string_pretty(&catalog) {
		let _ = fs::write(cache_path(), json);
	}
	Ok(catalog)
}

/// Load catalog: fresh cache, else network, else empty + embedded zen fallback.
pub fn load_or_refresh_catalog() -> ModelsDevCatalog {
	if let Some(c) = load_cached_catalog()
		&& cache_is_fresh(&c)
		&& c.provider_count() > 0
	{
		return c;
	}
	match refresh_catalog() {
		Ok(c) => c,
		Err(e) => {
			tracing::warn!("models.dev refresh failed: {e}");
			load_cached_catalog().unwrap_or_else(embedded_fallback_catalog)
		}
	}
}

fn embedded_fallback_catalog() -> ModelsDevCatalog {
	// Minimal always-available OpenCode Zen slice so the TUI works offline.
	let models = crate::zen::MODELS
		.iter()
		.map(|(name, id)| CatalogModel {
			id: (*id).to_string(),
			name: (*name).to_string(),
			family: None,
			reasoning: false,
			tool_call: true,
			context: Some(128_000),
		})
		.collect();
	ModelsDevCatalog {
		providers: vec![CatalogProvider {
			id: "opencode-zen".into(),
			name: "OpenCode Zen".into(),
			env: vec![],
			api: Some(crate::zen::ZEN_URL.trim_end_matches("/chat/completions").into()),
			models,
		}],
		fetched_at_unix: 0,
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn parse_minimal_catalog() {
		let json = r#"{
			"openai": {
				"id": "openai",
				"name": "OpenAI",
				"env": ["OPENAI_API_KEY"],
				"models": {
					"gpt-4o": { "id": "gpt-4o", "name": "GPT-4o", "limit": { "context": 128000 } }
				}
			}
		}"#;
		let cat = parse_models_dev_json(json).expect("parse");
		assert_eq!(cat.provider_count(), 1);
		assert_eq!(cat.model_count(), 1);
		assert_eq!(cat.providers[0].models[0].id, "gpt-4o");
	}
}
