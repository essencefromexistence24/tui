use anyhow::Result;
use fb_macro::{render, succ};
use fb_parser::VoidOpt;
use fb_shared::data::Data;

use crate::{Actor, Ctx};

pub struct Filter;

impl Actor for Filter {
	type Options = VoidOpt;

	const NAME: &str = "filter";

	fn act(cx: &mut Ctx, _: Self::Options) -> Result<Data> {
		let help = &mut cx.help;

		help.in_filter = Some(Default::default());
		help.filter_apply();
		succ!(render!());
	}
}

