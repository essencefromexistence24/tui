use std::cell::RefCell;

use hashbrown::HashMap;
use mlua::{AnyUserData, UserData};
use scopeguard::defer;
use tracing::error;
use fb_plugin::LUA;
use fb_shared::RoCell;

use super::{Core, PtrCell};

static TO_DESTROY: RoCell<RefCell<Vec<AnyUserData>>> = RoCell::new_const(RefCell::new(Vec::new()));
pub(super) static FILE_CACHE: RoCell<RefCell<HashMap<PtrCell<fb_fs::File>, AnyUserData>>> =
	RoCell::new();

pub struct Lives;

impl Lives {
	pub fn scope<T, F>(core: &fb_core::Core, f: F) -> mlua::Result<T>
	where
		F: FnOnce() -> mlua::Result<T>,
	{
		FILE_CACHE.init(Default::default());
		defer! { FILE_CACHE.drop(); }

		let result = LUA.scope(|scope| {
			scope.add_destructor(|| {
				for ud in TO_DESTROY.borrow_mut().drain(..) {
					ud.destroy().expect("failed to destruct scoped userdata");
				}
			});

			LUA.set_named_registry_value("cx", scope.create_any_userdata_ref(core)?)?;
			LUA.globals().raw_set("cx", Core::make(core)?)?;
			f()
		});

		if let Err(ref e) = result {
			error!("{e}");
		}
		result
	}

	pub(crate) fn scoped_userdata<T>(data: T) -> mlua::Result<AnyUserData>
	where
		T: UserData + 'static,
	{
		let ud = LUA.create_userdata(data)?;
		TO_DESTROY.borrow_mut().push(ud.clone());
		Ok(ud)
	}
}

