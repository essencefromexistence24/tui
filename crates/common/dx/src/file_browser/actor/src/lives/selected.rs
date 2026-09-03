use mlua::AnyUserData;

use super::Lives;
use crate::lives::PtrCell;

#[derive(Clone, Copy)]
pub(super) struct Selected;

impl Selected {
	pub(super) fn make(inner: &fb_core::tab::Selected) -> mlua::Result<AnyUserData> {
		let inner = PtrCell::from(inner);

		Lives::scoped_userdata(fb_binding::Iter::new(
			inner.as_static().values().map(fb_binding::Url::new),
			Some(inner.len()),
		))
	}
}

