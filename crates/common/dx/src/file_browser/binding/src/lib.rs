#![allow(unsafe_code)]
#![allow(clippy::all)]
#![allow(clippy::pedantic)]
#![allow(clippy::nursery)]
#![allow(clippy::cargo)]

mod macros;

fb_macro::mod_pub!(elements);

fb_macro::mod_flat!(access calculator cha chan chord_cow color composer error fd file handle icon id image input iter layer mouse path permit range runtime scheme stage style url utils);
