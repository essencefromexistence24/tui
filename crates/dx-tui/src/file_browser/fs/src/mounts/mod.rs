fb_macro::mod_flat!(partition partitions);

#[cfg(target_os = "linux")]
fb_macro::mod_flat!(linux);

#[cfg(target_os = "macos")]
fb_macro::mod_flat!(macos);

pub(super) fn init() { PARTITIONS.init(<_>::default()); }

