#![allow(unsafe_code)]
#![allow(clippy::all)]
#![allow(clippy::pedantic)]
#![allow(clippy::nursery)]
#![allow(clippy::cargo)]

fb_macro::mod_pub!(provider);

fb_macro::mod_flat!(cha file files fns op);

pub fn init() {
	provider::init();
}
