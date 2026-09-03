//! Lua-based plugin system for dx-tui.

#![allow(dead_code)]
//!
//! Plugins live under `~/.config/dx/plugins/<name>/` with a `manifest.toml`
//! and a Lua entry-point.  The sandboxed runtime exposes a `dx.*` API for
//! registering tools, hooks, and context sources.

use std::{
	collections::HashMap,
	fs,
	path::{Path, PathBuf},
	sync::{Arc, Mutex, OnceLock},
	time::Duration,
};

use anyhow::Result;
use mlua::{Lua, LuaSerdeExt, RegistryKey, Value as LuaValue};
use notify::Watcher;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{error, info, warn};

// ── Globals ─────────────────────────────────────────────────────────────

static GLOBAL_PLUGIN_REGISTRY: OnceLock<PluginRegistry> = OnceLock::new();

pub fn global_registry() -> &'static PluginRegistry {
	GLOBAL_PLUGIN_REGISTRY
		.get()
		.expect("PluginRegistry not initialised — call init_global_registry() first")
}

pub fn try_global_registry() -> Option<&'static PluginRegistry> {
	GLOBAL_PLUGIN_REGISTRY.get()
}

pub fn init_global_registry() {
	GLOBAL_PLUGIN_REGISTRY.get_or_init(|| {
		let reg = PluginRegistry::new();
		if let Err(e) = reg.discover_all() {
			warn!("Plugin discovery: {e}");
		}
		reg
	});
	info!("Plugin registry initialised");
}

// ── Errors ──────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum PluginError {
	#[error("Plugin not found: {0}")]
	NotFound(String),
	#[error("Plugin already loaded: {0}")]
	AlreadyLoaded(String),
	#[error("Load failed: {0}")]
	LoadFailed(String),
	#[error("Lua error: {0}")]
	Lua(#[from] mlua::Error),
	#[error("I/O error: {0}")]
	Io(#[from] std::io::Error),
	#[error("TOML parse error: {0}")]
	Parse(#[from] toml::de::Error),
	#[error("Hook not found: {0}")]
	HookNotFound(String),
	#[error("Tool not found: {0}")]
	ToolNotFound(String),
	#[error("{0}")]
	Msg(String),
}

// ── Types ───────────────────────────────────────────────────────────────

/// Tool entry from manifest.toml (static definition).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginToolDef {
	pub name: String,
	pub description: String,
	#[serde(default)]
	pub parameters: Value,
	#[serde(default)]
	pub permission_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
	pub name: String,
	pub version: String,
	pub description: String,
	pub author: String,
	#[serde(default = "default_entry_point")]
	pub entry_point: String,
	#[serde(default)]
	pub permissions: Vec<String>,
	#[serde(default)]
	pub tools: Vec<PluginToolDef>,
	#[serde(default)]
	pub hooks: Vec<String>,
}

fn default_entry_point() -> String {
	"main.lua".into()
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginScreenInfo {
	pub plugin_name: String,
	pub screen_name: String,
	pub title: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginState {
	Disabled,
	Enabled,
	Error,
}

impl PluginState {
	pub fn label(self) -> &'static str {
		match self {
			Self::Disabled => "disabled",
			Self::Enabled => "enabled",
			Self::Error => "error",
		}
	}
}

#[derive(Debug, Clone, Serialize)]
pub struct PluginInfo {
	pub name: String,
	pub version: String,
	pub description: String,
	pub author: String,
	pub state: String,
	pub tool_count: usize,
	pub hook_count: usize,
	pub screen_count: usize,
}

// ── Plugin ──────────────────────────────────────────────────────────────

pub struct Plugin {
	pub manifest: PluginManifest,
	pub state: PluginState,
	pub plugin_dir: PathBuf,
	lua: Lua,
	/// Tool name → RegistryKey for the Lua handler.
	tool_handlers: HashMap<String, RegistryKey>,
	/// Event name → list of handler registry keys.
	hook_handlers: HashMap<String, Vec<RegistryKey>>,
	/// Screen name → RegistryKey for the Lua render function.
	screen_handlers: HashMap<String, RegistryKey>,
	/// Arc-wrapped tool definitions for cheap sharing.
	tool_def_arcs: Vec<Arc<PluginToolDef>>,
}

impl Plugin {
	fn new(manifest: PluginManifest, plugin_dir: PathBuf) -> Result<Self> {
		let tool_def_arcs = manifest.tools.iter().map(|td| Arc::new(td.clone())).collect();
		let lua = Lua::new();
		let mut p = Self {
			manifest,
			state: PluginState::Enabled,
			plugin_dir,
			lua,
			tool_handlers: HashMap::new(),
			hook_handlers: HashMap::new(),
			screen_handlers: HashMap::new(),
			tool_def_arcs,
		};
		p.apply_sandbox()?;
		Ok(p)
	}

	/// Strip dangerous globals and inject `dx.*` API.
	fn apply_sandbox(&mut self) -> Result<()> {
		let globals = self.lua.globals();

		let remove =
			["io", "os", "loadfile", "dofile", "require", "package", "debug", "rawget", "rawset"];
		for key in &remove {
			let _ = globals.set(*key, LuaValue::Nil);
		}

		// dx.plugin info
		let pname = self.manifest.name.clone();
		let pversion = self.manifest.version.clone();
		let pdesc = self.manifest.description.clone();
		let pauthor = self.manifest.author.clone();

		// Create internal accumulator tables (scanned after loading)
		let _reg_tools = self.lua.create_table()?;
		let _reg_hooks = self.lua.create_table()?;
		globals.set("__dx_tools", _reg_tools.clone())?;
		globals.set("__dx_hooks", _reg_hooks.clone())?;

		let dx = self.lua.create_table()?;

		// dx.log(level, message)
		let log_fn = self.lua.create_function(|_, (level, msg): (String, String)| {
			match level.as_str() {
				"info" => info!("[plugin] {msg}"),
				"warn" => warn!("[plugin] {msg}"),
				"error" => error!("[plugin] {msg}"),
				_ => info!("[plugin] {msg}"),
			}
			Ok(())
		})?;
		dx.set("log", log_fn)?;

		// dx.plugin
		let pt = self.lua.create_table()?;
		pt.set("name", pname)?;
		pt.set("version", pversion)?;
		pt.set("description", pdesc)?;
		pt.set("author", pauthor)?;
		dx.set("plugin", pt)?;

		// dx.tools.register(name, description, parameters, handler_fn)
		let tools_reg = _reg_tools.clone();
		let register_fn = self.lua.create_function(
			move |lua, (name, desc, params, handler): (String, String, mlua::Value, mlua::Function)| {
				let entry = lua.create_table()?;
				entry.set("name", name.clone())?;
				entry.set("description", desc)?;
				entry.set("parameters", params)?;
				entry.set("handler", handler)?;
				tools_reg.set(name, entry)?;
				Ok(())
			},
		)?;
		let tools_tbl = self.lua.create_table()?;
		tools_tbl.set("register", register_fn)?;
		dx.set("tools", tools_tbl)?;

		// dx.hooks.on(event, handler_fn)
		let hooks_reg = _reg_hooks.clone();
		let hook_count = std::cell::Cell::new(0u64);
		let hook_fn =
			self.lua.create_function(move |lua, (event, handler): (String, mlua::Function)| {
				let entry = lua.create_table()?;
				entry.set("event", event.clone())?;
				entry.set("handler", handler)?;
				let idx = hook_count.get();
				hook_count.set(idx + 1);
				hooks_reg.set(format!("{event}_{idx}"), entry)?;
				Ok(())
			})?;
		let hooks_tbl = self.lua.create_table()?;
		hooks_tbl.set("on", hook_fn)?;
		dx.set("hooks", hooks_tbl)?;

		// dx.http.request(method, url, headers?, body?) -> (status, headers, body)
		let http_tbl = self.lua.create_table()?;
		let http_fn = self.lua.create_async_function(
			|_,
			 (method, url, headers, body): (
				String,
				String,
				Option<HashMap<String, String>>,
				Option<String>,
			)| async move {
				let client = reqwest::Client::builder()
					.timeout(std::time::Duration::from_secs(30))
					.build()
					.map_err(mlua::Error::external)?;
				let req = match method.to_uppercase().as_str() {
					"GET" => client.get(&url),
					"POST" => client.post(&url),
					"PUT" => client.put(&url),
					"PATCH" => client.patch(&url),
					"DELETE" => client.delete(&url),
					"HEAD" => client.head(&url),
					_ => return Err(mlua::Error::external(format!("unsupported method: {method}"))),
				};
				let req = if let Some(h) = headers {
					let mut r = req;
					for (k, v) in h {
						r = r.header(&k, &v);
					}
					r
				} else {
					req
				};
				let req = if let Some(b) = body { req.body(b) } else { req };
				let resp = req.send().await.map_err(mlua::Error::external)?;
				let status: u16 = resp.status().as_u16();
				let resp_headers: HashMap<String, String> = resp
					.headers()
					.iter()
					.map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
					.collect();
				let resp_body = resp.text().await.map_err(mlua::Error::external)?;
				Ok((status, resp_headers, resp_body))
			},
		)?;
		http_tbl.set("request", http_fn)?;
		dx.set("http", http_tbl)?;

		// dx.state — per-plugin k/v store
		let state = self.lua.create_table()?;
		{
			let st = state.clone();
			let get_fn = self.lua.create_function(move |_, key: String| {
				Ok(st.get::<LuaValue>(key.clone()).unwrap_or(LuaValue::Nil))
			})?;
			state.set("get", get_fn)?;
		}
		{
			let st = state.clone();
			let set_fn = self.lua.create_function(move |_, (key, val): (String, LuaValue)| {
				st.set(key.clone(), val).map_err(mlua::Error::external)?;
				Ok(())
			})?;
			state.set("set", set_fn)?;
		}
		dx.set("state", state)?;

		// dx.ui.register_screen(name, title, render_fn) — TUI plugin routes
		let ui_tbl = self.lua.create_table()?;
		let ui_screens_reg = self.lua.create_table()?;
		globals.set("__dx_ui_screens", ui_screens_reg.clone())?;
		let register_screen_fn = self.lua.create_function(
			move |lua, (name, title, render_fn): (String, String, mlua::Function)| {
				let entry = lua.create_table()?;
				entry.set("screen_name", name.clone())?;
				entry.set("title", title)?;
				entry.set("handler", render_fn)?;
				ui_screens_reg.set(name, entry)
			},
		)?;
		ui_tbl.set("register_screen", register_screen_fn)?;
		dx.set("ui", ui_tbl)?;

		globals.set("dx", dx)?;
		Ok(())
	}

	/// Execute the entry-point script.
	fn load_entry(&mut self) -> Result<()> {
		let entry = self.plugin_dir.join(&self.manifest.entry_point);
		if !entry.exists() {
			return Err(
				PluginError::LoadFailed(format!("entry point not found: {}", entry.display())).into(),
			);
		}
		let code = fs::read_to_string(&entry)?;
		self.lua.load(&code).set_name(&self.manifest.name).exec()?;

		// Scan accumulated registrations
		self.scan_registrations()?;
		Ok(())
	}

	/// After loading the entry script, read __dx_tools, __dx_hooks, and __dx_ui_screens
	/// and populate the Rust-side handler maps.
	fn scan_registrations(&mut self) -> Result<()> {
		let globals = self.lua.globals();

		if let Ok(tools_tbl) = globals.get::<mlua::Table>("__dx_tools") {
			for pair in tools_tbl.pairs::<String, mlua::Table>() {
				let (name, entry) = pair?;
				let handler: mlua::Function = entry.get("handler")?;
				let key = self.lua.create_registry_value(handler)?;
				self.tool_handlers.insert(name, key);
			}
		}

		if let Ok(hooks_tbl) = globals.get::<mlua::Table>("__dx_hooks") {
			for pair in hooks_tbl.pairs::<String, mlua::Table>() {
				let (_id, entry) = pair?;
				let event: String = entry.get("event")?;
				let handler: mlua::Function = entry.get("handler")?;
				let key = self.lua.create_registry_value(handler)?;
				self.hook_handlers.entry(event).or_default().push(key);
			}
		}

		if let Ok(screens_tbl) = globals.get::<mlua::Table>("__dx_ui_screens") {
			for pair in screens_tbl.pairs::<String, mlua::Table>() {
				let (name, entry) = pair?;
				let handler: mlua::Function = entry.get("handler")?;
				let key = self.lua.create_registry_value(handler)?;
				self.screen_handlers.insert(name, key);
			}
		}

		Ok(())
	}

	/// Call a registered tool handler.
	fn call_tool(&self, name: &str, args: Value) -> Result<Value> {
		let key = self.tool_handlers.get(name).ok_or_else(|| PluginError::ToolNotFound(name.into()))?;
		let handler: mlua::Function = self.lua.registry_value(key)?;
		let lua_args = self.lua.to_value(&args)?;
		let result: LuaValue = handler.call(lua_args)?;
		let json: Value = self.lua.from_value(result)?;
		Ok(json)
	}

	/// Call all handlers registered for a hook event.
	fn call_hook(&self, event: &str, payload: &Value) -> Result<Vec<Value>> {
		let Some(keys) = self.hook_handlers.get(event) else {
			return Ok(Vec::new());
		};
		let lua_payload = self.lua.to_value(payload)?;
		let mut results = Vec::new();
		for key in keys {
			let handler: mlua::Function = self.lua.registry_value(key)?;
			let result: LuaValue = handler.call(lua_payload.clone())?;
			if let Ok(jv) = self.lua.from_value::<Value>(result) {
				results.push(jv);
			}
		}
		Ok(results)
	}

	fn info(&self) -> PluginInfo {
		PluginInfo {
			name: self.manifest.name.clone(),
			version: self.manifest.version.clone(),
			description: self.manifest.description.clone(),
			author: self.manifest.author.clone(),
			state: self.state.label().into(),
			tool_count: self.tool_handlers.len(),
			hook_count: self.hook_handlers.values().map(|v| v.len()).sum(),
			screen_count: self.screen_handlers.len(),
		}
	}
}

unsafe impl Send for Plugin {}

// ── Registry ────────────────────────────────────────────────────────────

pub(crate) struct RegistryInner {
	plugins: HashMap<String, Plugin>,
	/// Tool name → plugin name lookup.
	tool_index: HashMap<String, String>,
	/// Screen name → plugin name lookup.
	screen_index: HashMap<String, String>,
}

pub struct PluginRegistry {
	inner: Arc<Mutex<RegistryInner>>,
}

impl PluginRegistry {
	pub fn new() -> Self {
		Self {
			inner: Arc::new(Mutex::new(RegistryInner {
				plugins: HashMap::new(),
				tool_index: HashMap::new(),
				screen_index: HashMap::new(),
			})),
		}
	}

	// ── Discovery ───────────────────────────────────────────────────

	pub fn discover_all(&self) -> Result<usize> {
		let dir = plugins_dir();
		if !dir.exists() {
			fs::create_dir_all(&dir)?;
			return Ok(0);
		}
		let mut count = 0;
		for entry in fs::read_dir(&dir)? {
			let entry = entry?;
			let path = entry.path();
			if !path.is_dir() {
				continue;
			}
			let manifest_path = path.join("manifest.toml");
			if !manifest_path.exists() {
				continue;
			}
			let name = path.file_name().unwrap().to_string_lossy().to_string();
			if let Err(e) = self.load_plugin_dir(&path, &name) {
				warn!("Failed to load plugin '{name}': {e}");
			} else {
				count += 1;
			}
		}
		info!("Discovered {count} plugin(s) from {}", dir.display());
		Ok(count)
	}

	pub fn load_plugin_dir(&self, dir: &Path, name: &str) -> Result<()> {
		let manifest_path = dir.join("manifest.toml");
		let text = fs::read_to_string(&manifest_path)?;
		let manifest: PluginManifest = toml::from_str(&text)?;
		if manifest.name != name {
			return Err(
				PluginError::LoadFailed(format!(
					"manifest name '{}' does not match dir '{name}'",
					manifest.name
				))
				.into(),
			);
		}
		self.load_plugin(manifest, dir.to_path_buf())
	}

	pub fn load_plugin(&self, manifest: PluginManifest, dir: PathBuf) -> Result<()> {
		let mut plugin = Plugin::new(manifest, dir)?;
		plugin.load_entry()?;
		let name = plugin.manifest.name.clone();

		// Collect tool and screen names to remove old entries
		let (remove_tools, remove_screens) = {
			let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
			let old = inner.plugins.get(&name);
			(
				old.map(|o| o.tool_handlers.keys().cloned().collect::<Vec<_>>()).unwrap_or_default(),
				old.map(|o| o.screen_handlers.keys().cloned().collect::<Vec<_>>()).unwrap_or_default(),
			)
		};

		let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
		for t in remove_tools {
			inner.tool_index.remove(&t);
		}
		for s in remove_screens {
			inner.screen_index.remove(&s);
		}

		let tool_names: Vec<String> = plugin.tool_handlers.keys().cloned().collect();
		for tool_name in tool_names {
			inner.tool_index.insert(tool_name, name.clone());
		}

		let screen_names: Vec<String> = plugin.screen_handlers.keys().cloned().collect();
		for screen_name in screen_names {
			inner.screen_index.insert(screen_name, name.clone());
		}

		inner.plugins.insert(name, plugin);
		Ok(())
	}

	// ── Lifecycle ───────────────────────────────────────────────────

	pub fn enable(&self, name: &str) -> Result<()> {
		let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
		let tool_names: Vec<String> = {
			let p = inner.plugins.get_mut(name).ok_or_else(|| PluginError::NotFound(name.into()))?;
			p.state = PluginState::Enabled;
			p.tool_handlers.keys().cloned().collect()
		};
		for t in tool_names {
			inner.tool_index.insert(t, name.into());
		}
		Ok(())
	}

	pub fn disable(&self, name: &str) -> Result<()> {
		let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
		let tool_names: Vec<String> = {
			let p = inner.plugins.get_mut(name).ok_or_else(|| PluginError::NotFound(name.into()))?;
			p.state = PluginState::Disabled;
			p.tool_handlers.keys().cloned().collect()
		};
		for t in tool_names {
			inner.tool_index.remove(&t);
		}
		Ok(())
	}

	pub fn reload(&self, name: &str) -> Result<()> {
		let dir = {
			let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
			inner.plugins.get(name).ok_or_else(|| PluginError::NotFound(name.into()))?.plugin_dir.clone()
		};
		self.load_plugin_dir(&dir, name)?;
		info!("Reloaded plugin '{name}'");
		Ok(())
	}

	pub fn reload_all(&self) -> Result<usize> {
		let names: Vec<String> = {
			let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
			inner.plugins.keys().cloned().collect()
		};
		let mut count = 0;
		for name in &names {
			if self.reload(name).is_ok() {
				count += 1;
			}
		}
		Ok(count)
	}

	// ── Queries ─────────────────────────────────────────────────────

	pub fn list_plugins(&self) -> Vec<PluginInfo> {
		let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
		inner.plugins.values().map(|p| p.info()).collect()
	}

	pub fn plugin_info(&self, name: &str) -> Option<PluginInfo> {
		let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
		inner.plugins.get(name).map(|p| p.info())
	}

	pub fn is_plugin_tool(&self, name: &str) -> bool {
		let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
		inner.tool_index.contains_key(name)
	}

	/// List all registered TUI plugin screens.
	pub fn list_screens(&self) -> Vec<PluginScreenInfo> {
		let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
		let mut screens = Vec::new();
		for (screen_name, plugin_name) in &inner.screen_index {
			if let Some(plugin) = inner.plugins.get(plugin_name) {
				if plugin.state != PluginState::Enabled {
					continue;
				}
				screens.push(PluginScreenInfo {
					plugin_name: plugin_name.clone(),
					screen_name: screen_name.clone(),
					title: screen_name.clone(),
				});
			}
		}
		screens.sort_by(|a, b| a.plugin_name.cmp(&b.plugin_name));
		screens
	}

	/// Check if a given screen name is registered by any plugin.
	pub fn has_screen(&self, name: &str) -> bool {
		let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
		inner.screen_index.contains_key(name)
	}

	/// Render a plugin screen by name. Returns the rendered content as a JSON value.
	/// The render function receives `(width, height)` and should return a string of text to display.
	pub fn render_screen(&self, screen_name: &str, width: u16, height: u16) -> Result<String> {
		let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
		let plugin_name = inner
			.screen_index
			.get(screen_name)
			.ok_or_else(|| PluginError::NotFound(screen_name.into()))?;
		let plugin =
			inner.plugins.get(plugin_name).ok_or_else(|| PluginError::NotFound(plugin_name.clone()))?;
		if plugin.state != PluginState::Enabled {
			return Err(PluginError::Msg(format!("plugin '{plugin_name}' is disabled")).into());
		}
		let key = plugin
			.screen_handlers
			.get(screen_name)
			.ok_or_else(|| PluginError::NotFound(screen_name.into()))?;
		let handler: mlua::Function = plugin.lua.registry_value(key)?;
		let result: mlua::String = handler.call((width, height))?;
		Ok(result.to_str()?.to_string())
	}

	/// Returns (plugin_name, PluginToolDef) for all plugin-registered tools.
	pub fn plugin_tool_defs(&self) -> Vec<(String, Arc<PluginToolDef>)> {
		let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
		let mut defs = Vec::new();
		for (tool_name, plugin_name) in &inner.tool_index {
			if let Some(plugin) = inner.plugins.get(plugin_name) {
				if plugin.state != PluginState::Enabled {
					continue;
				}
				// Try manifest definition first, then synthesize from handler
				if let Some(td) = plugin.tool_def_arcs.iter().find(|t| t.name == *tool_name) {
					defs.push((plugin_name.clone(), Arc::clone(td)));
				} else {
					defs.push((
						plugin_name.clone(),
						Arc::new(PluginToolDef {
							name: tool_name.clone(),
							description: String::new(),
							parameters: serde_json::json!({}),
							permission_required: false,
						}),
					));
				}
			}
		}
		defs
	}

	// ── Execution ───────────────────────────────────────────────────

	pub fn execute_tool(&self, tool_name: &str, args: &Value) -> Result<Value> {
		let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
		let plugin_name =
			inner.tool_index.get(tool_name).ok_or_else(|| PluginError::ToolNotFound(tool_name.into()))?;
		let plugin =
			inner.plugins.get(plugin_name).ok_or_else(|| PluginError::NotFound(plugin_name.clone()))?;
		if plugin.state != PluginState::Enabled {
			return Err(PluginError::Msg(format!("plugin '{plugin_name}' is disabled")).into());
		}
		plugin.call_tool(tool_name, args.clone())
	}

	/// Fire a hook across all enabled plugins.
	pub fn fire_hook(&self, event: &str, payload: &Value) -> Vec<Result<Value>> {
		let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
		let mut results = Vec::new();
		for plugin in inner.plugins.values() {
			if plugin.state != PluginState::Enabled {
				continue;
			}
			match plugin.call_hook(event, payload) {
				Ok(vals) => results.extend(vals.into_iter().map(Ok)),
				Err(e) => results.push(Err(e)),
			}
		}
		results
	}

	/// Collect context snippets from plugins that registered a `context.provide` hook.
	pub fn plugin_contexts(&self) -> Vec<(String, String)> {
		let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
		let mut ctx = Vec::new();
		for (name, plugin) in &inner.plugins {
			if plugin.state != PluginState::Enabled {
				continue;
			}
			if !plugin.hook_handlers.contains_key("context.provide") {
				continue;
			}
			if let Ok(results) = plugin.call_hook("context.provide", &serde_json::json!({})) {
				for r in results {
					if let Some(text) = r.as_str() {
						ctx.push((name.clone(), text.to_string()));
					}
				}
			}
		}
		ctx
	}

	pub fn inner(&self) -> &Arc<Mutex<RegistryInner>> {
		&self.inner
	}
}

// ── Helpers ─────────────────────────────────────────────────────────────

fn plugins_dir() -> PathBuf {
	dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".config").join("dx").join("plugins")
}

/// Start a background file-watcher that triggers hot-reload.
pub fn start_hot_reload_watcher() -> Result<()> {
	let dir = plugins_dir();
	if !dir.exists() {
		fs::create_dir_all(&dir)?;
	}

	let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
	let mut watcher = notify::RecommendedWatcher::new(
		move |event: notify::Result<notify::Event>| {
			if let Ok(ev) = event {
				let _ = tx.send(ev);
			}
		},
		notify::Config::default().with_poll_interval(Duration::from_secs(2)),
	)
	.map_err(|e| PluginError::Msg(format!("file watcher error: {e}")))?;

	watcher
		.watch(&dir, notify::RecursiveMode::Recursive)
		.map_err(|e| PluginError::Msg(format!("cannot watch {dir:?}: {e}")))?;

	// Leak the watcher so it lives for the program lifetime
	std::mem::forget(watcher);

	tokio::spawn(async move {
		while let Some(event) = rx.recv().await {
			tokio::time::sleep(Duration::from_millis(500)).await;
			for path in &event.paths {
				let Some(parent) = path.parent() else { continue };
				let Some(name) = parent.file_name() else { continue };
				let name = name.to_string_lossy().to_string();
				if let Err(e) = global_registry().reload(&name) {
					debug!("hot-reload '{name}': {e}");
				}
			}
		}
	});

	info!("Plugin hot-reload watcher started for {dir:?}");
	Ok(())
}

// Use tracing's debug macro
use tracing::debug;

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_default_entry_point() {
		assert_eq!(default_entry_point(), "main.lua");
	}

	#[test]
	fn test_manifest_parse() {
		let toml = r#"
name = "test-plugin"
version = "1.0.0"
description = "Test"
author = "Tester"
entry_point = "init.lua"
permissions = ["network"]
tools = []
hooks = ["session.start"]
"#;
		let m: PluginManifest = toml::from_str(toml).unwrap();
		assert_eq!(m.name, "test-plugin");
		assert_eq!(m.entry_point, "init.lua");
		assert!(m.permissions.contains(&"network".into()));
	}

	#[test]
	fn test_plugins_dir() {
		let d = plugins_dir();
		assert!(d.to_string_lossy().replace('\\', "/").contains("dx/plugins"));
	}

	#[test]
	fn test_registry_new() {
		let r = PluginRegistry::new();
		assert!(r.list_plugins().is_empty());
	}
}
