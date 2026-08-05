//! Connected provider store (~/.config/dx/providers.toml).

use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
	#[default]
	OpenAiCompatible,
	Anthropic,
	Google,
	Azure,
	Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectedProvider {
	pub id: String,
	pub name: String,
	pub kind: ProviderKind,
	/// Base URL for OpenAI-compatible endpoints.
	#[serde(default)]
	pub base_url: Option<String>,
	/// Env var name holding the API key (never store the key itself).
	#[serde(default)]
	pub api_key_env: Option<String>,
	#[serde(default)]
	pub default_model: Option<String>,
	#[serde(default)]
	pub enabled: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProviderStore {
	#[serde(default)]
	pub providers: Vec<ConnectedProvider>,
}

fn store_path() -> PathBuf {
	let base = dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".config").join("dx");
	let _ = fs::create_dir_all(&base);
	base.join("providers.toml")
}

pub fn load_provider_store() -> ProviderStore {
	let path = store_path();
	let Ok(text) = fs::read_to_string(path) else {
		return default_store();
	};
	toml::from_str(&text).unwrap_or_else(|_| default_store())
}

fn default_store() -> ProviderStore {
	ProviderStore {
		providers: vec![ConnectedProvider {
			id: "opencode-zen".into(),
			name: "OpenCode Zen".into(),
			kind: ProviderKind::OpenAiCompatible,
			base_url: Some("https://opencode.ai/zen/v1".into()),
			api_key_env: None,
			default_model: Some(crate::zen::DEFAULT_MODEL.into()),
			enabled: true,
		}],
	}
}

pub fn save_provider_store(store: &ProviderStore) -> Result<()> {
	let text = toml::to_string_pretty(store).context("serialize providers")?;
	fs::write(store_path(), text).context("write providers.toml")?;
	Ok(())
}

impl ProviderStore {
	pub fn upsert(&mut self, provider: ConnectedProvider) {
		if let Some(slot) = self.providers.iter_mut().find(|p| p.id == provider.id) {
			*slot = provider;
		} else {
			self.providers.push(provider);
		}
	}

	pub fn enabled(&self) -> impl Iterator<Item = &ConnectedProvider> {
		self.providers.iter().filter(|p| p.enabled)
	}

	pub fn connect_from_catalog(
		&mut self,
		catalog_id: &str,
		catalog_name: &str,
		api: Option<String>,
		env: &[String],
		default_model: Option<String>,
	) {
		self.upsert(ConnectedProvider {
			id: catalog_id.to_string(),
			name: catalog_name.to_string(),
			kind: ProviderKind::OpenAiCompatible,
			base_url: api,
			api_key_env: env.first().cloned(),
			default_model,
			enabled: true,
		});
	}
}
