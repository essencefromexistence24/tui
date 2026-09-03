#![allow(unsafe_code)]
#![allow(clippy::all)]
#![allow(clippy::pedantic)]
#![allow(clippy::nursery)]
#![allow(clippy::cargo)]

mod macros;

fb_macro::mod_flat!(app cmp confirm input mgr notify pick tasks which);
