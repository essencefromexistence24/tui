use mlua::{ExternalError, FromLua, IntoLua, Lua, Table, Value};
use tokio::sync::mpsc;
use fb_config::{KEYMAP, keymap::{ChordCow, Key}};
use fb_shared::{Layer, event::ActionCow};

#[derive(Clone, Debug)]
pub struct ActivateOpt {
	pub tx:     Option<mpsc::UnboundedSender<Option<fb_binding::ChordCow>>>,
	pub cands:  Vec<ChordCow>,
	pub silent: bool,
	pub times:  usize,
}

impl TryFrom<ActionCow> for ActivateOpt {
	type Error = anyhow::Error;

	fn try_from(mut a: ActionCow) -> Result<Self, Self::Error> {
		if let Some(opt) = a.take_any2("opt") {
			return opt;
		}

		Ok(Self {
			tx:     a.take_any2("tx").transpose()?,
			cands:  a.take_any_iter::<fb_binding::ChordCow>().map(Into::into).collect(),
			silent: a.bool("silent"),
			times:  a.get("times").unwrap_or(0),
		})
	}
}

impl From<(Layer, Key)> for ActivateOpt {
	fn from((layer, key): (Layer, Key)) -> Self {
		Self {
			tx:     None,
			cands:  KEYMAP
				.get(layer)
				.iter()
				.filter(|c| c.on.len() > 1 && c.on[0] == key)
				.map(Into::into)
				.collect(),
			times:  1,
			silent: false,
		}
	}
}

impl FromLua for ActivateOpt {
	fn from_lua(value: Value, _: &Lua) -> mlua::Result<Self> {
		let Value::Table(t) = value else {
			return Err("expected a table".into_lua_err());
		};

		Ok(Self {
			tx:     t.raw_get::<fb_binding::MpscUnboundedTx<_>>("tx").ok().map(|t| t.0),
			cands:  t
				.raw_get::<Table>("cands")?
				.sequence_values::<fb_binding::ChordCow>()
				.map(|c| c.map(Into::into))
				.collect::<mlua::Result<Vec<_>>>()?,
			times:  t.raw_get("times").unwrap_or_default(),
			silent: t.raw_get("silent")?,
		})
	}
}

impl IntoLua for ActivateOpt {
	#[rustfmt::skip]
	fn into_lua(self, lua: &Lua) -> mlua::Result<Value> {
		lua
			.create_table_from([
				("tx", self.tx.map(fb_binding::MpscUnboundedTx).into_lua(lua)?),
				("cands", lua.create_sequence_from(self.cands.into_iter().map(fb_binding::ChordCow))?.into_lua(lua)?),
				("times", self.times.into_lua(lua)?),
				("silent", self.silent.into_lua(lua)?),
			])?
			.into_lua(lua)
	}
}

