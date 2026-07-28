use anyhow::Result;
use crossterm::event::{Event as CrosstermEvent, EventStream};
use fb_config::YAZI;
use fb_shared::{CompletionToken, event::Event};
use futures::StreamExt;
use tokio::{select, sync::mpsc};

pub(super) struct Signals {
	pub(super) tx: mpsc::UnboundedSender<(bool, CompletionToken)>,
}

impl Signals {
	pub(super) fn start() -> Result<Self> {
		let (tx, rx) = mpsc::unbounded_channel();
		Self::spawn(rx)?;

		Ok(Self { tx })
	}

	#[cfg(unix)]
	fn handle_sys(n: libc::c_int) -> bool {
		use fb_proxy::AppProxy;
		use fb_term::YIELD_TO_SUBPROCESS;
		use libc::{SIGCONT, SIGHUP, SIGINT, SIGQUIT, SIGSTOP, SIGTERM, SIGTSTP};
		use tracing::error;

		match n {
			SIGINT => { /* ignored */ }
			SIGQUIT | SIGHUP | SIGTERM => {
				AppProxy::quit(Default::default());
				return false;
			}
			SIGTSTP => {
				tokio::spawn(async move {
					AppProxy::stop().await;
					// SAFETY: pid=0 targets self-process group; SIGSTOP is a valid signal; kill is async-signal-safe
					if unsafe { libc::kill(0, SIGSTOP) } != 0 {
						error!("Failed to stop the process:\n{}", std::io::Error::last_os_error());
						AppProxy::quit(Default::default());
					}
				});
			}
			SIGCONT if YIELD_TO_SUBPROCESS.try_acquire().is_ok() => _ = tokio::spawn(AppProxy::resume()),
			_ => {}
		}
		true
	}

	#[cfg(windows)]
	#[inline]
	fn handle_sys(_: ()) -> bool {
		unreachable!()
	}

	fn handle_term(event: CrosstermEvent) {
		match event {
			// Press/Repeat/Release — Release needed to track Shift for Shift+Enter
			CrosstermEvent::Key(key) => Event::Key(key).emit(),
			// Always forward mouse events for chat (select/drag/hover). Fall back to
			// mgr.mouse_events filter when a kind is restricted.
			CrosstermEvent::Mouse(mouse) => {
				let allowed = YAZI.mgr.mouse_events.get();
				// Ensure drag/move reach the chat input even if config omitted them.
				use crossterm::event::MouseEventKind as K;
				let always = matches!(
					mouse.kind,
					K::Down(_) | K::Up(_) | K::Drag(_) | K::Moved | K::ScrollUp | K::ScrollDown
				);
				if always || allowed.contains(mouse.kind.into()) {
					Event::Mouse(mouse).emit();
				}
			}
			CrosstermEvent::Resize(..) => Event::Resize.emit(),
			CrosstermEvent::FocusGained => Event::Focus.emit(),
			CrosstermEvent::Paste(str) => Event::Paste(str).emit(),
			_ => {}
		}
	}

	fn spawn(mut rx: mpsc::UnboundedReceiver<(bool, CompletionToken)>) -> Result<()> {
		#[cfg(unix)]
		use libc::{SIGCONT, SIGHUP, SIGINT, SIGQUIT, SIGTERM, SIGTSTP};

		#[cfg(unix)]
		let mut sys = signal_hook_tokio::Signals::new([
			// Interrupt signals (Ctrl-C, Ctrl-\)
			SIGINT, SIGQUIT, //
			// Hangup signal (Terminal closed)
			SIGHUP, //
			// Termination signal (kill)
			SIGTERM, //
			// Job control signals (Ctrl-Z, fg/bg)
			SIGTSTP, SIGCONT,
		])?;
		#[cfg(windows)]
		let mut sys = tokio_stream::empty();

		let mut term = Some(EventStream::new());

		tokio::spawn(async move {
			loop {
				if let Some(t) = &mut term {
					select! {
						biased;
						Some((state, token)) = rx.recv() => {
							term = term.filter(|_| state);
							token.complete(true);
						},
						Some(n) = sys.next() => if !Self::handle_sys(n) { return },
						Some(Ok(e)) = t.next() => Self::handle_term(e)
					}
				} else {
					select! {
						biased;
						Some((state, token)) = rx.recv() => {
							term = state.then(EventStream::new);
							token.complete(true);
						},
						Some(n) = sys.next() => if !Self::handle_sys(n) { return },
					}
				}
			}
		});

		Ok(())
	}
}
