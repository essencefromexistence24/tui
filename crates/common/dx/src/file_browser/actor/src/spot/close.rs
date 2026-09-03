use anyhow::Result;
use fb_macro::succ;
use fb_parser::VoidOpt;
use fb_shared::data::Data;

use crate::{Actor, Ctx};

pub struct Close;

impl Actor for Close {
	type Options = VoidOpt;

	const NAME: &str = "close";

	fn act(cx: &mut Ctx, _: Self::Options) -> Result<Data> {
		succ!(cx.tab_mut().spot.reset());
	}
}

