#![allow(unsafe_code)]
#![allow(clippy::all)]
#![allow(clippy::pedantic)]
#![allow(clippy::nursery)]
#![allow(clippy::cargo)]

mod macros;

fb_macro::mod_pub!(fetch file hook plugin preload process size);

fb_macro::mod_flat!(ongoing op out progress runner scheduler snap task);

const LOW: u8 = fb_config::Priority::Low as u8;
const NORMAL: u8 = fb_config::Priority::Normal as u8;
const HIGH: u8 = fb_config::Priority::High as u8;
