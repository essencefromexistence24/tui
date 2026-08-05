use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationType {
	TaskComplete,
	Error,
	PermissionRequest,
	SessionSaved,
	Info,
}

impl NotificationType {
	pub fn label(self) -> &'static str {
		match self {
			Self::TaskComplete => "Task Complete",
			Self::Error => "Error",
			Self::PermissionRequest => "Permission Request",
			Self::SessionSaved => "Session Saved",
			Self::Info => "Info",
		}
	}
}

#[derive(Debug, Clone)]
pub struct DesktopNotification {
	pub title: String,
	pub body: String,
	pub kind: NotificationType,
	pub auto_show: bool,
}

impl DesktopNotification {
	pub fn new(title: impl Into<String>, body: impl Into<String>, kind: NotificationType) -> Self {
		Self {
			title: title.into(),
			body: body.into(),
			kind,
			auto_show: matches!(kind, NotificationType::Error),
		}
	}
}

pub struct NotificationManager {
	pub enabled: bool,
	pub queue: Vec<DesktopNotification>,
	last_notification: Mutex<std::time::Instant>,
	cooldown: std::time::Duration,
}

impl NotificationManager {
	pub fn new() -> Self {
		Self {
			enabled: true,
			queue: Vec::new(),
			last_notification: Mutex::new(
				std::time::Instant::now()
					.checked_sub(std::time::Duration::from_secs(30))
					.unwrap_or_else(std::time::Instant::now),
			),
			cooldown: std::time::Duration::from_secs(5),
		}
	}

	pub fn notify(&mut self, notification: DesktopNotification) {
		if !self.enabled {
			return;
		}
		let auto_show = notification.auto_show;
		self.queue.push(notification);
		if auto_show {
			self.flush();
		}
	}

	pub fn notify_simple(
		&mut self,
		title: impl Into<String>,
		body: impl Into<String>,
		kind: NotificationType,
	) {
		self.notify(DesktopNotification::new(title, body, kind));
	}

	pub fn flush(&mut self) {
		let mut last = self.last_notification.lock().unwrap();
		if last.elapsed() < self.cooldown {
			return;
		}
		while let Some(notification) = self.queue.pop() {
			*last = std::time::Instant::now();
			send_desktop_notification(&notification);
		}
	}

	pub fn dismiss(&mut self) {
		self.queue.clear();
	}

	pub fn set_enabled(&mut self, enabled: bool) {
		self.enabled = enabled;
		if !enabled {
			self.dismiss();
		}
	}
}

impl Default for NotificationManager {
	fn default() -> Self {
		Self::new()
	}
}

fn send_desktop_notification(notification: &DesktopNotification) {
	// Platform-specific notification dispatch
	#[cfg(target_os = "windows")]
	send_windows_notification(notification);

	#[cfg(target_os = "macos")]
	send_macos_notification(notification);

	#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
	send_linux_notification(notification);
}

#[cfg(target_os = "windows")]
fn send_windows_notification(_notification: &DesktopNotification) {
	// Best-effort: flash the console window as a lightweight alternative
	// since full Windows toast requires COM initialization and app manifest.
	flash_console_window();
}

#[cfg(target_os = "macos")]
fn send_macos_notification(notification: &DesktopNotification) {
	use std::process::Command;
	let script = format!(
		r#"display notification "{}" with title "{}""#,
		notification.body.replace('"', "\\\""),
		notification.title.replace('"', "\\\"")
	);
	let _ = Command::new("osascript").arg("-e").arg(&script).output();
}

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
fn send_linux_notification(notification: &DesktopNotification) {
	use std::process::Command;
	let urgency = match notification.kind {
		NotificationType::Error => "critical",
		_ => "normal",
	};
	let _ = Command::new("notify-send")
		.arg("--urgency")
		.arg(urgency)
		.arg("--app-name")
		.arg("dx-tui")
		.arg(&notification.title)
		.arg(&notification.body)
		.output();
}

#[cfg(target_os = "windows")]
fn flash_console_window() {
	// SAFETY: GetStdHandle is always safe to call; return value is unused (only needed to flash window)
	unsafe {
		let _ = windows_sys::Win32::System::Console::GetStdHandle(
			windows_sys::Win32::System::Console::STD_OUTPUT_HANDLE,
		);
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_notification_manager_default() {
		let nm = NotificationManager::new();
		assert!(nm.enabled);
		assert!(nm.queue.is_empty());
	}

	#[test]
	fn test_notify_adds_to_queue() {
		let mut nm = NotificationManager::new();
		nm.notify(DesktopNotification::new("Test", "Body", NotificationType::Info));
		assert_eq!(nm.queue.len(), 1);
	}

	#[test]
	fn test_disabled_does_not_add() {
		let mut nm = NotificationManager::new();
		nm.set_enabled(false);
		nm.notify(DesktopNotification::new("Test", "Body", NotificationType::Info));
		assert!(nm.queue.is_empty());
	}

	#[test]
	fn test_error_auto_shows() {
		let notif = DesktopNotification::new("Error", "Something failed", NotificationType::Error);
		assert!(notif.auto_show);
		let notif2 = DesktopNotification::new("Info", "Something", NotificationType::Info);
		assert!(!notif2.auto_show);
	}

	#[test]
	fn test_notification_type_labels() {
		assert_eq!(NotificationType::TaskComplete.label(), "Task Complete");
		assert_eq!(NotificationType::Error.label(), "Error");
		assert_eq!(NotificationType::Info.label(), "Info");
	}

	#[test]
	fn test_dismiss_clears_queue() {
		let mut nm = NotificationManager::new();
		nm.notify_simple("Test", "Body", NotificationType::Info);
		nm.dismiss();
		assert!(nm.queue.is_empty());
	}

	#[test]
	fn test_toggle_disabled_clears() {
		let mut nm = NotificationManager::new();
		nm.notify_simple("Test", "Body", NotificationType::Info);
		nm.set_enabled(false);
		assert!(nm.queue.is_empty());
		assert!(!nm.enabled);
	}
}
