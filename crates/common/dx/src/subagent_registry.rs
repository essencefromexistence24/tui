//! Custom subagent registry — loads user-defined subagent types from
//! `~/.config/dx/subagents.toml` and makes them available to the task tool.
//!
//! Format:
//! ```toml
//! [subagents.my-custom-agent]
//! description = "Description shown to the model"
//! system_prompt = "You are a specialist subagent that does X..."
//! model = "custom-model-id"          # optional, defaults to parent
//! max_steps = 12                      # optional, defaults to 8
//! timeout_secs = 600                  # optional, defaults to 300
//! allow_tools = ["read", "grep"]     # optional, defaults to all
//! ```

use std::{collections::HashMap, fs, path::PathBuf};

use serde::Deserialize;

use crate::orchestration::SubagentConfig;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SubagentTomlConfig {
	#[serde(default)]
	pub subagents: HashMap<String, CustomSubagentDef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CustomSubagentDef {
	pub description: String,
	pub system_prompt: String,
	#[serde(default)]
	pub model: Option<String>,
	#[serde(default = "default_max_steps")]
	pub max_steps: u32,
	#[serde(default = "default_timeout")]
	pub timeout_secs: u64,
	/// If empty, all tools are allowed (subject to mode policy).
	#[serde(default)]
	pub allow_tools: Vec<String>,
}

fn default_max_steps() -> u32 {
	8
}
fn default_timeout() -> u64 {
	300
}

fn config_path() -> PathBuf {
	dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("dx").join("subagents.toml")
}

/// Load custom subagent definitions from disk.
pub fn load_custom_subagents() -> HashMap<String, SubagentConfig> {
	let path = config_path();
	let text = match fs::read_to_string(&path) {
		Ok(t) => t,
		Err(_) => return HashMap::new(),
	};
	let parsed: SubagentTomlConfig = match toml::from_str(&text) {
		Ok(p) => p,
		Err(e) => {
			tracing::warn!(?e, "failed to parse subagents.toml");
			return HashMap::new();
		}
	};

	let mut out = HashMap::new();
	for (name, def) in parsed.subagents {
		let allowlist = if def.allow_tools.is_empty() {
			None
		} else {
			Some(def.allow_tools.iter().filter_map(|t| crate::tools::ToolKind::from_name(t)).collect())
		};
		out.insert(
			name.clone(),
			SubagentConfig {
				name,
				description: def.description,
				system_prompt: def.system_prompt,
				model: def.model,
				max_steps: def.max_steps.clamp(1, 50),
				timeout_secs: def.timeout_secs.clamp(30, 3600),
				allowlist,
			},
		);
	}
	out
}

/// Resolve a subagent type name to a config, checking custom registry first.
pub fn resolve_subagent(
	name: &str,
	custom: &HashMap<String, SubagentConfig>,
) -> Option<SubagentConfig> {
	// Check custom registry first
	if let Some(c) = custom.get(name) {
		return Some(c.clone());
	}
	// Fall back to built-in types
	crate::orchestration::SubagentType::from_str(name)
		.map(crate::orchestration::SubagentConfig::builtin)
}

/// Return a description line for all available subagents (built-in + custom).
#[allow(dead_code)]
pub fn available_subagents_description(custom: &HashMap<String, SubagentConfig>) -> String {
	let mut lines = Vec::new();
	for st in crate::orchestration::SUBTYPES {
		lines.push(format!("{} — {}", st.name(), st.description()));
	}
	for (name, cfg) in custom {
		lines.push(format!("{name} — {}", cfg.description));
	}
	lines.join("\n")
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn resolve_builtin_types() {
		let custom = HashMap::new();
		assert!(resolve_subagent("explore", &custom).is_some());
		assert!(resolve_subagent("general-purpose", &custom).is_some());
		assert!(resolve_subagent("orchestrator", &custom).is_some());
	}

	#[test]
	fn resolve_custom_overrides_builtin() {
		let mut custom = HashMap::new();
		custom.insert(
			"explore".to_string(),
			SubagentConfig {
				name: "explore".into(),
				description: "custom explore".into(),
				system_prompt: "custom".into(),
				model: None,
				max_steps: 10,
				timeout_secs: 100,
				allowlist: None,
			},
		);
		let resolved = resolve_subagent("explore", &custom);
		assert!(resolved.is_some());
		assert_eq!(resolved.unwrap().max_steps, 10);
	}

	#[test]
	fn unknown_type_returns_none() {
		let custom = HashMap::new();
		assert!(resolve_subagent("nonexistent-agent", &custom).is_none());
	}
}
