use anyhow::Result;
use crossterm::{execute, terminal::SetTitle};
use fb_actor::Ctx;
use fb_dds::spark::SparkKind;
use fb_macro::succ;
use fb_parser::app::TitleOpt;
use fb_shared::{Source, data::Data};
use fb_term::TermState;
use fb_tty::TTY;

use crate::Actor;

pub struct Title;

impl Actor for Title {
	type Options = TitleOpt;

	const NAME: &str = "title";

	fn act(cx: &mut Ctx, opt: Self::Options) -> Result<Data> {
		let s = opt.value.unwrap_or_else(|| format!("DX TUI: {}", cx.tab().name()).into());
		execute!(TTY.writer(), SetTitle(&s))?;

		fb_term::STATE.set(TermState { title: !s.is_empty(), ..fb_term::STATE.get() });
		succ!()
	}

	fn hook(cx: &Ctx, _opt: &Self::Options) -> Option<SparkKind> {
		match cx.source() {
			Source::Ind => Some(SparkKind::IndAppTitle),
			_ => None,
		}
	}
}
