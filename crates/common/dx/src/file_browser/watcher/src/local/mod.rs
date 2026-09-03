fb_macro::mod_flat!(linked local);

pub static LINKED: fb_shared::RoCell<parking_lot::RwLock<Linked>> = fb_shared::RoCell::new();

pub(super) fn init() { LINKED.with(<_>::default); }

