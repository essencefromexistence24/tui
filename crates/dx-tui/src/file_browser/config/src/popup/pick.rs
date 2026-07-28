use serde::Deserialize;
use fb_codegen::DeserializeOver2;

use super::{Offset, Origin};

#[derive(Deserialize, DeserializeOver2)]
pub struct Pick {
	// open
	pub open_title:  String,
	pub open_origin: Origin,
	pub open_offset: Offset,
}

impl Pick {
	pub const fn border(&self) -> u16 { 2 }
}

