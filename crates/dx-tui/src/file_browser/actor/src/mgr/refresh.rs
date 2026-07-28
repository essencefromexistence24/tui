use std::time::Duration;

use anyhow::Result;
use fb_core::tab::Folder;
use fb_fs::{CWD, Files, FilesOp, cha::Cha};
use fb_macro::{act, succ};
use fb_parser::VoidOpt;
use fb_proxy::MgrProxy;
use fb_shared::{data::Data, url::{UrlBuf, UrlLike}};
use fb_vfs::{VfsFiles, VfsFilesOp};
use tokio::pin;
use tokio_stream::{StreamExt, wrappers::UnboundedReceiverStream};

use crate::{Actor, Ctx};

pub struct Refresh;

impl Actor for Refresh {
	type Options = VoidOpt;

	const NAME: &str = "refresh";

	fn act(cx: &mut Ctx, _: Self::Options) -> Result<Data> {
		CWD.set(cx.cwd(), Self::cwd_changed);

		if let Some(p) = cx.parent() {
			Self::trigger_dirs(&[cx.current(), p]);
		} else {
			Self::trigger_dirs(&[cx.current()]);
		}

		act!(mgr:peek, cx)?;
		act!(mgr:watch, cx)?;
		act!(mgr:update_paged, cx)?;

		cx.tasks().prework_sorted(&cx.current().files);
		succ!();
	}
}

impl Refresh {
	fn cwd_changed() {
		if CWD.load().kind().is_virtual() {
			MgrProxy::watch();
		}
	}

	fn trigger_dirs(folders: &[&Folder]) {
		async fn go(dir: UrlBuf, cha: Cha) {
			let Some(cha) = Files::assert_stale(&dir, cha).await else { return };

			let rx = match Files::from_dir(&dir).await {
				Ok(rx) => rx,
				Err(e) => return FilesOp::issue_error(&dir, e).await,
			};
			let stream =
				UnboundedReceiverStream::new(rx).chunks_timeout(64, Duration::from_millis(40));
			pin!(stream);

			let ticket = FilesOp::prepare(&dir);
			while let Some(chunk) = stream.next().await {
				FilesOp::Part(dir.clone(), chunk, ticket).emit();
			}
			FilesOp::Done(dir, cha, ticket).emit();
		}

		let futs: Vec<_> = folders
			.iter()
			.filter(|&f| f.url.is_absolute() && f.url.is_internal())
			.map(|&f| go(f.url.clone(), f.cha))
			.collect();

		if !futs.is_empty() {
			tokio::spawn(futures::future::join_all(futs));
		}
	}
}
