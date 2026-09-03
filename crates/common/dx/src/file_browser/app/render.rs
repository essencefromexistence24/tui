use std::sync::atomic::{AtomicU8, Ordering};

use anyhow::Result;
use crossterm::{cursor::{MoveTo, SetCursorStyle, Show}, execute, queue, terminal::{BeginSynchronizedUpdate, EndSynchronizedUpdate}};
use ratatui::{CompletedFrame, backend::{Backend, CrosstermBackend}, buffer::Buffer, layout::Position};
use fb_actor::{Ctx, lives::Lives};
use fb_binding::runtime_scope;
use fb_config::LAYOUT;
use fb_macro::{act, succ};
use fb_plugin::LUA;
use fb_shared::{data::Data, event::NEED_RENDER};
use fb_tty::TTY;
use fb_widgets::COLLISION;

use super::App;
use crate::root::TerminalRoot;

impl App {
	pub fn render(&mut self, partial: bool) -> Result<Data> {
		NEED_RENDER.store(0, Ordering::Relaxed);
		let Some(term) = &mut self.term else { succ!() };

		if partial {
			return self.render_partially();
		}

		Self::routine(true, None);
		let cursor = self.core.cursor().or_else(|| self.bridge.editor_adapter.cursor());
		let _guard = scopeguard::guard(cursor, |c| Self::routine(false, c));

		let collision = COLLISION.swap(false, Ordering::Relaxed);
		let preview_rect = LAYOUT.get().preview;
		let frame = term.draw(|f| {
			// Normal rendering path
			_ = Lives::scope(&self.core, || {
				runtime_scope!(LUA, "root", {
        let _: () = f.render_widget(TerminalRoot::new(&self.core, &mut self.bridge), f.area());
        Ok(())
    })
			});
		})?;

		if COLLISION.load(Ordering::Relaxed) {
			Self::patch(frame);
		}
		if !self.core.notify.messages.is_empty() {
			self.render_partially()?;
		}

		let cx = &mut Ctx::active(&mut self.core, &mut self.term);
		if collision && !COLLISION.load(Ordering::Relaxed) {
			act!(mgr:peek, cx, true)?; // Reload preview if collision is resolved
		} else if preview_rect != LAYOUT.get().preview {
			act!(mgr:peek, cx)?; // Reload preview if layout changed
		}
		succ!();
	}

	pub(crate) fn render_partially(&mut self) -> Result<Data> {
		let Some(term) = &mut self.term else { succ!() };
		if !term.can_partial() {
			return self.render(false);
		}

		Self::routine(true, None);
		let _guard = scopeguard::guard(self.core.cursor(), |c| Self::routine(false, c));

		let frame = term.draw_partial(|f| {
			_ = Lives::scope(&self.core, || {
				runtime_scope!(LUA, "root", {
					f.render_widget(super::super::tasks::Progress::new(&self.core), f.area());
					f.render_widget(super::super::notify::Notify::new(&self.core), f.area());
					Ok(())
				})
			});
		})?;

		if COLLISION.load(Ordering::Relaxed) {
			Self::patch(frame);
		}
		succ!();
	}

	#[inline]
	fn patch(frame: CompletedFrame) {
		let mut new = Buffer::empty(frame.area);
		for y in new.area.top()..new.area.bottom() {
			for x in new.area.left()..new.area.right() {
				let cell = &frame.buffer[(x, y)];
				if cell.skip {
					new[(x, y)] = cell.clone();
				}
				new[(x, y)].set_skip(!cell.skip);
			}
		}

		let patches = frame.buffer.diff(&new);
		CrosstermBackend::new(&mut *TTY.lockout()).draw(patches.into_iter()).ok();
	}

	fn routine(push: bool, cursor: Option<(Position, SetCursorStyle)>) {
		static COUNT: AtomicU8 = AtomicU8::new(0);
		let should_skip = if push {
			COUNT.fetch_add(1, Ordering::Relaxed) != 0
		} else {
			COUNT.fetch_sub(1, Ordering::Relaxed) != 1
		};
		if should_skip {
			return;
		}

		_ = if push {
			queue!(TTY.writer(), BeginSynchronizedUpdate)
		} else if let Some((Position { x, y }, shape)) = cursor {
			execute!(TTY.writer(), shape, MoveTo(x, y), Show, EndSynchronizedUpdate)
		} else {
			execute!(TTY.writer(), EndSynchronizedUpdate)
		};
	}
}
