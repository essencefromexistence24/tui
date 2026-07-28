use mlua::{FromLua, IntoLua, Lua, LuaSerdeExt, Value};
use serde::{Deserialize, Serialize};
use fb_binding::SER_OPT;
use fb_shared::SStr;

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct TitleOpt {
	pub value: Option<SStr>,
}

impl FromLua for TitleOpt {
	fn from_lua(value: Value, lua: &Lua) -> mlua::Result<Self> { lua.from_value(value) }
}

impl IntoLua for TitleOpt {
	fn into_lua(self, lua: &Lua) -> mlua::Result<Value> { lua.to_value_with(&self, SER_OPT) }
}

