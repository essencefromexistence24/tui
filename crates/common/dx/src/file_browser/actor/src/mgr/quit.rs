use std::time::Duration;

use anyhow::Result;
use tokio::{select, time};
use fb_config::popup::ConfirmCfg;
use fb_dds::spark::SparkKind;
use fb_macro::{act, succ};
use fb_parser::app::QuitOpt;
use fb_proxy::{AppProxy, ConfirmProxy};
use fb_shared::{data::Data, strand::{Strand, StrandLike, ToStrandJoin}, url::AsUrl};

use crate::{Actor, Ctx};

pub struct Quit;

impl Actor for Quit {
	type Options = QuitOpt;

	const NAME: &str = "quit";

	fn act(cx: &mut Ctx, opt: Self::Options) -> Result<Data> {
		let ongoing = cx.tasks().ongoing().clone();
		let (left, left_names) = {
			let ongoing = ongoing.lock();
			(ongoing.len(), ongoing.values().take(11).map(|t| t.name.clone()).collect())
		};

		if left == 0 {
			return act!(app:quit, cx, opt);
		}

		tokio::spawn(async move {
			let mut i = 0;
			let token = ConfirmProxy::show_sync(ConfirmCfg::quit(left, left_names));
			loop {
				select! {
					_ = time::sleep(Duration::from_millis(50)) => {
						i += 1;
						if i > 40 { break }
						else if ongoing.lock().is_empty() {
							AppProxy::quit(opt);
							return;
						}
					}
					b = token.future() => {
						if b {
							AppProxy::quit(opt);
						}
						return;
					}
				}
			}

			if token.future().await {
				AppProxy::quit(opt);
			}
		});
		succ!();
	}

	fn hook(cx: &Ctx, _opt: &Self::Options) -> Option<SparkKind> {
		Some(SparkKind::KeyQuit).filter(|_| cx.source().is_key())
	}
}

impl Quit {
	pub(super) fn with_selected<I>(selected: I)
	where
		I: IntoIterator,
		I::Item: AsUrl,
	{
		let paths = selected.into_iter().join(Strand::Utf8("\n"));
		if !paths.is_empty() {
			AppProxy::quit(QuitOpt { selected: Some(paths), ..Default::default() });
		}
	}
}

