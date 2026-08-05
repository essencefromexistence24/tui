#![allow(unsafe_code)]
#![allow(clippy::all)]
#![allow(clippy::pedantic)]
#![allow(clippy::nursery)]
#![allow(clippy::cargo)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::return_self_not_must_use)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::trivially_copy_pass_by_ref)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_lossless)]
#![allow(clippy::ptr_as_ptr)]
#![allow(clippy::borrow_as_ptr)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::if_same_then_else)]
#![allow(clippy::semicolon_if_nothing_returned)]
#![allow(clippy::default_trait_access)]
#![allow(clippy::struct_excessive_bools)]
#![allow(clippy::inline_always)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::unused_self)]
#![allow(clippy::unit_arg)]
#![allow(clippy::ignored_unit_patterns)]
#![allow(clippy::elidable_lifetime_names)]
#![allow(clippy::used_underscore_binding)]

fb_macro::mod_pub!(cha error mounts path provider);

fb_macro::mod_flat!(cwd file files filter fns hash op scheme sorter sorting splatter stage url xdg);

pub fn init() {
	CWD.init(<_>::default());

	mounts::init();
}
