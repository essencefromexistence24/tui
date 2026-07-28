use fb_config::plugin::Preloader;
use fb_shared::Id;

#[derive(Clone, Debug)]
pub(crate) struct PreloadIn {
	pub(crate) id:     Id,
	pub(crate) plugin: &'static Preloader,
	pub(crate) target: fb_fs::File,
}

impl PreloadIn {
	pub(crate) fn id(&self) -> Id { self.id }
}

