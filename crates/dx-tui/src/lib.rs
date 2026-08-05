// Dead code is allowed per-module with targeted annotations or in specific
// modules where items are intentionally retained (e.g. public API surface,
// planned features, platform-specific stubs). The blanket allow was removed
// so new dead code will be caught by CI (clippy -D warnings).

// Windows uses the system allocator (which is fine — Windows has a good allocator).
#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
#[unsafe(global_allocator)]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

mod file_browser;
mod flow_backend;
mod menu;

mod codex;
mod codex_bridge;

mod agent_backend;
mod agent_loop;
mod agent_workspace;
mod animations;
mod api;
mod api_handler;
mod api_sdk;
mod background_review;
mod bottom_center;
mod bridge;
mod channel_actions;
mod channels;
mod chat_render;
mod command_palette;
mod compaction;
mod components;
mod diff_view;
mod dispatcher;
mod dx_system;
mod editor;
mod effects;
mod embedded;
mod file_tabs;
mod font;
mod goal_runner;
mod input;
mod learning_graph;
mod llm;
mod logs;
mod lsp;
mod lsp_tool;
mod mcp;
mod mcp_tool;
mod memory_provider;
mod memory_tool;
mod modes;
mod msg_ui;
mod notifications;
mod omniroute;
mod orchestration;
mod panic;
mod perf;
mod permission_hub;
mod plan_wizard;
mod plugin_system;
mod plugin_system_tool;
mod profile_prompts;
mod prompt_queue;
mod provider_registry;
mod providers;
mod question_hub;
mod root;
mod scheduler;
mod session_db;
mod session_meta;
mod session_search;
mod session_store;
mod sidebar_data;
mod signals;
mod skills;
mod slash_commands;
mod sound;
mod splash;
mod state;
mod stream_events;
mod subagent_registry;
mod theme;
mod token_save;
mod tools;
mod tui_prefs;
mod update_check;
mod vim_mode;
mod voice;
mod workspace_tools;
mod zen;

use logs::Logs;
use panic::Panic;
pub use root::TerminalRoot;
pub use state::AnimationType;

/// Env used to pass `dx continue <id>` into ChatState bootstrap.
pub(crate) const CONTINUE_SESSION_ENV: &str = "DX_TUI_CONTINUE_SESSION";

pub(crate) fn set_exit_continue_hint(cmd: impl Into<String>) {
	if let Ok(mut g) = fb_term::GOODBYE_MESSAGE.lock() {
		*g = Some(cmd.into());
	}
}

/// Parse CLI: `dx continue <session-id>` | `dx --continue <id>` | normal launch.
fn apply_cli_args() {
	let mut args = std::env::args().skip(1).peekable();
	while let Some(arg) = args.next() {
		match arg.as_str() {
			"continue" | "--continue" | "-c" => {
				if let Some(id) = args.next() {
					// Single-threaded at startup before UI.
					// SAFETY: only this process; ChatState::new reads once.
					unsafe {
						std::env::set_var(CONTINUE_SESSION_ENV, id);
					}
				}
			}
			"--help" | "-h" => {
				eprintln!(
					"dx — DX TUI\n\n\
					 Usage:\n\
					   dx                    Start a new session\n\
					   dx continue <id>      Resume a saved session\n\
					   dx --continue <id>    Same as continue\n"
				);
				std::process::exit(0);
			}
			_ => {}
		}
	}
}

pub fn run_main() -> anyhow::Result<()> {
	apply_cli_args();
	let runtime = tokio::runtime::Runtime::new()?;
	runtime.block_on(async {
		Panic::install();
		fb_shared::init();

		Logs::start()?;
		_ = fdlimit::raise_fd_limit();

		fb_tty::init();
		fb_term::init();
		fb_fs::init();
		fb_config::init()?;
		fb_vfs::init();
		fb_adapter::init()?;
		fb_boot::init();
		fb_dds::init();
		fb_widgets::init();
		fb_watcher::init();
		fb_plugin::init()?;
		fb_dds::serve();

		// Wire MCP and LSP into the global registries
		crate::plugin_system::init_global_registry();
		crate::lsp::init_global_registry(
			std::env::current_dir().unwrap_or_default().to_string_lossy().as_ref(),
		);
		if let Err(e) = crate::mcp::init_global_registry().await {
			tracing::warn!("MCP registry init failed (non-fatal): {e}");
		}

		// Drive the main app *outside* LocalSet so `tokio::task::block_in_place`
		// works (editor adapter + nested `Runtime::block_on` for file tree / LSP).
		// LocalSet still runs in parallel so file-browser Lua plugins can
		// `LOCAL_SET.spawn_local(...)` without requiring the main future to live
		// inside `run_until` (which sets `disallow_block_in_place` and panics with
		// "can call blocking only when running on the multi-threaded runtime").
		tokio::select! {
			biased;
			result = file_browser::app::App::serve() => result,
			() = fb_shared::LOCAL_SET.run_until(std::future::pending::<()>()) => {
				unreachable!("LocalSet driver should not complete while pending()")
			}
		}
	})?;

	Ok(())
}

pub mod markdown_render;
