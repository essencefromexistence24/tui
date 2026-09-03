use codex_app_server_client::InProcessServerEvent;
use codex_app_server_protocol::AuthMode;
use codex_app_server_protocol::ServerNotification;
use codex_app_server_protocol::ServerRequest;

use super::event_targets::ServerNotificationThreadTarget;
use super::event_targets::server_notification_thread_target;
use super::event_targets::server_request_thread_id;
use super::pending_requests::PendingAppServerRequests;

pub(crate) fn handle_app_server_event(
	pending_requests: &mut PendingAppServerRequests,
	event: InProcessServerEvent,
) -> InProcessCodexEvent {
	match event {
		InProcessServerEvent::Lagged { skipped } => {
			tracing::warn!(skipped, "app-server event consumer lagged; dropping ignored events");
			InProcessCodexEvent::Lagged { skipped: skipped as u32 }
		}
		InProcessServerEvent::ServerNotification(notification) => {
			handle_server_notification(pending_requests, notification)
		}
		InProcessServerEvent::ServerRequest(request) => {
			handle_server_request(pending_requests, request)
		}
	}
}

fn handle_server_notification(
	pending_requests: &mut PendingAppServerRequests,
	notification: ServerNotification,
) -> InProcessCodexEvent {
	match &notification {
		ServerNotification::ServerRequestResolved(notification) => {
			if pending_requests.resolve_notification(&notification.request_id).is_some() {
				// Request was resolved; no longer pending
			}
			return InProcessCodexEvent::ServerRequestResolved {
				request_id: notification.request_id.clone(),
			};
		}
		ServerNotification::AccountRateLimitsUpdated(notification) => {
			return InProcessCodexEvent::AccountRateLimitsUpdated(notification.rate_limits.clone());
		}
		ServerNotification::AccountUpdated(notification) => {
			return InProcessCodexEvent::AccountUpdated {
				auth_mode: notification.auth_mode,
				plan_type: notification.plan_type,
				has_chatgpt_account: notification.auth_mode.is_some_and(AuthMode::has_chatgpt_account),
			};
		}
		ServerNotification::AppListUpdated(notification) => {
			return InProcessCodexEvent::AppListUpdated(notification.data.clone());
		}
		_ => {}
	}

	match server_notification_thread_target(&notification) {
		ServerNotificationThreadTarget::Thread(_) | ServerNotificationThreadTarget::AppScoped => {
			InProcessCodexEvent::Notification(notification)
		}
		ServerNotificationThreadTarget::InvalidThreadId(thread_id) => {
			tracing::warn!(thread_id, "ignoring app-server notification with invalid thread_id");
			InProcessCodexEvent::Ignored
		}
		ServerNotificationThreadTarget::Global => InProcessCodexEvent::Notification(notification),
	}
}

fn handle_server_request(
	pending_requests: &mut PendingAppServerRequests,
	request: ServerRequest,
) -> InProcessCodexEvent {
	if let Some(unsupported) = pending_requests.note_server_request(&request) {
		tracing::warn!(
				request_id = ?unsupported.request_id,
				message = unsupported.message,
				"rejecting unsupported app-server request"
		);
		return InProcessCodexEvent::UnsupportedRequest {
			request_id: unsupported.request_id,
			message: unsupported.message,
		};
	}

	let _thread_id = server_request_thread_id(&request);
	InProcessCodexEvent::Request(request)
}

#[allow(dead_code)]
#[derive(Debug)]
pub(crate) enum InProcessCodexEvent {
	Notification(ServerNotification),
	Request(ServerRequest),
	ServerRequestResolved {
		request_id: codex_app_server_protocol::RequestId,
	},
	AccountRateLimitsUpdated(codex_app_server_protocol::RateLimitSnapshot),
	AccountUpdated {
		auth_mode: Option<AuthMode>,
		plan_type: Option<codex_protocol::account::PlanType>,
		has_chatgpt_account: bool,
	},
	AppListUpdated(Vec<codex_app_server_protocol::AppInfo>),
	UnsupportedRequest {
		request_id: codex_app_server_protocol::RequestId,
		message: String,
	},
	Lagged {
		skipped: u32,
	},
	Disconnected {
		message: String,
	},
	Ignored,
}
