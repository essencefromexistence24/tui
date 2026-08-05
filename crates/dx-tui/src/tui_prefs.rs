//! Persist lightweight TUI preferences (`~/.config/dx/tui.toml`).

use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::modes::{AgentMode, ReasoningEffort, RuntimeMode};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuiPrefs {
	#[serde(default = "default_agent_mode")]
	pub agent_mode: String,
	#[serde(default = "default_runtime")]
	pub runtime_mode: String,
	#[serde(default)]
	pub selected_model: Option<String>,
	#[serde(default = "default_true")]
	pub auto_compact: bool,
	#[serde(default = "default_true")]
	pub token_save: bool,
	#[serde(default = "default_true")]
	pub show_sidebar: bool,
	#[serde(default)]
	pub last_session_id: Option<String>,
	/// Display name in chat bubbles (default "You").
	#[serde(default = "default_user_name")]
	pub user_name: String,
	/// Reasoning effort preference.
	#[serde(default)]
	pub reasoning_effort: String,
}

fn default_user_name() -> String {
	"You".into()
}

fn default_agent_mode() -> String {
	"Ask".into()
}
fn default_runtime() -> String {
	// "Local".into()
	"Remote".into()
}
fn default_true() -> bool {
	true
}

impl Default for TuiPrefs {
	fn default() -> Self {
		Self {
			agent_mode: default_agent_mode(),
			runtime_mode: default_runtime(),
			// selected_model: Some("minicpm5-1b-tooluse".to_string()),
			selected_model: Some(crate::zen::DEFAULT_MODEL.to_string()),
			auto_compact: true,
			token_save: true,
			show_sidebar: true,
			last_session_id: None,
			user_name: default_user_name(),
			reasoning_effort: String::new(),
		}
	}
}

impl TuiPrefs {
	pub fn agent_mode_enum(&self) -> AgentMode {
		match self.agent_mode.to_ascii_lowercase().as_str() {
			"write" => AgentMode::Write,
			"plan" => AgentMode::Plan,
			"goal" => AgentMode::Goal,
			"agent" => AgentMode::Agent,
			"multi" => AgentMode::Multi,
			"automation" => AgentMode::Automation,
			_ => AgentMode::Ask,
		}
	}

	pub fn runtime_mode_enum(&self) -> RuntimeMode {
		if self.runtime_mode.eq_ignore_ascii_case("local") {
			RuntimeMode::Local
		} else {
			RuntimeMode::Remote
		}
	}

	pub fn reasoning_effort_enum(&self) -> ReasoningEffort {
		match self.reasoning_effort.to_ascii_lowercase().as_str() {
			"low" => ReasoningEffort::Low,
			"high" => ReasoningEffort::High,
			"xhigh" => ReasoningEffort::XHigh,
			_ => ReasoningEffort::Medium,
		}
	}
}

fn prefs_path() -> PathBuf {
	dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("dx").join("tui.toml")
}

pub fn load() -> TuiPrefs {
	let path = prefs_path();
	let Ok(text) = fs::read_to_string(&path) else {
		return TuiPrefs::default();
	};
	toml::from_str(&text).unwrap_or_default()
}

pub fn save(prefs: &TuiPrefs) -> Result<()> {
	let path = prefs_path();
	if let Some(parent) = path.parent() {
		fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
	}
	let text = toml::to_string_pretty(prefs).context("serialize tui prefs")?;
	fs::write(&path, text).with_context(|| format!("write {}", path.display()))?;
	Ok(())
}

pub fn path_display() -> String {
	prefs_path().display().to_string()
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn default_modes() {
		let p = TuiPrefs::default();
		assert_eq!(p.agent_mode_enum(), AgentMode::Ask);
		assert_eq!(p.runtime_mode_enum(), RuntimeMode::Remote);
	}
}
