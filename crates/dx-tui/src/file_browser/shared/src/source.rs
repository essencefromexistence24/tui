use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum Source {
	#[default]
	Unknown,

	Key,
	Emit,
	Relay,

	Ind,
}

impl Source {
	#[inline]
	#[must_use]
	pub fn is_key(self) -> bool { self == Self::Key }

	#[inline]
	#[must_use]
	pub fn is_ind(self) -> bool { self == Self::Ind }
}

