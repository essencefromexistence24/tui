use anyhow::Result;
use fb_dds::spark::SparkKind;
use fb_macro::succ;
use fb_parser::mgr::StashOpt;
use fb_shared::{Source, data::Data, url::{AsUrl, UrlLike}};

use crate::{Actor, Ctx};

pub struct Stash;

impl Actor for Stash {
	type Options = StashOpt;

	const NAME: &str = "stash";

	fn act(cx: &mut Ctx, opt: Self::Options) -> Result<Data> {
		if opt.target.is_absolute() && opt.target.is_internal() {
			cx.tab_mut().backstack.push(opt.target.as_url());
		}

		succ!()
	}

	fn hook(cx: &Ctx, _opt: &Self::Options) -> Option<SparkKind> {
		match cx.source() {
			Source::Ind => Some(SparkKind::IndStash),
			Source::Relay => Some(SparkKind::RelayStash),
			_ => None,
		}
	}
}

