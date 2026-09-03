//! Automation scheduler — runs prompts on a timer interval or daily schedule.

#![allow(dead_code)]
//!
//! Usage: user sets interval via `/automation every 30m` or `/automation daily 09:00`.
//! The scheduler tracks last-run time and fires the next prompt when due.
//!
//! Integrates with ChatState via `update()` tick: checks if automation is due.

use std::{
	fs,
	path::PathBuf,
	time::{Duration, Instant},
};

use chrono::Timelike;

/// Interval presets (minutes).
pub const INTERVAL_PRESETS: &[u32] = &[5, 10, 15, 30, 60, 120, 360, 1440];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Schedule {
	/// Run every N minutes.
	Interval(u32),
	/// Run daily at a specific hour/minute (0-23, 0-59).
	Daily { hour: u32, minute: u32 },
	/// No schedule (paused).
	#[default]
	Off,
}

impl Schedule {
	pub fn label(&self) -> String {
		match self {
			Self::Off => "Off".into(),
			Self::Interval(m) => {
				if *m < 60 {
					format!("Every {m}m")
				} else {
					format!("Every {}h{}", m / 60, m % 60)
				}
			}
			Self::Daily { hour, minute } => {
				format!("Daily {:02}:{:02}", hour, minute)
			}
		}
	}

	/// Check if this schedule is due based on last run time.
	pub fn is_due(&self, last_run: Option<Instant>) -> bool {
		match self {
			Self::Off => false,
			Self::Interval(m) => {
				let Some(last) = last_run else {
					return true;
				};
				let interval = Duration::from_secs(*m as u64 * 60);
				last.elapsed() >= interval
			}
			Self::Daily { hour, minute } => {
				let Some(last) = last_run else {
					return true;
				};
				// Simplified daily check: run once per day window
				let now = chrono::Local::now();
				let now_hour = now.hour12().1;
				let now_min = now.minute();
				let target_minutes = hour * 60 + minute;
				let current_minutes = now_hour * 60 + now_min;
				// It's past the target time today
				if current_minutes < target_minutes {
					return false;
				}
				// Check we haven't already run today
				if last.elapsed() < Duration::from_secs(12 * 3600) {
					return false; // ran within last 12 hours (daily guard)
				}
				true
			}
		}
	}

	/// Parse from string: "5m", "10m", "30m", "1h", "daily 09:00", "off"
	pub fn parse(input: &str) -> Self {
		let t = input.trim().to_ascii_lowercase();
		if t == "off" || t == "stop" || t == "pause" {
			return Self::Off;
		}
		if let Some(rest) = t.strip_prefix("daily ") {
			if let Some((h, m)) = rest.trim().split_once(':') {
				let hour = h.parse::<u32>().unwrap_or(9).min(23);
				let minute = m.parse::<u32>().unwrap_or(0).min(59);
				return Self::Daily { hour, minute };
			}
			// Try "daily HH:MM" without space
			if let Some((h, m)) = rest.trim().split_once(':') {
				let hour = h.parse::<u32>().unwrap_or(9).min(23);
				let minute = m.parse::<u32>().unwrap_or(0).min(59);
				return Self::Daily { hour, minute };
			}
		}
		// "5m", "10m", "30m", "1h", "2h"
		let num: u32 =
			t.chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse().unwrap_or(30);
		if t.contains('h') { Self::Interval(num * 60) } else { Self::Interval(num.max(1)) }
	}
}

/// Scheduler state stored in ChatState.
#[derive(Debug, Clone)]
pub struct AutomationState {
	/// Current schedule.
	pub schedule: Schedule,
	/// When the last automation prompt was sent.
	pub last_run: Option<Instant>,
	/// The prompt text to send on each tick.
	pub prompt: String,
}

impl Default for AutomationState {
	fn default() -> Self {
		Self {
			schedule: Schedule::Off,
			last_run: None,
			prompt: "Status check — report any issues or updates.".into(),
		}
	}
}

impl AutomationState {
	pub fn new(prompt: String, schedule: Schedule) -> Self {
		Self { schedule, prompt, last_run: None }
	}

	/// Check if automation should fire now.
	pub fn should_run(&self) -> bool {
		self.schedule.is_due(self.last_run)
	}

	/// Mark that automation just ran.
	pub fn mark_run(&mut self) {
		self.last_run = Some(Instant::now());
		self.save_to_disk();
	}

	pub fn status_line(&self) -> String {
		format!(
			"Auto: {} · {}",
			self.schedule.label(),
			self.prompt.chars().take(40).collect::<String>()
		)
	}

	/// Path to automation state file.
	fn state_path() -> PathBuf {
		crate::agent_workspace::workspace_dir().join("automation_state.json")
	}

	/// Save schedule + prompt to disk (survives restarts).
	pub fn save_to_disk(&self) {
		let json = serde_json::json!({
			"schedule": match &self.schedule {
				Schedule::Interval(m) => format!("every_{m}m"),
				Schedule::Daily { hour, minute } => format!("daily_{:02}:{:02}", hour, minute),
				Schedule::Off => "off".to_string(),
			},
			"prompt": self.prompt,
		});
		if let Ok(text) = serde_json::to_string_pretty(&json) {
			let _ = fs::write(Self::state_path(), &text);
		}
	}

	/// Load schedule + prompt from disk.
	pub fn load_from_disk() -> Self {
		let path = Self::state_path();
		let text = match fs::read_to_string(&path) {
			Ok(t) => t,
			Err(_) => return Self::default(),
		};
		let json: serde_json::Value = match serde_json::from_str(&text) {
			Ok(v) => v,
			Err(_) => return Self::default(),
		};
		let schedule_str = json.get("schedule").and_then(|v| v.as_str()).unwrap_or("off");
		let schedule = if let Some(rest) = schedule_str.strip_prefix("every_") {
			let m = rest.trim_end_matches('m').parse::<u32>().unwrap_or(30);
			Schedule::Interval(m)
		} else if let Some(rest) = schedule_str.strip_prefix("daily_") {
			if let Some((h, m)) = rest.split_once(':') {
				let hour = h.parse::<u32>().unwrap_or(9).min(23);
				let minute = m.parse::<u32>().unwrap_or(0).min(59);
				Schedule::Daily { hour, minute }
			} else {
				Schedule::Off
			}
		} else {
			Schedule::Off
		};
		let prompt = json.get("prompt").and_then(|v| v.as_str()).unwrap_or("").to_string();
		Self { schedule, prompt, last_run: None }
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn parse_interval_minutes() {
		assert_eq!(Schedule::parse("5m"), Schedule::Interval(5));
		assert_eq!(Schedule::parse("30m"), Schedule::Interval(30));
		assert_eq!(Schedule::parse("60m"), Schedule::Interval(60));
	}

	#[test]
	fn parse_interval_hours() {
		assert_eq!(Schedule::parse("1h"), Schedule::Interval(60));
		assert_eq!(Schedule::parse("2h"), Schedule::Interval(120));
	}

	#[test]
	fn parse_daily() {
		match Schedule::parse("daily 09:00") {
			Schedule::Daily { hour, minute } => {
				assert_eq!(hour, 9);
				assert_eq!(minute, 0);
			}
			_ => panic!("expected Daily"),
		}
	}

	#[test]
	fn parse_off() {
		assert_eq!(Schedule::parse("off"), Schedule::Off);
		assert_eq!(Schedule::parse("stop"), Schedule::Off);
	}

	#[test]
	fn schedule_default() {
		assert_eq!(Schedule::default(), Schedule::Off);
	}

	#[test]
	fn interval_is_due() {
		let s = Schedule::Interval(5);
		assert!(s.is_due(None)); // never run → due
		assert!(!s.is_due(Some(Instant::now()))); // just ran → not due
	}

	#[test]
	fn off_is_never_due() {
		assert!(!Schedule::Off.is_due(None));
	}

	#[test]
	fn label_formats() {
		assert_eq!(Schedule::Interval(5).label(), "Every 5m");
		assert_eq!(Schedule::Interval(120).label(), "Every 2h0");
		assert_eq!(Schedule::Daily { hour: 9, minute: 0 }.label(), "Daily 09:00");
		assert_eq!(Schedule::Off.label(), "Off");
	}
}
