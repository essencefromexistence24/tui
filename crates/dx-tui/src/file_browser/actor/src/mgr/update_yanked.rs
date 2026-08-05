use anyhow::Result;
use fb_core::mgr::Yanked;
use fb_macro::{render, succ};
use fb_parser::mgr::UpdateYankedOpt;
use fb_shared::data::Data;

use crate::{Actor, Ctx};

pub struct UpdateYanked;

impl Actor for UpdateYanked {
	type Options = UpdateYankedOpt<'static>;

	const NAME: &str = "update_yanked";

	fn act(cx: &mut Ctx, opt: Self::Options) -> Result<Data> {
		if opt.urls.is_empty() && cx.mgr.yanked.is_empty() {
			succ!();
		}

		cx.mgr.yanked = Yanked::new(opt.cut, opt.urls.into_owned());
		succ!(render!());
	}
}

