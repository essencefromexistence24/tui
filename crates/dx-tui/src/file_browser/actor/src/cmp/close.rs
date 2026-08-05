use std::mem;

use anyhow::Result;
use fb_macro::{act, render, succ};
use fb_parser::cmp::CloseOpt;
use fb_shared::data::Data;
use fb_widgets::input::parser::CompleteOpt;

use crate::{Actor, Ctx};

pub struct Close;

impl Actor for Close {
	type Options = CloseOpt;

	const NAME: &str = "close";

	fn act(cx: &mut Ctx, opt: Self::Options) -> Result<Data> {
		let cmp = &mut cx.cmp;
		if let Some(item) = cmp.selected().filter(|_| opt.submit).cloned() {
			return act!(input:complete, cx, CompleteOpt { name: item.name, is_dir: item.is_dir, ticket: cmp.ticket });
		}

		cmp.caches.clear();
		cmp.ticket = Default::default();
		succ!(render!(mem::replace(&mut cmp.visible, false)));
	}
}

