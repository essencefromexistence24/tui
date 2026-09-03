use anyhow::Result;
use fb_macro::succ;
use fb_parser::VoidOpt;
use fb_shared::data::Data;

use crate::{Actor, Ctx};

pub struct Suspend;

impl Actor for Suspend {
	type Options = VoidOpt;

	const NAME: &str = "suspend";

	fn act(_: &mut Ctx, _: Self::Options) -> Result<Data> {
		#[cfg(unix)]
		if !fb_shared::session_leader() {
			unsafe {
				libc::raise(libc::SIGTSTP);
			}
		}
		succ!();
	}
}

