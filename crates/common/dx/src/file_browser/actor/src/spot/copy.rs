use anyhow::Result;
use fb_macro::succ;
use fb_parser::spot::CopyOpt;
use fb_shared::data::Data;
use fb_widgets::CLIPBOARD;

use crate::{Actor, Ctx};

pub struct Copy;

impl Actor for Copy {
	type Options = CopyOpt;

	const NAME: &str = "copy";

	fn act(cx: &mut Ctx, opt: Self::Options) -> Result<Data> {
		let spot = &cx.tab().spot;
		let Some(lock) = &spot.lock else { succ!() };
		let Some(table) = lock.table() else { succ!() };

		let mut s = String::new();
		match opt.r#type.as_ref() {
			"cell" => {
				let Some(cell) = table.selected_cell() else { succ!() };
				s = cell.to_string();
			}
			"line" => {
				// TODO
			}
			_ => {}
		}

		futures::executor::block_on(CLIPBOARD.set(s));
		succ!();
	}
}

