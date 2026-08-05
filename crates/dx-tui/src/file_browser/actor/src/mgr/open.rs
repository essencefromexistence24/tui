use anyhow::Result;
use fb_boot::ARGS;
use fb_fs::File;
use fb_macro::{act, succ};
use fb_parser::mgr::{OpenDoOpt, OpenOpt};
use fb_proxy::MgrProxy;
use fb_shared::data::Data;
use fb_vfs::VfsFile;

use crate::{Actor, Ctx, mgr::Quit};

pub struct Open;

impl Actor for Open {
	type Options = OpenOpt;

	const NAME: &str = "open";

	fn act(cx: &mut Ctx, mut opt: Self::Options) -> Result<Data> {
		if !opt.interactive && ARGS.chooser_file.is_some() {
			succ!(if !opt.targets.is_empty() {
				Quit::with_selected(opt.targets)
			} else if opt.hovered {
				Quit::with_selected(cx.hovered().map(|h| &h.url))
			} else {
				act!(mgr:escape_visual, cx)?;
				Quit::with_selected(cx.tab().selected_or_hovered())
			});
		}

		if opt.targets.is_empty() {
			opt.targets = if opt.hovered {
				cx.hovered().map(|h| vec![h.url.clone().into()]).unwrap_or_default()
			} else {
				act!(mgr:escape_visual, cx)?;
				cx.tab().selected_or_hovered().cloned().map(Into::into).collect()
			};
		}
		if opt.targets.is_empty() {
			succ!();
		}

		let todo: Vec<_> = opt
			.targets
			.iter()
			.enumerate()
			.filter(|&(_, u)| !cx.mgr.mimetype.contains(u))
			.map(|(i, _)| i)
			.collect();

		let cwd = opt.cwd.unwrap_or_else(|| cx.cwd().clone().into());
		if todo.is_empty() {
			return act!(mgr:open_do, cx, OpenDoOpt { cwd, targets: opt.targets, interactive: opt.interactive });
		}

		let scheduler = cx.tasks.scheduler.clone();
		tokio::spawn(async move {
			let mut files = Vec::with_capacity(todo.len());
			for i in todo {
				if let Ok(f) = File::new(&opt.targets[i]).await {
					files.push(f);
				}
			}
			if scheduler.fetch_mimetype(files).await {
				MgrProxy::open_do(OpenDoOpt { cwd, targets: opt.targets, interactive: opt.interactive });
			}
		});
		succ!();
	}
}

