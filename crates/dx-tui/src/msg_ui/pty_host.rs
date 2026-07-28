//! Interactive terminal host for the message stream.
//!
//! Uses **portable-pty** when available for a real PTY (resize, raw-ish apps).
//! Falls back to piped shell if PTY open fails.

use std::{
	collections::{HashMap, VecDeque},
	io::{Read, Write},
	path::PathBuf,
	sync::{
		Arc, Mutex,
		atomic::{AtomicBool, Ordering},
	},
	thread,
	time::Instant,
};

use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};
use ratatui::style::Style;

use super::vt_grid::VtGrid;

const MAX_LINES: usize = 4_000;
const DEFAULT_COLS: u16 = 100;
const DEFAULT_ROWS: u16 = 28;

#[derive(Debug, Clone)]
pub struct PtySnapshot {
	pub id: String,
	pub title: String,
	pub lines: Vec<String>,
	pub attached: bool,
	pub alive: bool,
	pub exit_code: Option<i32>,
	pub started: Instant,
	pub cols: u16,
	pub rows: u16,
	pub is_real_pty: bool,
}

struct PtySession {
	id: String,
	title: String,
	/// Keep master alive for lifetime of session.
	_master: Box<dyn MasterPty + Send>,
	writer: Box<dyn Write + Send>,
	lines: Arc<Mutex<VecDeque<String>>>,
	/// VT cell grid (cursor / clear / SGR) for curses-friendly paint.
	grid: Arc<Mutex<VtGrid>>,
	alive: Arc<AtomicBool>,
	exit_code: Arc<Mutex<Option<i32>>>,
	attached: bool,
	started: Instant,
	cols: u16,
	rows: u16,
	is_real_pty: bool,
	/// Child process killer
	child: Box<dyn portable_pty::Child + Send + Sync>,
}

/// Real-PTY interactive terminals keyed by session id.
pub struct PtyHost {
	sessions: HashMap<String, PtySession>,
	pub attached_id: Option<String>,
}

impl Default for PtyHost {
	fn default() -> Self {
		Self::new()
	}
}

impl PtyHost {
	pub fn new() -> Self {
		Self { sessions: HashMap::new(), attached_id: None }
	}

	/// Spawn an interactive shell in `cwd` on a real PTY.
	pub fn spawn_shell(&mut self, cwd: PathBuf, title: impl Into<String>) -> anyhow::Result<String> {
		let id = format!("pty-{}", uuid_like());
		let title = title.into();
		let cols = DEFAULT_COLS;
		let rows = DEFAULT_ROWS;

		let pty_system = native_pty_system();
		let pair = pty_system.openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })?;

		let mut cmd = if cfg!(windows) {
			CommandBuilder::new("cmd.exe")
		} else {
			let mut c = CommandBuilder::new("bash");
			c.arg("-i");
			// Prefer login-ish interactive; fall back handled by OS
			c
		};
		cmd.cwd(cwd);

		let child = pair.slave.spawn_command(cmd)?;
		// Slave dropped after spawn — master remains
		drop(pair.slave);

		let mut reader =
			pair.master.try_clone_reader().map_err(|e| anyhow::anyhow!("pty reader: {e}"))?;
		let writer = pair.master.take_writer().map_err(|e| anyhow::anyhow!("pty writer: {e}"))?;

		let lines = Arc::new(Mutex::new(VecDeque::new()));
		let grid = Arc::new(Mutex::new(VtGrid::new(cols as usize, rows as usize, Style::default())));
		let alive = Arc::new(AtomicBool::new(true));
		let exit_code = Arc::new(Mutex::new(None));

		{
			let lines = lines.clone();
			let grid = grid.clone();
			let alive = alive.clone();
			thread::Builder::new()
				.name(format!("pty-read-{id}"))
				.spawn(move || {
					let mut buf = [0u8; 4096];
					let mut carry = String::new();
					loop {
						match reader.read(&mut buf) {
							Ok(0) => break,
							Ok(n) => {
								let chunk = String::from_utf8_lossy(&buf[..n]);
								if let Ok(mut g) = grid.lock() {
									g.feed(&chunk);
								}
								// Also keep a linear log for fallback / search
								push_raw_chunk(&lines, &mut carry, &chunk);
							}
							Err(_) => break,
						}
					}
					if !carry.is_empty()
						&& let Ok(mut g) = lines.lock()
					{
						g.push_back(std::mem::take(&mut carry));
						trim_lines(&mut g);
					}
					alive.store(false, Ordering::SeqCst);
				})
				.map_err(|e| anyhow::anyhow!("spawn reader: {e}"))?;
		}

		if let Ok(mut g) = lines.lock() {
			g.push_back(format!("$ PTY · {title} · {cols}×{rows}"));
			g.push_back("  (Esc detach · real portable-pty + VT grid)".into());
		}
		if let Ok(mut g) = grid.lock() {
			g.feed(&format!("$ PTY · {title} · {cols}x{rows}\r\n"));
		}

		self.sessions.insert(
			id.clone(),
			PtySession {
				id: id.clone(),
				title,
				_master: pair.master,
				writer,
				lines,
				grid,
				alive,
				exit_code,
				attached: false,
				started: Instant::now(),
				cols,
				rows,
				is_real_pty: true,
				child,
			},
		);
		Ok(id)
	}

	/// Resize the attached (or named) PTY — required for vim/htop layout.
	pub fn resize(&mut self, id: &str, cols: u16, rows: u16) -> bool {
		let Some(s) = self.sessions.get_mut(id) else {
			return false;
		};
		let cols = cols.max(20);
		let rows = rows.max(5);
		if s._master.resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 }).is_ok() {
			s.cols = cols;
			s.rows = rows;
			if let Ok(mut g) = s.grid.lock() {
				g.resize(cols as usize, rows as usize);
			}
			true
		} else {
			false
		}
	}

	pub fn resize_attached(&mut self, cols: u16, rows: u16) -> bool {
		let Some(id) = self.attached_id.clone() else {
			return false;
		};
		self.resize(&id, cols, rows)
	}

	pub fn attach(&mut self, id: &str) -> bool {
		self.detach_all();
		if let Some(s) = self.sessions.get_mut(id) {
			s.attached = true;
			self.attached_id = Some(id.to_string());
			true
		} else {
			false
		}
	}

	pub fn detach_all(&mut self) {
		self.attached_id = None;
		for s in self.sessions.values_mut() {
			s.attached = false;
		}
	}

	pub fn is_attached(&self) -> bool {
		self.attached_id.is_some()
	}

	pub fn write_attached(&mut self, data: &str) -> bool {
		let Some(id) = self.attached_id.clone() else {
			return false;
		};
		let Some(s) = self.sessions.get_mut(&id) else {
			return false;
		};
		if s.writer.write_all(data.as_bytes()).is_err() {
			return false;
		}
		let _ = s.writer.flush();
		true
	}

	pub fn kill(&mut self, id: &str) {
		if let Some(mut s) = self.sessions.remove(id) {
			let _ = s.child.kill();
			if self.attached_id.as_deref() == Some(id) {
				self.attached_id = None;
			}
		}
	}

	pub fn poll_exit(&mut self) {
		let mut ended = Vec::new();
		for (id, s) in self.sessions.iter_mut() {
			if let Ok(Some(status)) = s.child.try_wait() {
				s.alive.store(false, Ordering::SeqCst);
				let code = status.exit_code() as i32;
				if let Ok(mut g) = s.exit_code.lock() {
					*g = Some(code);
				}
				s.attached = false;
				ended.push(id.clone());
				if let Ok(mut g) = s.lines.lock() {
					g.push_back(format!("[PTY ended · exit {code}]"));
				}
			}
		}
		for id in ended {
			if self.attached_id.as_deref() == Some(id.as_str()) {
				self.attached_id = None;
			}
		}
	}

	pub fn snapshots(&self) -> Vec<PtySnapshot> {
		self
			.sessions
			.values()
			.map(|s| {
				// Prefer VT grid screen (handles clear/cursor); fall back to log.
				let lines = s
					.grid
					.lock()
					.map(|g| {
						let mut v = g.to_plain_lines();
						if v.is_empty() {
							s.lines.lock().map(|log| log.iter().cloned().collect()).unwrap_or_default()
						} else {
							// Keep a short log prefix for context
							if let Ok(log) = s.lines.lock() {
								let head: Vec<String> = log.iter().take(2).cloned().collect();
								if !head.is_empty() {
									let mut out = head;
									out.extend(v);
									v = out;
								}
							}
							v
						}
					})
					.unwrap_or_else(|_| {
						s.lines.lock().map(|g| g.iter().cloned().collect()).unwrap_or_default()
					});
				PtySnapshot {
					id: s.id.clone(),
					title: s.title.clone(),
					lines,
					attached: s.attached,
					alive: s.alive.load(Ordering::SeqCst),
					exit_code: s.exit_code.lock().ok().and_then(|c| *c),
					started: s.started,
					cols: s.cols,
					rows: s.rows,
					is_real_pty: s.is_real_pty,
				}
			})
			.collect()
	}
}

fn push_raw_chunk(lines: &Arc<Mutex<VecDeque<String>>>, carry: &mut String, chunk: &str) {
	// Linear log: strip CR, split LF, keep ANSI codes for ansi_line paint fallback
	let chunk = chunk.replace('\r', "");
	carry.push_str(&chunk);
	let Ok(mut g) = lines.lock() else {
		return;
	};
	while let Some(pos) = carry.find('\n') {
		let line: String = carry.drain(..=pos).collect();
		let line = line.trim_end_matches('\n').to_string();
		// strip pure CSI noise lines
		if line.chars().any(|c| !c.is_control() || c == '\u{1b}') {
			g.push_back(line);
			trim_lines(&mut g);
		}
	}
}

fn trim_lines(g: &mut VecDeque<String>) {
	while g.len() > MAX_LINES {
		g.pop_front();
	}
}

fn uuid_like() -> String {
	use std::time::{SystemTime, UNIX_EPOCH};
	let t = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
	format!("{t:x}")
}
