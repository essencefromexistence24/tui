use anyhow::Result;
use fb_actor::Ctx;
use fb_boot::BOOT;
use fb_macro::{act, succ};
use fb_parser::{VoidOpt, mgr::CdSource};
use fb_shared::{data::Data, strand::StrandLike, url::UrlLike};

use crate::Actor;

pub struct Bootstrap;

impl Actor for Bootstrap {
	type Options = VoidOpt;

	const NAME: &str = "bootstrap";

	fn act(cx: &mut Ctx, _: Self::Options) -> Result<Data> {
		cx.mgr.tabs.resize_with(BOOT.files.len(), Default::default);

		for (i, file) in BOOT.files.iter().enumerate().rev() {
			cx.tab = i;
			if file.is_empty() {
				act!(mgr:cd, cx, (BOOT.cwds[i].clone(), CdSource::Tab))?;
			} else if let Ok(u) = BOOT.cwds[i].try_join(file) {
				act!(mgr:reveal, cx, (u, CdSource::Tab))?;
			}
		}

		succ!();
	}
}

