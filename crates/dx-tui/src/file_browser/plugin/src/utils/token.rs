use mlua::{Function, Lua};
use tiktoken_rs::cl100k_base;

use super::Utils;

impl Utils {
	pub(super) fn count_tokens(lua: &Lua) -> mlua::Result<Function> {
		lua.create_function(|_, text: mlua::String| {
			let text_bytes = text.as_bytes();
			let text_str = std::str::from_utf8(&text_bytes)
				.map_err(|e| mlua::Error::RuntimeError(format!("Invalid UTF-8: {}", e)))?;
			match cl100k_base() {
				Ok(bpe) => Ok(bpe.encode_with_special_tokens(text_str).len()),
				Err(_) => Ok(text_str.len() / 4),
			}
		})
	}
}

