use anyhow::Result;
use fb_macro::succ;
use fb_parser::tasks::ProcessOpenOpt;
use fb_shared::data::Data;

use crate::{Actor, Ctx};

pub struct ProcessOpen;

impl Actor for ProcessOpen {
	type Options = ProcessOpenOpt;

	const NAME: &str = "process_open";

	fn act(cx: &mut Ctx, opt: Self::Options) -> Result<Data> {
		succ!(cx.tasks.scheduler.process_open(opt));
	}
}

