use anyhow::Result;
use fb_macro::succ;
use fb_parser::VoidOpt;
use fb_shared::data::Data;

use crate::{Actor, Ctx};

pub struct Dismiss;

impl Actor for Dismiss {
	type Options = VoidOpt;

	const NAME: &str = "dismiss";

	fn act(cx: &mut Ctx, _: Self::Options) -> Result<Data> {
		succ!(cx.which.dismiss(None));
	}
}

