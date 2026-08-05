use mlua::{IntoLua, Lua, ObjectLike, Table};
use fb_binding::{File, elements::Rect};
use fb_config::LAYOUT;
use fb_parser::app::PluginOpt;
use fb_proxy::AppProxy;
use fb_shared::event::Action;

pub fn seek_sync(action: &'static Action, file: fb_fs::File, units: i16) {
	let cb = move |lua: &Lua, plugin: Table| {
		let job = lua.create_table_from([
			("file", File::new(file).into_lua(lua)?),
			("area", Rect::from(LAYOUT.get().preview).into_lua(lua)?),
			("units", units.into_lua(lua)?),
		])?;

		plugin.call_method("seek", job)
	};

	AppProxy::plugin(PluginOpt::new_callback(&*action.name, cb));
}

