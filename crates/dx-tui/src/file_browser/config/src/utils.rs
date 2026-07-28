use std::path::PathBuf;

use fb_fs::path::{clean_url, expand_url};
use fb_shared::url::UrlBuf;

pub(crate) fn normalize_path(path: PathBuf) -> Option<PathBuf> {
	clean_url(fb_fs::provider::local::try_absolute(expand_url(UrlBuf::from(path)))?)
		.into_loc()
		.into_os()
		.ok()
		.filter(|p| p.as_os_str().is_empty() || p.is_absolute())
}

