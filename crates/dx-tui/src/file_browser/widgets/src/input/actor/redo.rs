use anyhow::Result;
use fb_macro::{act, render};
use fb_shared::data::Data;

use crate::input::Input;

impl Input {
	pub fn redo(&mut self, _: ()) -> Result<Data> {
		render!(self.snaps.redo());

		act!(r#move, self)
	}
}

