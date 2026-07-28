// File browser module - contains the embedded DX TUI file-browser engine.

// File browser UI components
pub mod app;
pub mod cmp;
pub mod confirm;
pub mod help;
pub mod input;
pub mod mgr;
pub mod notify;
pub mod pick;
pub mod spot;
pub mod tasks;
pub mod which;

// Core functionality
pub mod executor;
pub mod router;

// Re-export commonly used items
pub use executor::Executor;
pub use router::Router;
