//! Provider catalog (models.dev) + connected providers for dx-tui.
//!
//! Mirrors OpenCode's models.dev source: `https://models.dev/api.json`
//! with a local cache under the user config dir.

mod catalog;
mod connect;

pub use catalog::{ModelsDevCatalog, load_cached_catalog, load_or_refresh_catalog};
pub use connect::{
	ConnectedProvider, ProviderKind, ProviderStore, load_provider_store, save_provider_store,
};
