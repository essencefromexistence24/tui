use anyhow::Result;
use fb_macro::{act, render, succ};
use fb_parser::VoidOpt;
use fb_shared::data::Data;
use fb_widgets::input::InputOp;

use crate::{Actor, Ctx};

pub struct Escape;

impl Actor for Escape {
	type Options = VoidOpt;

	const NAME: &str = "escape";

	fn act(cx: &mut Ctx, _: Self::Options) -> Result<Data> {
		use fb_widgets::input::InputMode as M;
		let input = &mut cx.input;

		let mode = input.snap().mode;
		match mode {
			M::Normal if input.snap_mut().op == InputOp::None => act!(input:close, cx),
			M::Insert => act!(cmp:close, cx),
			M::Normal | M::Replace => Ok(().into()),
		}?;

		act!(escape, cx.input)?;
		succ!(render!());
	}
}

