//! Channel actions: share transcript / notify via dx-agent channels.

use std::{fs, path::PathBuf, process::Command, time::Duration};

use anyhow::{Context, Result, bail};

use crate::channels::ChannelEntry;

/// Gateway process control state (best-effort local probe).
#[derive(Debug, Clone, Default)]
pub struct GatewayStatus {
	pub running: bool,
	pub detail: String,
}

/// Share a markdown transcript to a channel via dx-agent CLI when available.
pub fn share_transcript_to_channel(
	channel: &ChannelEntry,
	session_name: &str,
	transcript_md: &str,
) -> Result<String> {
	if !channel.configured && !channel.connected {
		bail!("Channel {} is not configured. Set it up in dx-agent config, then retry.", channel.name);
	}

	// Write temp export
	let path =
		std::env::temp_dir().join(format!("dx-share-{}-{}.md", channel.type_key, uuid::Uuid::new_v4()));
	fs::write(&path, transcript_md).context("write share temp file")?;

	// Prefer dx-agent CLI: `dx-agent channel send --type telegram --file ...`
	for bin in ["dx-agent", "dx_agent"] {
		if which(bin) {
			let status = Command::new(bin)
				.args([
					"channel",
					"send",
					"--type",
					&channel.type_key,
					"--title",
					session_name,
					"--file",
					path.to_str().unwrap_or(""),
				])
				.status();
			match status {
				Ok(s) if s.success() => {
					return Ok(format!("Shared to {} via {bin} ({})", channel.name, path.display()));
				}
				Ok(s) => {
					// Fallback: open file path in toast for manual paste
					return Ok(format!(
						"{bin} exit {s} — transcript ready at {} (paste into {})",
						path.display(),
						channel.name
					));
				}
				Err(e) => {
					return Ok(format!("Could not spawn {bin}: {e}. Transcript at {}", path.display()));
				}
			}
		}
	}

	// No CLI: stage path for user
	Ok(format!(
		"Transcript exported to {} — configure dx-agent CLI to auto-send to {}",
		path.display(),
		channel.name
	))
}

/// How many fixed action rows sit above the channel list in the Channels popup.
pub const CHANNELS_MENU_ACTIONS: usize = 4;

/// Labels for the fixed action rows (indices 0..CHANNELS_MENU_ACTIONS).
pub fn channels_menu_action_rows() -> [(&'static str, &'static str); CHANNELS_MENU_ACTIONS] {
	[
		("↻ Refresh status", "reload dx-agent config"),
		("▶ Start gateway", "dx-agent listeners"),
		("■ Stop gateway", "stop channel gateway"),
		("🩺 Channel doctor", "health summary"),
	]
}

/// Ensure a channel table stub exists in `~/.config/dx/config.toml` for configuration.
/// Returns (config_path, created_or_updated message).
pub fn ensure_channel_config_stub(type_key: &str, name: &str) -> Result<String> {
	let path = dirs::home_dir()
		.unwrap_or_else(|| PathBuf::from("."))
		.join(".config")
		.join("dx")
		.join("config.toml");
	if let Some(parent) = path.parent() {
		fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
	}

	let existing = fs::read_to_string(&path).unwrap_or_default();
	let header = format!("[channels.{type_key}]");
	if existing.lines().any(|l| l.trim() == header) {
		return Ok(format!(
			"{name} already in {} — set token/env, then refresh (key 1)",
			path.display()
		));
	}

	let stub = match type_key {
		"telegram" => format!(
			"\n# DX channel · {name}\n{header}\nenabled = true\n# token = \"YOUR_BOT_TOKEN\"  # or env TELEGRAM_BOT_TOKEN\n"
		),
		"discord" => format!(
			"\n# DX channel · {name}\n{header}\nenabled = true\n# token = \"YOUR_BOT_TOKEN\"  # or env DISCORD_BOT_TOKEN\n"
		),
		"slack" => format!(
			"\n# DX channel · {name}\n{header}\nenabled = true\n# bot_token = \"xoxb-...\"  # or env SLACK_BOT_TOKEN\n"
		),
		"webhook" => {
			format!("\n# DX channel · {name}\n{header}\nenabled = true\n# path = \"/hooks/dx\"\n")
		}
		"email" => format!(
			"\n# DX channel · {name}\n{header}\nenabled = true\n# smtp_host = \"smtp.example.com\"\n"
		),
		_ => format!(
			"\n# DX channel · {name}\n{header}\nenabled = true\n# Configure credentials for {type_key} via dx-agent docs\n"
		),
	};

	let mut text = existing;
	if !text.is_empty() && !text.ends_with('\n') {
		text.push('\n');
	}
	text.push_str(&stub);
	fs::write(&path, text).with_context(|| format!("write {}", path.display()))?;
	Ok(format!("Configured stub for {name} → {} · edit token then Start gateway", path.display()))
}

/// List sendable channels (configured first).
pub fn sendable_channels(all: &[ChannelEntry]) -> Vec<ChannelEntry> {
	let mut v: Vec<_> = all.iter().filter(|c| c.configured || c.connected).cloned().collect();
	if v.is_empty() {
		// Still show telegram/discord/slack as targets with setup hint
		v = all
			.iter()
			.filter(|c| {
				matches!(c.type_key.as_str(), "telegram" | "discord" | "slack" | "email" | "webhook")
			})
			.cloned()
			.collect();
	}
	v
}

fn which(name: &str) -> bool {
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

/// Path used for last share (debug / export staging).
#[allow(dead_code)]
pub fn last_share_dir() -> PathBuf {
	std::env::temp_dir()
}

fn agent_bins() -> [&'static str; 2] {
	["dx-agent", "dx_agent"]
}

/// Start the dx-agent channel gateway (Telegram/Discord/… listeners).
pub fn start_channel_gateway() -> Result<String> {
	for bin in agent_bins() {
		if !which(bin) {
			continue;
		}
		// Prefer non-blocking start variants.
		for args in
			[vec!["gateway", "start"], vec!["channel", "gateway", "start"], vec!["channels", "start"]]
		{
			let status = Command::new(bin).args(&args).status();
			if let Ok(s) = status
				&& s.success()
			{
				return Ok(format!("Gateway started via `{bin} {}`", args.join(" ")));
			}
		}
		// Last resort: spawn detached `gateway run` if available
		let mut child = Command::new(bin).args(["gateway", "run"]).spawn();
		if child.is_err() {
			child = Command::new(bin).args(["channel", "listen"]).spawn();
		}
		if let Ok(mut c) = child {
			// Don't wait forever — if it exits immediately report status
			std::thread::sleep(Duration::from_millis(200));
			match c.try_wait() {
				Ok(Some(s)) => {
					return Ok(format!("{bin} gateway exited early ({s}) — check dx-agent logs / config"));
				}
				Ok(None) => {
					// Detach: forget the child so it keeps running
					std::mem::forget(c);
					return Ok(format!("Gateway process started ({bin})"));
				}
				Err(e) => return Ok(format!("Gateway spawn uncertain: {e}")),
			}
		}
	}
	bail!("dx-agent CLI not found — install dx-agent or build with --features dx-stack")
}

/// Stop the channel gateway.
pub fn stop_channel_gateway() -> Result<String> {
	for bin in agent_bins() {
		if !which(bin) {
			continue;
		}
		for args in
			[vec!["gateway", "stop"], vec!["channel", "gateway", "stop"], vec!["channels", "stop"]]
		{
			if let Ok(s) = Command::new(bin).args(&args).status()
				&& s.success()
			{
				return Ok(format!("Gateway stopped via `{bin} {}`", args.join(" ")));
			}
		}
	}
	// Best-effort: no CLI stop
	Ok("No gateway stop command available — stop the dx-agent process manually if needed".into())
}

/// Probe whether a gateway/channel daemon looks alive.
pub fn probe_gateway() -> GatewayStatus {
	for bin in agent_bins() {
		if !which(bin) {
			continue;
		}
		for args in [vec!["gateway", "status"], vec!["channel", "status"], vec!["status"]] {
			if let Ok(out) = Command::new(bin).args(&args).output() {
				let text = String::from_utf8_lossy(&out.stdout);
				let err = String::from_utf8_lossy(&out.stderr);
				let combined = format!("{text}{err}").to_ascii_lowercase();
				let running = out.status.success()
					&& (combined.contains("running")
						|| combined.contains("active")
						|| combined.contains("listening")
						|| combined.contains("ok"));
				return GatewayStatus {
					running,
					detail: format!(
						"{bin} {} · {}",
						args.join(" "),
						text.lines().next().unwrap_or("(no output)").chars().take(80).collect::<String>()
					),
				};
			}
		}
	}
	GatewayStatus { running: false, detail: "dx-agent CLI not on PATH".into() }
}

/// Live channel doctor: configured + optional token presence (never prints secrets).
pub fn channel_doctor(channels: &[ChannelEntry]) -> Vec<(String, String)> {
	let gw = probe_gateway();
	let mut rows = vec![(
		"Gateway".into(),
		if gw.running {
			format!("running · {}", gw.detail)
		} else {
			format!("stopped · {}", gw.detail)
		},
	)];
	for ch in channels {
		let status = if ch.connected {
			"connected"
		} else if ch.configured {
			"configured"
		} else {
			"not set"
		};
		rows.push((ch.name.clone(), status.into()));
	}
	rows
}

/// Bind a TUI session id to a channel thread (metadata file for agent routing).
pub fn bind_session_to_channel(
	session_id: &str,
	channel: &ChannelEntry,
	thread_id: Option<&str>,
) -> Result<String> {
	let dir =
		dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("dx").join("channel-binds");
	fs::create_dir_all(&dir)?;
	let path = dir.join(format!("{session_id}.json"));
	let body = serde_json::json!({
		"session_id": session_id,
		"channel_type": channel.type_key,
		"channel_name": channel.name,
		"thread_id": thread_id,
		"bound_at": chrono::Local::now().to_rfc3339(),
	});
	fs::write(&path, serde_json::to_string_pretty(&body)?)?;
	Ok(format!("Bound session to {} ({})", channel.name, path.display()))
}
