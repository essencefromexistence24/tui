use anyhow::Result;
use fb_macro::{render, succ};
use fb_parser::ArrowOpt;
use fb_shared::data::Data;
use fb_widgets::Scrollable;

use crate::{Actor, Ctx};

pub struct Arrow;

impl Actor for Arrow {
	type Options = ArrowOpt;

	const NAME: &str = "arrow";

	fn act(cx: &mut Ctx, opt: Self::Options) -> Result<Data> {
		succ!(render!(cx.cmp.scroll(opt.step)));
	}
}

