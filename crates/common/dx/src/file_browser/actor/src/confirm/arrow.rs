use anyhow::Result;
use fb_macro::{render, succ};
use fb_parser::ArrowOpt;
use fb_shared::data::Data;

use crate::{Actor, Ctx};

pub struct Arrow;

impl Actor for Arrow {
	type Options = ArrowOpt;

	const NAME: &str = "arrow";

	fn act(cx: &mut Ctx, opt: Self::Options) -> Result<Data> {
		let confirm = &mut cx.core.confirm;

		let area = cx.core.mgr.area(confirm.position);
		let len = confirm.list.line_count(area.width);

		let old = confirm.offset;
		confirm.offset = opt.step.add(confirm.offset, len, area.height as _);

		succ!(render!(old != confirm.offset));
	}
}

