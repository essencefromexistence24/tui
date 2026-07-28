use std::{
	sync::atomic::Ordering,
	time::{Duration, Instant},
};

use anyhow::Result;
use fb_actor::Ctx;
use fb_core::Core;
use fb_macro::act;
use fb_shared::{
	data::Data,
	event::{Event, NEED_RENDER},
};
use fb_term::Term;
use tokio::{select, time::sleep};
// use tracing::debug;  // COMMENTED OUT: only used by commented-out agent/flow backend init

use crate::bridge::TuiSession;
use crate::dispatcher::Dispatcher;
use crate::signals::Signals;

pub(crate) struct App {
	pub(crate) core: Core,
	pub(crate) term: Option<Term>,
	pub(crate) signals: Signals,
	pub(crate) bridge: TuiSession,
}

impl App {
	pub(crate) async fn serve() -> Result<()> {
		let term = Term::start()?;
		let (mut rx, signals) = (Event::take(), Signals::start()?);

		let mut app =
			Self { core: Core::make(), term: Some(term), signals, bridge: TuiSession::new() };
		app.bootstrap()?;

		// Animation timer: 50ms = ~20 FPS for smooth animations
		let mut animation_timer = tokio::time::interval(Duration::from_millis(50));
		animation_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

		let mut events = Vec::with_capacity(50);
		let (mut timeout, mut need_render, mut last_render) = (None, 0, Instant::now());
		macro_rules! drain_events {
			() => {
				for event in events.drain(..) {
					Dispatcher::new(&mut app).dispatch(event)?;

					need_render = NEED_RENDER.load(Ordering::Relaxed);
					if need_render == 0 {
						continue;
					}

					timeout = Duration::from_millis(10).checked_sub(last_render.elapsed());
					if timeout.is_none() {
						app.render(need_render == 2)?;
						last_render = Instant::now();
					}
				}
			};
		}

		loop {
			if let Some(t) = timeout.take() {
				select! {
					_ = sleep(t) => {
						app.render(need_render == 2)?;
						last_render = Instant::now();
					}
					_ = animation_timer.tick() => {
						// Timer tick for animations - emit Timer event
						events.push(Event::Timer);
						drain_events!();
					}
					n = rx.recv_many(&mut events, 50) => {
						if n == 0 { break }
						drain_events!();
					}
				}
			} else {
				select! {
					_ = animation_timer.tick() => {
						// Timer tick for animations - emit Timer event
						events.push(Event::Timer);
						drain_events!();
					}
					n = rx.recv_many(&mut events, 50) => {
						if n == 0 { break }
						drain_events!();
					} else => break,
				}
			}
		}
		Ok(())
	}

	fn bootstrap(&mut self) -> anyhow::Result<Data> {
		let cx = &mut Ctx::active(&mut self.core, &mut self.term);
		act!(app:bootstrap, cx)?;

		// Agent/flow warmup happens in ChatState::new → bootstrap_backends().

		self.render(false)
	}
}
