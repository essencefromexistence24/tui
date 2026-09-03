use anyhow::Result;
use fb_macro::{render, succ};
use fb_parser::mgr::UpdatePeekedOpt;
use fb_shared::data::Data;

use crate::{Actor, Ctx};

pub struct UpdatePeeked;

impl Actor for UpdatePeeked {
	type Options = UpdatePeekedOpt;

	const NAME: &str = "update_peeked";

	fn act(cx: &mut Ctx, opt: Self::Options) -> Result<Data> {
		let Some(hovered) = cx.hovered().map(|h| &h.url) else {
			succ!(cx.tab_mut().preview.reset());
		};

		if opt.lock.url == *hovered {
			cx.tab_mut().preview.lock = Some(opt.lock);
			render!();
		}

		succ!();
	}
}

