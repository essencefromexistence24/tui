//! Plugin system for external memory providers (Hermes-inspired).

#![allow(dead_code)]
//!
//! Providers can implement `MemoryProvider` to sync memory entries to external
//! services (Honcho, Hindsight, Mem0, etc). Only ONE provider is active at a time,
//! selected via `memory.provider` in the DX config.

use std::sync::Arc;

/// A memory entry.
#[derive(Debug, Clone)]
pub struct MemoryEntry {
	pub content: String,
	pub source: String, // "memory" or "user"
}

/// Result of a sync operation.
#[derive(Debug, Clone)]
pub struct SyncResult {
	pub ok: bool,
	pub message: String,
}

/// Trait for external memory providers.
pub trait MemoryProvider: Send + Sync {
	fn name(&self) -> &str;
	fn is_available(&self) -> bool;
	/// Called when a memory entry is added.
	fn on_memory_added(&self, entry: &MemoryEntry) -> SyncResult;
	/// Called when a memory entry is replaced.
	fn on_memory_replaced(&self, index: usize, entry: &MemoryEntry) -> SyncResult;
	/// Called when a memory entry is removed.
	fn on_memory_removed(&self, index: usize, source: &str) -> SyncResult;
	/// Called at session start to prefetch memories.
	fn prefetch(&self) -> Vec<MemoryEntry>;
}

/// No-op provider (default).
pub struct NullProvider;

impl MemoryProvider for NullProvider {
	fn name(&self) -> &str {
		"none"
	}
	fn is_available(&self) -> bool {
		true
	}
	fn on_memory_added(&self, _entry: &MemoryEntry) -> SyncResult {
		SyncResult { ok: true, message: "noop".into() }
	}
	fn on_memory_replaced(&self, _index: usize, _entry: &MemoryEntry) -> SyncResult {
		SyncResult { ok: true, message: "noop".into() }
	}
	fn on_memory_removed(&self, _index: usize, _source: &str) -> SyncResult {
		SyncResult { ok: true, message: "noop".into() }
	}
	fn prefetch(&self) -> Vec<MemoryEntry> {
		Vec::new()
	}
}

/// Thread-safe handle to the active memory provider.
#[derive(Clone)]
pub struct MemoryProviderHandle {
	inner: Arc<dyn MemoryProvider>,
}

impl MemoryProviderHandle {
	pub fn new() -> Self {
		Self { inner: Arc::new(NullProvider) }
	}

	pub fn set_provider(&mut self, provider: impl MemoryProvider + 'static) {
		self.inner = Arc::new(provider);
	}

	pub fn name(&self) -> &str {
		self.inner.name()
	}
	pub fn is_available(&self) -> bool {
		self.inner.is_available()
	}
	pub fn on_memory_added(&self, entry: &MemoryEntry) -> SyncResult {
		self.inner.on_memory_added(entry)
	}
	pub fn on_memory_replaced(&self, index: usize, entry: &MemoryEntry) -> SyncResult {
		self.inner.on_memory_replaced(index, entry)
	}
	pub fn on_memory_removed(&self, index: usize, source: &str) -> SyncResult {
		self.inner.on_memory_removed(index, source)
	}
	pub fn prefetch(&self) -> Vec<MemoryEntry> {
		self.inner.prefetch()
	}
}
