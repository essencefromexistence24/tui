use fb_config::plugin::Fetcher;
use fb_shared::Id;

#[derive(Debug)]
pub(crate) struct FetchIn {
	pub(crate) id:      Id,
	pub(crate) plugin:  &'static Fetcher,
	pub(crate) targets: Vec<fb_fs::File>,
}

impl FetchIn {
	pub(crate) fn id(&self) -> Id { self.id }
}

