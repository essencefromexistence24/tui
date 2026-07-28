use anyhow::Result;
use fb_macro::{act, render, succ};
use fb_parser::pick::ShowOpt;
use fb_shared::data::Data;

use crate::{Actor, Ctx};

pub struct Show;

impl Actor for Show {
	type Options = ShowOpt;

	const NAME: &str = "show";

	fn act(cx: &mut Ctx, opt: Self::Options) -> Result<Data> {
		act!(pick:close, cx)?;

		let pick = &mut cx.pick;
		pick.title = opt.cfg.title;
		pick.items = opt.cfg.items;
		pick.position = opt.cfg.position;

		pick.callback = Some(opt.tx);
		pick.visible = true;
		succ!(render!());
	}
}

