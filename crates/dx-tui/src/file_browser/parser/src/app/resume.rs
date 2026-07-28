use mlua::{ExternalError, FromLua, IntoLua, Lua, Table, Value};
use ratatui::layout::Rect;
use tokio::sync::mpsc;
use fb_shared::CompletionToken;

#[derive(Debug)]
pub struct ResumeOpt {
	pub tx:     mpsc::UnboundedSender<(bool, CompletionToken)>,
	pub token:  CompletionToken,
	pub reflow: fn(Rect) -> mlua::Result<Table>,
}

impl FromLua for ResumeOpt {
	fn from_lua(_: Value, _: &Lua) -> mlua::Result<Self> { Err("unsupported".into_lua_err()) }
}

impl IntoLua for ResumeOpt {
	fn into_lua(self, _: &Lua) -> mlua::Result<Value> { Err("unsupported".into_lua_err()) }
}

