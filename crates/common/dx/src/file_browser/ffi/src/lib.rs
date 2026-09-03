#![allow(unsafe_code)]
#![allow(clippy::all)]
#![allow(clippy::pedantic)]
#![allow(clippy::nursery)]
#![allow(clippy::cargo)]

#[cfg(target_os = "macos")]
fb_macro::mod_flat!(cf_dict cf_string disk_arbitration io_kit);
