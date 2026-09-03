use tokio_util::sync::CancellationToken;
use fb_config::YAZI;
use fb_fs::File;
use fb_macro::render;
use fb_parser::mgr::SpotLock;
use fb_plugin::isolate;
use fb_shared::{pool::Symbol, url::UrlBuf};

#[derive(Default)]
pub struct Spot {
	pub lock: Option<SpotLock>,
	pub skip: usize,

	pub(super) ct: Option<CancellationToken>,
}

impl Spot {
	pub fn go(&mut self, file: File, mime: Symbol<str>) {
		if mime.is_empty() {
			return; // Wait till mimetype is resolved to avoid flickering
		} else if self.same_lock(&file, &mime) {
			return;
		}

		let Some(spotter) = YAZI.plugin.spotter(&file, &mime) else {
			return self.reset();
		};

		self.abort();
		self.ct = Some(isolate::spot(&spotter.run, file, mime, self.skip));
	}

	pub fn visible(&self) -> bool { self.lock.is_some() }

	pub fn abort(&mut self) { self.ct.take().map(|ct| ct.cancel()); }

	pub fn reset(&mut self) {
		self.abort();
		render!(self.lock.take().is_some());
	}

	pub fn same_url(&self, url: &UrlBuf) -> bool { self.lock.as_ref().is_some_and(|l| *url == l.url) }

	pub fn same_file(&self, file: &File, mime: &str) -> bool {
		self.same_url(&file.url)
			&& self.lock.as_ref().is_some_and(|l| file.cha.hits(l.cha) && mime == l.mime)
	}

	pub fn same_lock(&self, file: &File, mime: &str) -> bool {
		self.same_file(file, mime) && self.lock.as_ref().is_some_and(|l| self.skip == l.skip)
	}
}

