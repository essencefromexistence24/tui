use anyhow::Result;
use fb_macro::succ;
use fb_parser::tasks::UpdateSucceedOpt;
use fb_shared::data::Data;

use crate::{Actor, Ctx};

pub struct UpdateSucceed;

impl Actor for UpdateSucceed {
	type Options = UpdateSucceedOpt;

	const NAME: &str = "update_succeed";

	fn act(cx: &mut Ctx, opt: Self::Options) -> Result<Data> {
		cx.mgr.watcher.report(opt.urls);
		succ!();
	}
}

