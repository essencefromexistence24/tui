use anyhow::Result;
use fb_macro::{act, render, succ};
use fb_parser::VoidOpt;
use fb_shared::data::Data;

use crate::{Actor, Ctx};

pub struct Cancel;

impl Actor for Cancel {
	type Options = VoidOpt;

	const NAME: &str = "cancel";

	fn act(cx: &mut Ctx, _: Self::Options) -> Result<Data> {
		let tasks = &mut cx.tasks;

		let id = tasks.ongoing().lock().get_id(tasks.cursor);
		if id.map(|id| tasks.scheduler.cancel(id)) != Some(true) {
			succ!();
		}

		tasks.snaps = tasks.paginate();
		act!(tasks:arrow, cx)?;
		succ!(render!());
	}
}

