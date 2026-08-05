#![allow(unsafe_code)]
#![allow(clippy::all)]
#![allow(clippy::pedantic)]
#![allow(clippy::nursery)]
#![allow(clippy::cargo)]

extern crate self as fb_actor;

fb_macro::mod_pub!(app cmp confirm core help input lives mgr notify pick spot tasks which);

fb_macro::mod_flat!(actor context);
