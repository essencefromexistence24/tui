use mlua::{ExternalError, Function, IntoLuaMulti, Lua, Table, Value};
use fb_binding::{Error, elements::{Area, Renderable, Text}};
use fb_config::YAZI;
use fb_fs::FsUrl;
use fb_parser::mgr::{PreviewLock, UpdatePeekedOpt};
use fb_proxy::MgrProxy;
use fb_shared::{errors::PeekError, url::AsUrl};

use super::Utils;
use crate::external::Highlighter;

impl Utils {
	pub(super) fn preview_code(lua: &Lua) -> mlua::Result<Function> {
		lua.create_async_function(|lua, t: Table| async move {
			let area: Area = t.raw_get("area")?;
			let mut lock = PreviewLock::try_from(t)?;

			let path = lock.url.as_url().unified_path();
			let inner = match Highlighter::new(path).highlight(lock.skip, area.size()).await {
				Ok(text) => text,
				Err(e @ PeekError::Exceed(max)) => return (e.to_string(), max).into_lua_multi(&lua),
				Err(e @ PeekError::Unexpected(_)) => {
					return e.to_string().into_lua_multi(&lua);
				}
			};

			lock.data = vec![Renderable::Text(Text {
				area,
				inner,
				wrap: YAZI.preview.wrap.into(),
				scroll: Default::default(),
			})];

			MgrProxy::update_peeked(UpdatePeekedOpt { lock });
			().into_lua_multi(&lua)
		})
	}

	pub(super) fn preview_widget(lua: &Lua) -> mlua::Result<Function> {
		lua.create_async_function(|_, (t, value): (Table, Value)| async move {
			let mut lock = PreviewLock::try_from(t)?;
			lock.data = match value {
				Value::Nil => vec![],
				Value::Table(tbl) => tbl.sequence_values::<Renderable>().collect::<mlua::Result<_>>()?,
				Value::UserData(ud) => match Renderable::try_from(&ud) {
					Ok(r) => vec![r],
					Err(e) => {
						if let Ok(err) = ud.take::<Error>() {
							vec![
								Renderable::Clear(fb_binding::elements::Clear { area: lock.area.into() }),
								Renderable::from(err).with_area(lock.area),
							]
						} else {
							Err(e)?
						}
					}
				},
				_ => Err("preview widget must be a renderable element or a table of them".into_lua_err())?,
			};

			MgrProxy::update_peeked(UpdatePeekedOpt { lock });
			Ok(())
		})
	}
}

