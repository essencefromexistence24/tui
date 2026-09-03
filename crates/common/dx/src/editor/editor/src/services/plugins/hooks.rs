//! Hook System: Event subscription and notification for plugins
//!
//! Re-exports hook system types from fresh-core for backward compatibility.

pub use dx_core::hooks::{
	HookArgs, HookCallback, HookRegistry, LineInfo, LspLocation, hook_args_to_json,
};
