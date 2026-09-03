use std::ops::Deref;

use mlua::{AnyUserData, MetaMethod, UserData, UserDataMethods};

use super::{Lives, PtrCell};

pub(super) struct Filter {
	inner: PtrCell<fb_fs::Filter>,
}

impl Deref for Filter {
	type Target = fb_fs::Filter;

	fn deref(&self) -> &Self::Target { &self.inner }
}

impl Filter {
	pub(super) fn make(inner: &fb_fs::Filter) -> mlua::Result<AnyUserData> {
		Lives::scoped_userdata(Self { inner: inner.into() })
	}
}

impl UserData for Filter {
	fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
		methods.add_meta_method(MetaMethod::ToString, |_, me, ()| Ok(me.to_string()));
	}
}

