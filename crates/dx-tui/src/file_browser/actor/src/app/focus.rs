use anyhow::Result;
use fb_actor::Ctx;
use fb_macro::act;
use fb_parser::VoidOpt;
use fb_shared::data::Data;

use crate::Actor;

pub struct Focus;

impl Actor for Focus {
	type Options = VoidOpt;

	const NAME: &str = "focus";

	fn act(cx: &mut Ctx, _: Self::Options) -> Result<Data> { act!(mgr:refresh, cx) }
}

