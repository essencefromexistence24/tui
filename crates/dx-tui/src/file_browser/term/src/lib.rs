#![allow(unsafe_code)]
#![allow(clippy::all)]
#![allow(clippy::pedantic)]
#![allow(clippy::nursery)]
#![allow(clippy::cargo)]

fb_macro::mod_flat!(option semaphore state term);

pub fn init() {
	YIELD_TO_SUBPROCESS.init(tokio::sync::Semaphore::new(1));
}
