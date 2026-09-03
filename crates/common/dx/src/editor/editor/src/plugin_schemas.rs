//! Re-export of `dx_core::plugin_schemas` so editor-side modules can
//! address it via `crate::plugin_schemas::...` without a deeper
//! refactor. The actual logic lives in fresh-core because the plugin
//! runtime needs to call the validators synchronously from JS bindings.

pub use dx_core::plugin_schemas::*;
