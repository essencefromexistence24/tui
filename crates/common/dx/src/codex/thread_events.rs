use std::collections::VecDeque;

use codex_app_server_protocol::ServerNotification;
use codex_app_server_protocol::ServerRequest;
use codex_app_server_protocol::Turn;
use codex_app_server_protocol::TurnStatus;

use super::thread_session_state::ThreadSessionState;

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub(crate) enum ThreadBufferedEvent {
	Notification(ServerNotification),
	Request(ServerRequest),
}

#[derive(Debug)]
pub(crate) struct ThreadEventStore {
	pub(crate) session: Option<ThreadSessionState>,
	pub(crate) turns: Vec<Turn>,
	pub(crate) buffer: VecDeque<ThreadBufferedEvent>,
	pub(crate) active_turn_id: Option<String>,
	pub(crate) capacity: usize,
}

impl ThreadEventStore {
	pub(crate) fn new(capacity: usize) -> Self {
		Self {
			session: None,
			turns: Vec::new(),
			buffer: VecDeque::new(),
			active_turn_id: None,
			capacity,
		}
	}

	pub(crate) fn set_session(&mut self, session: ThreadSessionState, turns: Vec<Turn>) {
		self.session = Some(session);
		self.set_turns(turns);
	}

	fn set_turns(&mut self, turns: Vec<Turn>) {
		self.active_turn_id = turns
			.iter()
			.rev()
			.find(|turn| matches!(turn.status, TurnStatus::InProgress))
			.map(|turn| turn.id.clone());
		self.turns = turns;
	}

	pub(crate) fn push_notification(&mut self, notification: ServerNotification) {
		match &notification {
			ServerNotification::TurnStarted(turn) => {
				self.active_turn_id = Some(turn.turn.id.clone());
			}
			ServerNotification::TurnCompleted(turn)
				if self.active_turn_id.as_deref() == Some(turn.turn.id.as_str()) =>
			{
				self.active_turn_id = None;
			}
			ServerNotification::ThreadClosed(_) => {
				self.active_turn_id = None;
			}
			_ => {}
		}
		self.buffer.push_back(ThreadBufferedEvent::Notification(notification));
		if self.buffer.len() > self.capacity
			&& let Some(removed) = self.buffer.pop_front()
			&& let ThreadBufferedEvent::Request(_request) = &removed
		{}
	}

	pub(crate) fn push_request(&mut self, request: ServerRequest) {
		self.buffer.push_back(ThreadBufferedEvent::Request(request));
		if self.buffer.len() > self.capacity
			&& let Some(removed) = self.buffer.pop_front()
			&& let ThreadBufferedEvent::Request(_) = &removed
		{}
	}

	pub(crate) fn active_turn_id(&self) -> Option<&str> {
		self.active_turn_id.as_deref()
	}
}
