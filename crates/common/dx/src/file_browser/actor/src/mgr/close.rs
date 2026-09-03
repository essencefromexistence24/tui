use anyhow::Result;
use fb_macro::act;
use fb_parser::mgr::CloseOpt;
use fb_shared::data::Data;

use crate::{Actor, Ctx};

pub struct Close;

impl Actor for Close {
	type Options = CloseOpt;

	const NAME: &str = "close";

	fn act(cx: &mut Ctx, opt: Self::Options) -> Result<Data> {
		if cx.tabs().len() > 1 {
			act!(mgr:tab_close, cx, cx.tabs().cursor)
		} else {
			act!(mgr:quit, cx, opt.0)
		}
	}
}

