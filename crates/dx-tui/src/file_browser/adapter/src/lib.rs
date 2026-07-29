#![allow(unsafe_code)]
#![allow(clippy::all)]
#![allow(clippy::pedantic)]
#![allow(clippy::nursery)]
#![allow(clippy::cargo)]

fb_macro::mod_pub!(drivers);

fb_macro::mod_flat!(adapter adapters icc image info);

use fb_emulator::{Brand, CLOSE, EMULATOR, ESCAPE, Emulator, Mux, START, TMUX};
use fb_shared::{SyncCell, in_wsl};

pub static ADAPTOR: SyncCell<Adapter> = SyncCell::new(Adapter::Chafa);

// Image state
static SHOWN: SyncCell<Option<ratatui::layout::Rect>> = SyncCell::new(None);

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
