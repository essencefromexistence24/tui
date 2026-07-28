//! Embedded mode for dx-tui.
//!
//! Runs the dx-tui API server as a background service without a TUI.
//! Useful for IDE plugins, CI/CD tools, and other headless integrations.

#![allow(dead_code)]

use std::sync::Arc;

use anyhow::Result;

use crate::{
	api::{ApiConfig, ApiServer, Telemetry},
	api_handler::{AppApiContext, AppApiHandler},
};

/// Embedded dx-tui server that runs the API without a TUI.
pub struct EmbeddedServer {
	config: ApiConfig,
	server: Option<ApiServer>,
	handle: Option<tokio::task::JoinHandle<()>>,
}

impl EmbeddedServer {
	/// Create a new embedded server with default config.
	pub fn new() -> Self {
		Self::with_config(ApiConfig::default())
	}

	/// Create a new embedded server with a custom config.
	pub fn with_config(config: ApiConfig) -> Self {
		let ctx = Arc::new(AppApiContext::new());
		let handler = Arc::new(AppApiHandler::new(ctx));
		let server = ApiServer::new(config.clone(), handler);
		Self { config, server: Some(server), handle: None }
	}

	/// Start the server in the background. Use `stop()` to shut down.
	pub async fn start(&mut self) -> Result<()> {
		// Initialize registries
		crate::plugin_system::init_global_registry();
		crate::lsp::init_global_registry(
			std::env::current_dir().unwrap_or_default().to_string_lossy().as_ref(),
		);

		if let Some(server) = self.server.as_mut() {
			let handle = server.start().await?;
			self.handle = Some(handle);
		}
		Ok(())
	}

	/// Request graceful shutdown.
	pub fn stop(&self) {
		if let Some(server) = &self.server {
			server.stop();
		}
	}

	/// Wait for the server to finish (block until stopped).
	pub async fn wait(&mut self) {
		if let Some(handle) = self.handle.take() {
			let _ = handle.await;
		}
	}

	/// Run the server and block until stopped (Ctrl+C or stop()).
	pub async fn run(&mut self) -> Result<()> {
		self.start().await?;

		let cancel = tokio::signal::ctrl_c();
		let wait = self.wait();

		tokio::select! {
				_ = cancel => {
						self.stop();
						Ok(())
				}
				_ = wait => {
						Ok(())
				}
		}
	}

	/// Start the server in a background tokio task, returning a handle
	/// that can be used to await or shutdown.
	pub fn spawn(mut self) -> tokio::task::JoinHandle<Result<()>> {
		tokio::spawn(async move { self.run().await })
	}

	/// Get a reference to the telemetry data.
	pub fn telemetry(&self) -> Option<&Telemetry> {
		self.server.as_ref().map(|s| s.telemetry())
	}
}

impl Default for EmbeddedServer {
	fn default() -> Self {
		Self::new()
	}
}

/// Helper: start the embedded server on the default port (10245) and run
/// until Ctrl+C is received.
pub async fn run_headless() -> Result<()> {
	let mut server = EmbeddedServer::default();
	server.run().await
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_embedded_server_creation() {
		let server = EmbeddedServer::new();
		assert!(server.handle.is_none());
	}

	#[test]
	fn test_embedded_server_with_custom_config() {
		let config =
			ApiConfig { host: "0.0.0.0".into(), port: 19999, auth_token: Some("test-token".into()) };
		let server = EmbeddedServer::with_config(config);
		assert_eq!(server.config.port, 19999);
	}

	#[test]
	fn test_embedded_server_default_impl() {
		let server = EmbeddedServer::default();
		assert!(server.handle.is_none());
	}
}
