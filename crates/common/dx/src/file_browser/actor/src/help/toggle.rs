use anyhow::Result;
use fb_macro::{render, succ};
use fb_parser::help::ToggleOpt;
use fb_shared::data::Data;

use crate::{Actor, Ctx};

pub struct Toggle;

impl Actor for Toggle {
	type Options = ToggleOpt;

	const NAME: &str = "toggle";

	fn act(cx: &mut Ctx, opt: Self::Options) -> Result<Data> {
		let help = &mut cx.help;

		help.visible = !help.visible;
		help.layer = opt.layer;

		help.keyword = String::new();
		help.in_filter = None;
		help.filter_apply();

		help.offset = 0;
		help.cursor = 0;
		succ!(render!());
	}
}

