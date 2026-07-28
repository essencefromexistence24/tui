use mlua::{Function, Lua, Table};
use fb_dds::Sendable;
use fb_macro::emit;
use fb_shared::{Layer, Source, event::Action};

use super::Utils;

impl Utils {
	pub(super) fn emit(lua: &Lua) -> mlua::Result<Function> {
		lua.create_function(|lua, (name, args): (String, Table)| {
			let mut action = Action::new(name, Source::Emit, Some(Layer::Mgr))?;
			action.args = Sendable::table_to_args(lua, args)?;
			Ok(emit!(Call(action)))
		})
	}

	pub(super) fn mgr_emit(lua: &Lua) -> mlua::Result<Function> {
		lua.create_function(|lua, (name, args): (String, Table)| {
			emit!(Call(Action {
				name:   name.into(),
				args:   Sendable::table_to_args(lua, args)?,
				layer:  Layer::Mgr,
				source: Source::Emit,
			}));
			Ok(())
		})
	}
}

