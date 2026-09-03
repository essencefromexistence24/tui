//! dx-agent channel inventory and connection status for the Channels menu.

use std::{
	fs,
	path::{Path, PathBuf},
};

/// One messaging / ingress channel known to dx-agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelEntry {
	pub name: String,
	pub type_key: String,
	/// Compiled into the agent binary (when known).
	pub compiled: bool,
	/// Appears configured in local config.toml.
	pub configured: bool,
	/// Best-effort "connected / running" probe.
	pub connected: bool,
	pub description: String,
}

impl ChannelEntry {
	pub fn status_label(&self) -> &'static str {
		if self.connected {
			"connected"
		} else if self.configured {
			"configured"
		} else if self.compiled {
			"available"
		} else {
			"not compiled"
		}
	}

	pub fn status_glyph(&self) -> &'static str {
		if self.connected {
			"●"
		} else if self.configured {
			"◐"
		} else if self.compiled {
			"○"
		} else {
			"·"
		}
	}
}

/// Canonical channel inventory (mirrors dx-agent-channels listing names).
const CHANNEL_SPECS: &[(&str, &str, &str)] = &[
	("Telegram", "telegram", "Telegram Bot API"),
	("Discord", "discord", "Discord bot gateway"),
	("Slack", "slack", "Slack app / socket mode"),
	("Mattermost", "mattermost", "Mattermost webhook / bot"),
	("iMessage", "imessage", "macOS iMessage bridge"),
	("Matrix", "matrix", "Matrix homeserver client"),
	("Signal", "signal", "signal-cli bridge"),
	("WhatsApp", "whatsapp", "WhatsApp Cloud API"),
	("WhatsApp Web", "whatsapp-web", "WhatsApp Web session"),
	("Email", "email", "IMAP / SMTP email"),
	("Gmail Push", "gmail-push", "Gmail push notifications"),
	("IRC", "irc", "IRC network"),
	("Lark", "lark", "Lark / Feishu"),
	("DingTalk", "dingtalk", "DingTalk bot"),
	("WeCom", "wecom", "WeCom / WeChat Work"),
	("Webhook", "webhook", "HTTP webhook ingress"),
	("ACP Server", "acp-server", "Editor ACP server"),
	("Bluesky", "bluesky", "Bluesky AT"),
	("X/Twitter", "twitter", "X / Twitter"),
	("Reddit", "reddit", "Reddit bot"),
	("Nostr", "nostr", "Nostr relays"),
	("LINE", "line", "LINE Messaging API"),
	("Mochat", "mochat", "Mochat"),
	("Voice Call", "voice-call", "Voice call channel"),
	("VoiceWake", "voice-wake", "Always-on voice wake"),
];

/// Load channel status by probing dx-agent config on disk.
pub fn load_channels() -> Vec<ChannelEntry> {
	let config_text = load_agent_config_text();
	let configured_keys = parse_configured_channel_keys(config_text.as_deref().unwrap_or(""));
	let agent_connected = probe_agent_process();

	CHANNEL_SPECS
		.iter()
		.map(|(name, key, desc)| {
			let configured = configured_keys.iter().any(|k| k == key || k.replace('_', "-") == *key);
			ChannelEntry {
				name: (*name).to_string(),
				type_key: (*key).to_string(),
				// Without linking dx-agents we assume common channels are available.
				compiled: true,
				configured,
				// Connected only when agent is up and channel is configured.
				connected: agent_connected && configured,
				description: (*desc).to_string(),
			}
		})
		.collect()
}

fn agent_config_candidates() -> Vec<PathBuf> {
	let mut paths = Vec::new();
	if let Some(home) = dirs::home_dir() {
		paths.push(home.join(".config").join("dx").join("config.toml"));
		paths.push(home.join(".config").join("dx-agent").join("config.toml"));
		paths.push(home.join(".dx").join("config.toml"));
		paths.push(home.join(".codex").join("config.toml"));
	}
	// Workspace-relative path when developing beside agent.
	paths.push(PathBuf::from("../agent/config.toml"));
	paths.push(PathBuf::from("config.toml"));
	paths
}

fn load_agent_config_text() -> Option<String> {
	for path in agent_config_candidates() {
		if path.is_file()
			&& let Ok(text) = fs::read_to_string(&path)
		{
			return Some(text);
		}
	}
	None
}

/// Very light TOML scan for `[channels.<key>]` / `channels.<key>` / enabled keys.
fn parse_configured_channel_keys(text: &str) -> Vec<String> {
	let mut keys = Vec::new();
	for line in text.lines() {
		let trimmed = line.trim();
		if let Some(rest) = trimmed.strip_prefix("[channels.") {
			if let Some(end) = rest.find(']') {
				let key = rest[..end].trim().trim_matches('"').to_string();
				if !key.is_empty() {
					keys.push(key.replace('_', "-"));
				}
			}
		} else if let Some(rest) = trimmed.strip_prefix("[[channels.")
			&& let Some(end) = rest.find(']')
		{
			let key = rest[..end].trim().trim_matches('"').to_string();
			if !key.is_empty() {
				keys.push(key.replace('_', "-"));
			}
		}
		// type = "telegram" style under channel tables
		if let Some(val) = trimmed.strip_prefix("type") {
			let val = val.trim().trim_start_matches('=').trim().trim_matches('"');
			if CHANNEL_SPECS.iter().any(|(_, k, _)| *k == val) {
				keys.push(val.replace('_', "-"));
			}
		}
	}
	keys.sort();
	keys.dedup();
	keys
}

/// Best-effort: dx-agent binary present / process running.
fn probe_agent_process() -> bool {
	// PATH binary
	if which_exists("dx-agent") || which_exists("dx_agent") {
		return true;
	}
	// Local sibling checkout
	let local = Path::new("../agent/target/release/dx-agent");
	let local_debug = Path::new("../agent/target/debug/dx-agent");
	if local.exists() || local_debug.exists() {
		return true;
	}
	// Windows .exe
	let local_exe = Path::new("../agent/target/release/dx-agent.exe");
	let local_debug_exe = Path::new("../agent/target/debug/dx-agent.exe");
	local_exe.exists() || local_debug_exe.exists()
}

fn which_exists(name: &str) -> bool {
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

/// Whether the local dx-agent source tree is present (g:Dx/agent).
pub fn agent_source_available() -> bool {
	Path::new("../agent/Cargo.toml").is_file()
		|| Path::new("G:/Dx/agent/Cargo.toml").is_file()
		|| Path::new(r"G:\Dx\agent\Cargo.toml").is_file()
}

/// Whether the local dx-flow source tree is present (g:Dx/flow).
pub fn flow_source_available() -> bool {
	Path::new("../flow/Cargo.toml").is_file()
		|| Path::new("G:/Dx/flow/Cargo.toml").is_file()
		|| Path::new(r"G:\Dx\flow\Cargo.toml").is_file()
}

/// Connection summary line for toast / menus.
pub fn connection_summary() -> String {
	let agent = if agent_source_available() { "dx-agent ✓" } else { "dx-agent ✗" };
	let flow = if flow_source_available() { "dx-flow ✓" } else { "dx-flow ✗" };
	let channels = load_channels();
	let configured = channels.iter().filter(|c| c.configured).count();
	let connected = channels.iter().filter(|c| c.connected).count();
	format!("{agent} · {flow} · channels {connected}/{configured} connected")
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn parse_channel_table_headers() {
		let toml = r#"
[channels.telegram]
token = "x"
[channels.discord]
"#;
		let keys = parse_configured_channel_keys(toml);
		assert!(keys.contains(&"telegram".to_string()));
		assert!(keys.contains(&"discord".to_string()));
	}

	#[test]
	fn load_channels_returns_inventory() {
		let channels = load_channels();
		assert!(channels.len() >= 10);
		assert!(channels.iter().any(|c| c.type_key == "telegram"));
	}
}
