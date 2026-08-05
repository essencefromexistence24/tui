#![allow(unsafe_code)]
#![allow(clippy::all)]
#![allow(clippy::pedantic)]
#![allow(clippy::nursery)]
#![allow(clippy::cargo)]

fb_macro::mod_pub!(drivers);

fb_macro::mod_flat!(adapter adapters icc image info);

use std::sync::{
	Mutex,
	atomic::{AtomicBool, Ordering},
};

use ansi_to_tui::IntoText;
use fb_emulator::{Brand, CLOSE, EMULATOR, ESCAPE, Emulator, Mux, START, TMUX};
use fb_shared::{SyncCell, in_wsl};
use ratatui::{buffer::Buffer, layout::Rect, widgets::{Paragraph, Widget}};

pub static ADAPTOR: SyncCell<Adapter> = SyncCell::new(Adapter::Chafa);

// Image state
static SHOWN: SyncCell<Option<ratatui::layout::Rect>> = SyncCell::new(None);
static EMBEDDED: AtomicBool = AtomicBool::new(false);
static EMBEDDED_IMAGE: Mutex<Option<(Rect, Vec<u8>)>> = Mutex::new(None);

// WSL support
pub static WSL: SyncCell<bool> = SyncCell::new(false);

pub fn init() -> anyhow::Result<()> {
	init_with_flavor(false)
}

/// Embedded initialization: do not start image protocols and never allow
/// configuration recovery to take ownership of the host terminal.
pub fn init_embedded() -> anyhow::Result<()> {
	init_with_flavor(true)
}

fn init_with_flavor(embedded: bool) -> anyhow::Result<()> {
	EMBEDDED.store(embedded, Ordering::Relaxed);
	// WSL support
	WSL.set(in_wsl());

	// Grok already owns the terminal input stream. Capability detection emits
	// DA/OSC queries and waits for their replies on DX's private TTY reader,
	// which can never receive them in embedded mode. Use conservative,
	// terminal-independent defaults there; standalone DX keeps full detection.
	let mut emulator = detect_emulator(embedded);
	TMUX.set(emulator.kind.left() == Some(Brand::Tmux));

	// Tmux support
	if !embedded && TMUX.get() {
		ESCAPE.set("\x1b\x1b");
		START.set("\x1bPtmux;\x1b\x1b");
		CLOSE.set("\x1b\\");
		Mux::tmux_passthrough();
		emulator = Emulator::detect().unwrap_or_default();
	}

	EMULATOR.init(emulator);
	if embedded {
		fb_config::init_flavor_embedded(EMULATOR.light)?;
	} else {
		fb_config::init_flavor(EMULATOR.light)?;
	}

	ADAPTOR.set(Adapter::matches(&EMULATOR));
	if !embedded {
		ADAPTOR.get().start();
	}
	Ok(())
}

#[inline]
pub(crate) fn embedded() -> bool { EMBEDDED.load(Ordering::Relaxed) }

pub(crate) fn store_embedded_image(area: Rect, ansi: Vec<u8>) {
	*EMBEDDED_IMAGE.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some((area, ansi));
}

pub(crate) fn clear_embedded_image() {
	*EMBEDDED_IMAGE.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
}

/// Paint Chafa's ANSI/symbol output into Grok's Ratatui frame. Standalone DX
/// writes this through its terminal adapter; embedded mode must retain it in
/// the host buffer so the next Grok redraw does not erase the preview.
pub fn render_embedded_image(win: Rect, buf: &mut Buffer) {
	let image = EMBEDDED_IMAGE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
	let Some((area, ansi)) = image.as_ref() else { return };
	let target = area.intersection(win);
	if target.is_empty() {
		return;
	}
	if let Ok(text) = ansi.as_slice().to_text() {
		Paragraph::new(text).render(target, buf);
	}
}

fn detect_emulator(embedded: bool) -> Emulator {
	if embedded {
		Emulator::default()
	} else {
		Emulator::detect().unwrap_or_default()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn embedded_emulator_never_performs_terminal_detection() {
		let emulator = detect_emulator(true);
		assert!(emulator.version.is_empty());
		assert_eq!(emulator.csi_16t, (0, 0));
		assert!(!emulator.force_16t);
	}
}
