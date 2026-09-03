use anyhow::Result;
use fb_config::YAZI;
use fb_macro::succ;
use fb_parser::mgr::SeekOpt;
use fb_plugin::isolate;
use fb_shared::data::Data;

use crate::{Actor, Ctx};

pub struct Seek;

impl Actor for Seek {
	type Options = SeekOpt;

	const NAME: &str = "seek";

	fn act(cx: &mut Ctx, opt: Self::Options) -> Result<Data> {
		let Some(hovered) = cx.hovered() else {
			succ!(cx.tab_mut().preview.reset());
		};

		let Some(mime) = cx.mgr.mimetype.get(&hovered.url) else {
			succ!(cx.tab_mut().preview.reset());
		};

		let Some(previewer) = YAZI.plugin.previewer(hovered, mime) else {
			succ!(cx.tab_mut().preview.reset());
		};

		isolate::seek_sync(&previewer.run, hovered.clone(), opt.units);
		succ!();
	}
}

