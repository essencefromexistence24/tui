use anyhow::Result;
use fb_macro::succ;
use fb_parser::tasks::ProcessOpenOpt;
use fb_shared::data::Data;

use crate::{Actor, Ctx};

pub struct OpenShellCompat;

// TODO: remove
impl Actor for OpenShellCompat {
	type Options = ProcessOpenOpt;

	const NAME: &str = "open_shell_compat";

	fn act(cx: &mut Ctx, opt: Self::Options) -> Result<Data> {
		succ!(cx.tasks.open_shell_compat(opt));
	}
}

