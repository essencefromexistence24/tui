use anyhow::Result;
use fb_macro::succ;
use fb_parser::mgr::LinkOpt;
use fb_shared::data::Data;

use crate::{Actor, Ctx};

pub struct Link;

impl Actor for Link {
	type Options = LinkOpt;

	const NAME: &str = "link";

	fn act(cx: &mut Ctx, opt: Self::Options) -> Result<Data> {
		let mgr = &mut cx.core.mgr;
		let tab = &mgr.tabs[cx.tab];

		if !mgr.yanked.cut {
			cx.core.tasks.file_link(&mgr.yanked, tab.cwd(), opt.relative, opt.force);
		}

		succ!();
	}
}

