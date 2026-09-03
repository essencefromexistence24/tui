use anyhow::Result;
use fb_fs::path::clean_url;
use fb_macro::{act, succ};
use fb_parser::{VoidOpt, mgr::CdSource};
use fb_shared::{data::Data, url::UrlLike};

use crate::{Actor, Ctx};

pub struct Follow;

impl Actor for Follow {
	type Options = VoidOpt;

	const NAME: &str = "follow";

	fn act(cx: &mut Ctx, _: Self::Options) -> Result<Data> {
		let Some(file) = cx.hovered() else { succ!() };
		let Some(link_to) = &file.link_to else { succ!() };
		let Some(parent) = file.url.parent() else { succ!() };
		let Ok(joined) = parent.try_join(link_to) else { succ!() };
		act!(mgr:reveal, cx, (clean_url(joined), CdSource::Follow))
	}
}

