#![allow(unsafe_code)]
#![allow(clippy::all)]
#![allow(clippy::pedantic)]
#![allow(clippy::nursery)]
#![allow(clippy::cargo)]

fb_macro::mod_pub!(actions);

fb_macro::mod_flat!(args boot);

use clap::Parser;
use fb_shared::RoCell;

pub static ARGS: RoCell<Args> = RoCell::new();
pub static BOOT: RoCell<Boot> = RoCell::new();

pub fn init() {
	ARGS.with(<_>::parse);
	BOOT.init(<_>::from(&*ARGS));

	actions::Actions::act(&ARGS);
}

pub fn init_default() {
	ARGS.with(<_>::default);
	BOOT.with(<_>::default);
}
