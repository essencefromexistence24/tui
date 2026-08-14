//! Chat agent mode and runtime selection (bottom bar).

use serde::{Deserialize, Serialize};

/// Reasoning effort tiers for models that support configurable thinking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ReasoningEffort {
	Low,
	#[default]
	Medium,
	High,
	XHigh,
}

impl ReasoningEffort {
	pub const ALL: [Self; 4] = [Self::Low, Self::Medium, Self::High, Self::XHigh];

	pub fn label(self) -> &'static str {
		match self {
			Self::Low => "Low",
			Self::Medium => "Medium",
			Self::High => "High",
			Self::XHigh => "Ultra",
		}
	}

	pub fn next(self) -> Self {
		match self {
			Self::Low => Self::Medium,
			Self::Medium => Self::High,
			Self::High => Self::XHigh,
			Self::XHigh => Self::Low,
		}
	}

	pub fn cycle(&mut self) {
		*self = self.next();
	}

	pub fn toggle(self) -> Self {
		match self {
			Self::Low | Self::Medium => Self::High,
			Self::High | Self::XHigh => Self::Low,
		}
	}

	/// API value passed to reasoning_effort parameter.
	pub fn api_value(self) -> &'static str {
		match self {
			Self::Low => "low",
			Self::Medium => "medium",
			Self::High => "high",
			Self::XHigh => "xhigh",
		}
	}

	/// Color hint for bottom-bar indicator.
	pub fn color(self) -> ratatui::style::Color {
		use ratatui::style::Color;
		match self {
			Self::Low => Color::Rgb(0x88, 0x88, 0x88),
			Self::Medium => Color::Rgb(0x66, 0xbb, 0x66),
			Self::High => Color::Rgb(0xff, 0xaa, 0x00),
			Self::XHigh => Color::Rgb(0xff, 0x44, 0x44),
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AgentMode {
	/// Read-only Q&A — default.
	#[default]
	Ask,
	/// Allow file edits and tool writes.
	Write,
	/// Planning / design without execution.
	Plan,
	/// Goal-driven multi-step runs.
	Goal,
	/// Full dx-agent tool-using profile (channels, tools, multi-step).
	Agent,
	/// Multi-query: send multiple prompts to different models concurrently.
	Multi,
	/// Automation: run prompts on a timer/interval or daily schedule.
	Automation,
	/// Codex CLI app-server backend.
	Codex,
}

impl AgentMode {
	pub const ALL: [Self; 8] = [
		Self::Ask,
		Self::Write,
		Self::Plan,
		Self::Goal,
		Self::Agent,
		Self::Multi,
		Self::Automation,
		Self::Codex,
	];

	pub fn label(self) -> &'static str {
		match self {
			Self::Ask => "Ask",
			Self::Write => "Write",
			Self::Plan => "Plan",
			Self::Goal => "Goal",
			Self::Agent => "Agent",
			Self::Multi => "Multi",
			Self::Automation => "Automation",
			Self::Codex => "Codex",
		}
	}

	pub fn next(self) -> Self {
		match self {
			Self::Ask => Self::Write,
			Self::Write => Self::Plan,
			Self::Plan => Self::Goal,
			Self::Goal => Self::Agent,
			Self::Agent => Self::Multi,
			Self::Multi => Self::Automation,
			Self::Automation => Self::Codex,
			Self::Codex => Self::Ask,
		}
	}

	pub fn from_index(i: usize) -> Self {
		Self::ALL[i % Self::ALL.len()]
	}

	/// Whether this mode should use the dx-agent backend.
	pub fn prefers_dx_agent(self) -> bool {
		matches!(self, Self::Agent)
	}

	/// Whether this mode uses the codex-rs app-server backend.
	pub fn prefers_codex(self) -> bool {
		matches!(self, Self::Codex)
	}

	/// Distinct text colors per profile (bottom bar + headers).
	pub fn color(self, _theme: &crate::theme::ChatTheme) -> ratatui::style::Color {
		use ratatui::style::Color;
		match self {
			Self::Ask => Color::Rgb(0x26, 0x71, 0xf4),        // blue
			Self::Write => Color::Rgb(0x22, 0xc5, 0x5e),      // green — edits
			Self::Plan => Color::Rgb(0x06, 0xb6, 0xd4),       // cyan — design
			Self::Goal => Color::Rgb(0xff, 0xae, 0x04),       // amber
			Self::Agent => Color::Rgb(0xa8, 0x55, 0xf7),      // purple
			Self::Multi => Color::Rgb(0xff, 0x6b, 0x35),      // bright orange
			Self::Automation => Color::Rgb(0xff, 0x2d, 0x7b), // hot pink
			Self::Codex => Color::Rgb(0x00, 0xbc, 0x8d),      // teal — codex
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RuntimeMode {
	/// Local models via dx-flow.
	Local,
	/// Remote providers (OpenCode Zen / dx-agent providers).
	#[default]
	Remote,
}

impl RuntimeMode {
	pub fn label(self) -> &'static str {
		match self {
			Self::Local => "Local",
			Self::Remote => "Remote",
		}
	}

	pub fn toggle(self) -> Self {
		match self {
			Self::Local => Self::Remote,
			Self::Remote => Self::Local,
		}
	}
}

/// A selectable model entry for the AI models menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelEntry {
	pub display_name: String,
	pub model_id: String,
	pub provider: String,
	pub is_local: bool,
	/// True when the GGUF / runtime path is present (local) or always true (remote).
	pub available: bool,
	/// Whether this model supports reasoning_effort parameter.
	pub reasoning_capable: bool,
}

/// Magic ids for section headers / menu actions in the unified model popup.
pub mod model_menu {
	pub const SEC_PREFIX: &str = "__sec:";
	pub const ACT_CONNECT: &str = "__act:connect";
	pub const ACT_REFRESH_FLOW: &str = "__act:refresh_flow";
	/// Optional full-drive GGUF scan (C–Z). Slow; use when models already exist but you want more.
	pub const ACT_SCAN_ALL_DRIVES: &str = "__act:scan_all_drives";
	pub const ACT_RUNTIME_LOCAL: &str = "__act:runtime_local";
	pub const ACT_RUNTIME_REMOTE: &str = "__act:runtime_remote";
}

impl ModelEntry {
	pub fn remote(display: &str, id: &str, provider: &str) -> Self {
		Self {
			display_name: display.to_string(),
			model_id: id.to_string(),
			provider: provider.to_string(),
			is_local: false,
			available: true,
			reasoning_capable: false,
		}
	}

	pub fn local(display: &str, id: &str, available: bool) -> Self {
		Self {
			display_name: display.to_string(),
			model_id: id.to_string(),
			provider: "dx-flow".to_string(),
			is_local: true,
			available,
			reasoning_capable: false,
		}
	}

	pub fn section(title: &str) -> Self {
		Self {
			display_name: format!("── {title} ──"),
			model_id: format!("{}{title}", model_menu::SEC_PREFIX),
			provider: String::new(),
			is_local: false,
			available: false,
			reasoning_capable: false,
		}
	}

	pub fn action(id: &str, label: &str, provider_hint: &str) -> Self {
		Self {
			display_name: label.to_string(),
			model_id: id.to_string(),
			provider: provider_hint.to_string(),
			is_local: false,
			available: true,
			reasoning_capable: false,
		}
	}

	pub fn is_section(&self) -> bool {
		self.model_id.starts_with(model_menu::SEC_PREFIX)
	}

	pub fn is_action(&self) -> bool {
		self.model_id.starts_with("__act:")
	}

	pub fn is_selectable_model(&self) -> bool {
		!self.is_section() && !self.is_action()
	}

	pub fn status_label(&self) -> &str {
		if self.is_section() {
			""
		} else if self.is_action() {
			self.provider.as_str()
		} else if self.available {
			"ready"
		} else {
			"missing"
		}
	}

	/// Compact source badge used by the model picker.
	///
	/// The badge is derived from the actual source. Local weights are
	/// `dx-flow`; XAI/Grok entries are `xai`; other providers keep their name.
	pub fn provider_badge(&self) -> String {
		if self.is_local {
			return "dx-flow".to_string();
		}
		let provider = self.provider.trim();
		let normalized = provider.to_ascii_lowercase();
		if normalized == "xai"
			|| normalized == "x.ai"
			|| normalized.contains("xai")
			|| normalized.contains("grok")
		{
			"xai".to_string()
		} else if normalized.contains("opencode zen") {
			"zen".to_string()
		} else {
			provider.to_string()
		}
	}
}

/// Catalog of remote OpenCode Zen free models (6). Default is Big Pickle.
pub fn remote_models() -> Vec<ModelEntry> {
	crate::zen::MODELS
		.iter()
		.map(|(name, id)| ModelEntry::remote(name, id, crate::zen::DEFAULT_PROVIDER))
		.collect()
}

/// Catalog of local models from **dx-flow only** (no drive scanning in TUI).
#[allow(dead_code)]
pub fn local_models() -> Vec<ModelEntry> {
	crate::flow_backend::discover_local_models()
}

/// Default selection at startup: Remote + Big Pickle + OpenCode Zen.
#[allow(dead_code)]
pub fn default_remote_selection() -> ModelEntry {
	ModelEntry::remote(
		crate::zen::DEFAULT_MODEL_DISPLAY,
		crate::zen::DEFAULT_MODEL,
		crate::zen::DEFAULT_PROVIDER,
	)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn local_model_badge_is_dx_flow() {
		let model = ModelEntry::local("MiniCPM", "minicpm", true);
		assert_eq!(model.provider_badge(), "dx-flow");
	}

	#[test]
	fn xai_model_badge_is_xai() {
		let model = ModelEntry::remote("Grok", "grok-4", "xAI");
		assert_eq!(model.provider_badge(), "xai");
	}

	#[test]
	fn zen_model_badge_is_zen() {
		let model = ModelEntry::remote("Big Pickle", "big-pickle", "OpenCode Zen");
		assert_eq!(model.provider_badge(), "zen");
	}
}
