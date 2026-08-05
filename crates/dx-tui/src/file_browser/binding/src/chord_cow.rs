use mlua::UserData;
use fb_codegen::FromLuaOwned;

#[derive(Clone, FromLuaOwned)]
pub struct ChordCow(pub fb_config::keymap::ChordCow);

impl From<fb_config::keymap::ChordCow> for ChordCow {
	fn from(value: fb_config::keymap::ChordCow) -> Self { Self(value) }
}

impl From<ChordCow> for fb_config::keymap::ChordCow {
	fn from(value: ChordCow) -> Self { value.0 }
}

impl UserData for ChordCow {}

