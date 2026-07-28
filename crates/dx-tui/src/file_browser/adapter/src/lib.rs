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
	// WSL support
	WSL.set(in_wsl());

	// Emulator detection
	let mut emulator = Emulator::detect().unwrap_or_default();
	TMUX.set(emulator.kind.left() == Some(Brand::Tmux));

	// Tmux support
	if TMUX.get() {
		ESCAPE.set("\x1b\x1b");
		START.set("\x1bPtmux;\x1b\x1b");
		CLOSE.set("\x1b\\");
		Mux::tmux_passthrough();
		emulator = Emulator::detect().unwrap_or_default();
	}

	EMULATOR.init(emulator);
	fb_config::init_flavor(EMULATOR.light)?;

	ADAPTOR.set(Adapter::matches(&EMULATOR));
	ADAPTOR.get().start();
	Ok(())
}
