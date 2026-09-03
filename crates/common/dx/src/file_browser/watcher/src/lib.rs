#![allow(unsafe_code)]
#![allow(clippy::all)]
#![allow(clippy::pedantic)]
#![allow(clippy::nursery)]
#![allow(clippy::cargo)]

fb_macro::mod_pub!(local remote);

fb_macro::mod_flat!(backend reporter watched watchee watcher);

pub static WATCHED: fb_shared::RoCell<parking_lot::RwLock<Watched>> = fb_shared::RoCell::new();
pub static WATCHER: fb_shared::RoCell<tokio::sync::Semaphore> = fb_shared::RoCell::new();

pub fn init() {
	WATCHED.with(<_>::default);
	WATCHER.init(tokio::sync::Semaphore::new(1));

	local::init();
}
