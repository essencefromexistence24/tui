use anyhow::Result;
use fb_dds::spark::SparkKind;
use fb_shared::data::Data;

use crate::Ctx;

pub trait Actor {
	type Options;

	const NAME: &str;

	fn act(cx: &mut Ctx, opt: Self::Options) -> Result<Data>;

	fn hook(_cx: &Ctx, _opt: &Self::Options) -> Option<SparkKind> { None }
}

