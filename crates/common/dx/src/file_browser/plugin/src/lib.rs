#![allow(unsafe_code)]
#![allow(clippy::all)]
#![allow(clippy::pedantic)]
#![allow(clippy::nursery)]
#![allow(clippy::cargo)]

fb_macro::mod_pub!(elements external fs isolate loader process pubsub runtime theme utils);

fb_macro::mod_flat!(lua);

pub fn init() -> anyhow::Result<()> {
	crate::loader::init();
	crate::init_lua()?;
	Ok(())
}
