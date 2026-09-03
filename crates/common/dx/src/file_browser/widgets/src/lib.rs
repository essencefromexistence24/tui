#![allow(unsafe_code)]
#![allow(clippy::all)]
#![allow(clippy::pedantic)]
#![allow(clippy::nursery)]
#![allow(clippy::cargo)]

fb_macro::mod_pub!(input);

fb_macro::mod_flat!(clear clipboard scrollable step);

pub fn init() {
	CLIPBOARD.with(<_>::default);
}
