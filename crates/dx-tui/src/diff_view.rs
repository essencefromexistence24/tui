//! Full-screen git differ: collapsible file tree (left) + unified diff (right).

use std::{
	collections::HashSet,
	path::{Component, Path},
	process::Command,
	sync::OnceLock,
};

use ratatui::{
	buffer::Buffer,
	layout::{Constraint, Direction, Layout, Rect},
	style::{Color, Modifier, Style},
	text::{Line, Span},
	widgets::{Block, Borders, Paragraph, Widget},
};
use syntect::{easy::HighlightLines, highlighting::ThemeSet, parsing::SyntaxSet};

use crate::theme::ChatTheme;

/// Lazy-loaded syntect syntax set + theme for multi-language highlighting.
pub(crate) fn syntect_engine() -> &'static (SyntaxSet, syntect::highlighting::Theme) {
	static ENGINE: OnceLock<(SyntaxSet, syntect::highlighting::Theme)> = OnceLock::new();
	ENGINE.get_or_init(|| {
		let ss = SyntaxSet::load_defaults_newlines();
		let ts = ThemeSet::load_defaults();
		// Prefer a dark theme that reads well on our dark TUI bg
		let theme = ts
			.themes
			.get("base16-ocean.dark")
			.or_else(|| ts.themes.get("InspiredGitHub"))
			.or_else(|| ts.themes.values().next())
			.cloned()
			.unwrap_or_else(|| ThemeSet::load_defaults().themes.into_values().next().unwrap());
		(ss, theme)
	})
}

/// One file with a unified diff hunk set.
#[derive(Debug, Clone)]
pub struct DiffFile {
	pub path: String,
	pub additions: usize,
	pub deletions: usize,
	pub patch: String,
}

/// A single paint row in the right-hand patch pane (code only — no hunk headers).
#[derive(Debug, Clone)]
pub struct DiffDisplayRow {
	pub kind: DiffRowKind,
	pub line_no: usize,
	pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffRowKind {
	Add,
	Del,
	Context,
}

/// Tree node for the left file browser.
#[derive(Debug, Clone)]
pub enum TreeNode {
	Dir {
		name: String,
		children: Vec<TreeNode>,
		expanded: bool,
	},
	File {
		name: String,
		/// Index into `DiffState.files`.
		file_index: usize,
	},
}

#[derive(Debug, Clone, Default)]
pub struct DiffState {
	pub open: bool,
	pub files: Vec<DiffFile>,
	pub tree: Vec<TreeNode>,
	/// Selected file index in `files`.
	pub selected_file: usize,
	/// Cursor row in the flattened tree (for keyboard nav).
	pub tree_cursor: usize,
	pub diff_scroll: usize,
	pub tree_scroll: usize,
	pub focus_tree: bool,
	pub total_additions: usize,
	pub total_deletions: usize,
	pub error: Option<String>,
	pub last_refresh: Option<std::time::Instant>,
	/// Hit-test rects filled by the last render (outer bordered panes).
	pub tree_area: Rect,
	pub patch_area: Rect,
	/// Inner content of the file tree (inside borders).
	pub tree_inner: Rect,
	/// Inner content of the patch pane (inside borders + topbar).
	pub patch_inner: Rect,
	/// Hover highlight for file-tree scrollbar (editor-style).
	pub tree_scrollbar_hovered: bool,
	/// Hover highlight for patch scrollbar (editor-style).
	pub patch_scrollbar_hovered: bool,
}

impl DiffState {
	pub fn empty() -> Self {
		Self { focus_tree: true, ..Self::default() }
	}

	/// Refresh from `git diff` (unstaged + staged). Creates empty structure if clean.
	pub fn refresh(&mut self) {
		match collect_git_diff() {
			Ok(files) => {
				self.total_additions = files.iter().map(|f| f.additions).sum();
				self.total_deletions = files.iter().map(|f| f.deletions).sum();
				self.files = files;
				self.tree = build_tree(&self.files);
				if self.selected_file >= self.files.len() {
					self.selected_file = 0;
				}
				let initial = self.selected_file;
				self.select_file(initial);
				self.error = None;
			}
			Err(e) => {
				self.files.clear();
				self.tree.clear();
				self.total_additions = 0;
				self.total_deletions = 0;
				self.error = Some(e);
			}
		}
		self.last_refresh = Some(std::time::Instant::now());
	}

	pub fn open_and_refresh(&mut self) {
		self.open = true;
		self.refresh();
	}

	pub fn close(&mut self) {
		self.open = false;
	}

	pub fn summary_label(&self) -> String {
		let lines = self.total_additions + self.total_deletions;
		format!("{lines} Line, {} Addition, {} Deletion", self.total_additions, self.total_deletions)
	}

	/// Stats for the currently selected file (or totals when empty).
	pub fn selected_stats_label(&self) -> String {
		if let Some(f) = self.files.get(self.selected_file) {
			let line_n = f.additions + f.deletions;
			format!("{line_n} Line, {} Addition, {} Deletion", f.additions, f.deletions)
		} else {
			self.summary_label()
		}
	}

	pub fn has_changes(&self) -> bool {
		!self.files.is_empty()
	}

	/// Whether a screen point is inside the last-rendered tree content area.
	pub fn point_in_tree(&self, x: u16, y: u16) -> bool {
		rect_contains(self.tree_area, x, y)
	}

	pub fn point_in_patch(&self, x: u16, y: u16) -> bool {
		rect_contains(self.patch_area, x, y)
	}

	/// Map a screen Y to a flattened tree row index (accounts for scroll + borders).
	pub fn tree_row_at_y(&self, y: u16) -> Option<usize> {
		let inner = self.tree_inner;
		if inner.height == 0 || y < inner.y || y >= inner.bottom() {
			return None;
		}
		let rows = self.visible_tree_rows();
		let viewport = inner.height as usize;
		let max_scroll = rows.len().saturating_sub(viewport.max(1));
		let scroll = self.tree_scroll.min(max_scroll);
		let idx = scroll + (y - inner.y) as usize;
		if idx < rows.len() { Some(idx) } else { None }
	}

	pub fn selected_patch_lines(&self) -> Vec<String> {
		self
			.files
			.get(self.selected_file)
			.map(|f| {
				let mut lines: Vec<String> = Vec::new();
				let mut in_header = true;
				for line in f.patch.lines() {
					if in_header {
						if line.starts_with("diff --git ")
							|| line.starts_with("index ")
							|| line.starts_with("--- ")
							|| line.starts_with("+++ ")
						{
							continue;
						} else {
							in_header = false;
						}
					}
					// Keep @@ in the raw list for line-number tracking helpers
					lines.push(line.to_string());
				}
				lines
			})
			.unwrap_or_default()
	}

	/// Code-only rows for the right pane (no `@@` hunk headers — those are not painted).
	pub fn selected_display_rows(&self) -> Vec<DiffDisplayRow> {
		let mut rows = Vec::new();
		let mut cur_add = 1usize;
		let mut cur_del = 1usize;
		for line in self.selected_patch_lines() {
			if line.starts_with("@@") {
				apply_hunk_line_counters(&line, &mut cur_add, &mut cur_del);
				continue; // never shown in the UI
			}
			if line.starts_with("diff ")
				|| line.starts_with("index ")
				|| line.starts_with("---")
				|| line.starts_with("+++")
			{
				continue;
			}
			if line.starts_with('+') {
				rows.push(DiffDisplayRow {
					kind: DiffRowKind::Add,
					line_no: cur_add,
					text: line.chars().skip(1).collect(),
				});
				cur_add = cur_add.saturating_add(1);
			} else if line.starts_with('-') {
				rows.push(DiffDisplayRow {
					kind: DiffRowKind::Del,
					line_no: cur_del,
					text: line.chars().skip(1).collect(),
				});
				cur_del = cur_del.saturating_add(1);
			} else {
				let text =
					if line.starts_with(' ') { line.chars().skip(1).collect() } else { line.clone() };
				rows.push(DiffDisplayRow { kind: DiffRowKind::Context, line_no: cur_add, text });
				cur_add = cur_add.saturating_add(1);
				cur_del = cur_del.saturating_add(1);
			}
		}
		rows
	}

	pub fn max_diff_scroll(&self, viewport: usize) -> usize {
		let lines = self.selected_display_rows().len();
		lines.saturating_sub(viewport.max(1))
	}

	pub fn scroll_diff_by(&mut self, delta: i32, viewport: usize) {
		let max = self.max_diff_scroll(viewport);
		if delta < 0 {
			self.diff_scroll = self.diff_scroll.saturating_sub((-delta) as usize);
		} else {
			self.diff_scroll = (self.diff_scroll + delta as usize).min(max);
		}
	}

	pub fn select_file(&mut self, index: usize) {
		if index < self.files.len() {
			self.selected_file = index;
			self.diff_scroll = 0;

			// Scroll to first addition/deletion (display rows — no @@ headers)
			let rows = self.selected_display_rows();
			for (i, row) in rows.iter().enumerate() {
				if matches!(row.kind, DiffRowKind::Add | DiffRowKind::Del) {
					self.diff_scroll = i.saturating_sub(8);
					break;
				}
			}
		}
	}

	/// Flatten visible tree rows for navigation / hit testing.
	pub fn visible_tree_rows(&self) -> Vec<TreeRow> {
		let mut rows = Vec::new();
		fn walk(nodes: &[TreeNode], depth: usize, rows: &mut Vec<TreeRow>) {
			for node in nodes {
				match node {
					TreeNode::Dir { name, children, expanded } => {
						rows.push(TreeRow {
							depth,
							label: name.clone(),
							is_dir: true,
							expanded: *expanded,
							file_index: None,
						});
						if *expanded {
							walk(children, depth + 1, rows);
						}
					}
					TreeNode::File { name, file_index } => {
						rows.push(TreeRow {
							depth,
							label: name.clone(),
							is_dir: false,
							expanded: false,
							file_index: Some(*file_index),
						});
					}
				}
			}
		}
		walk(&self.tree, 0, &mut rows);
		rows
	}

	pub fn toggle_tree_at_cursor(&mut self) {
		let rows = self.visible_tree_rows();
		let Some(row) = rows.get(self.tree_cursor) else {
			return;
		};
		if row.is_dir {
			toggle_dir_named(&mut self.tree, &row.label, row.depth, 0);
		} else if let Some(idx) = row.file_index {
			self.select_file(idx);
		}
	}

	pub fn activate_tree_cursor(&mut self) {
		let rows = self.visible_tree_rows();
		if let Some(row) = rows.get(self.tree_cursor) {
			if row.is_dir {
				self.toggle_tree_at_cursor();
			} else if let Some(idx) = row.file_index {
				self.select_file(idx);
			}
		}
	}

	pub fn move_tree_cursor(&mut self, delta: i32) {
		let len = self.visible_tree_rows().len();
		if len == 0 {
			return;
		}
		if delta < 0 {
			self.tree_cursor = self.tree_cursor.saturating_sub((-delta) as usize);
		} else {
			self.tree_cursor = (self.tree_cursor + delta as usize).min(len - 1);
		}
		// Auto-scroll tree to keep cursor visible (use last-rendered viewport when available)
		let viewport = self.tree_inner.height.max(1) as usize;
		if self.tree_cursor < self.tree_scroll {
			self.tree_scroll = self.tree_cursor;
		} else if self.tree_cursor >= self.tree_scroll + viewport {
			self.tree_scroll = self.tree_cursor.saturating_sub(viewport) + 1;
		}
	}
}

#[derive(Debug, Clone)]
pub struct TreeRow {
	pub depth: usize,
	pub label: String,
	pub is_dir: bool,
	pub expanded: bool,
	pub file_index: Option<usize>,
}

fn toggle_dir_named(nodes: &mut [TreeNode], name: &str, target_depth: usize, depth: usize) -> bool {
	for node in nodes.iter_mut() {
		if let TreeNode::Dir { name: n, children, expanded } = node {
			if depth == target_depth && n == name {
				*expanded = !*expanded;
				return true;
			}
			if *expanded && toggle_dir_named(children, name, target_depth, depth + 1) {
				return true;
			}
		}
	}
	false
}

fn collect_git_diff() -> Result<Vec<DiffFile>, String> {
	// Prefer porcelain name-status + per-file patches.
	let status = run_git(&["status", "--porcelain"])?;
	if status.trim().is_empty() {
		// Also check staged-only / unstaged with diff
		let unstaged = run_git(&["diff", "--numstat"]).unwrap_or_default();
		let staged = run_git(&["diff", "--cached", "--numstat"]).unwrap_or_default();
		if unstaged.trim().is_empty() && staged.trim().is_empty() {
			return Ok(Vec::new());
		}
	}

	let mut paths: HashSet<String> = HashSet::new();
	for line in status.lines() {
		if line.len() < 4 {
			continue;
		}
		// XY PATH or XY ORIG -> PATH
		let rest = line[2..].trim();
		let path = if let Some((_, right)) = rest.split_once(" -> ") { right.trim() } else { rest };
		if !path.is_empty() {
			paths.insert(path.to_string());
		}
	}

	// Supplement from numstat in case porcelain missed renames oddly
	for source in [
		run_git(&["diff", "--numstat"]).unwrap_or_default(),
		run_git(&["diff", "--cached", "--numstat"]).unwrap_or_default(),
	] {
		for line in source.lines() {
			let parts: Vec<_> = line.split('\t').collect();
			if parts.len() >= 3 {
				paths.insert(parts[2].to_string());
			}
		}
	}

	let mut files = Vec::new();
	let mut sorted: Vec<_> = paths.into_iter().collect();
	sorted.sort();

	for path in sorted {
		let unstaged_patch = run_git(&["diff", "-U999999", "--", &path]).unwrap_or_default();
		let staged_patch = run_git(&["diff", "--cached", "-U999999", "--", &path]).unwrap_or_default();
		let mut patch = String::new();
		if !staged_patch.trim().is_empty() {
			patch.push_str(&staged_patch);
			if !unstaged_patch.trim().is_empty() {
				patch.push('\n');
			}
		}
		if !unstaged_patch.trim().is_empty() {
			patch.push_str(&unstaged_patch);
		}
		// Untracked: show as full file add if no patch
		if patch.trim().is_empty() {
			if let Ok(content) = std::fs::read_to_string(&path) {
				let mut synthetic = format!("diff --git a/{path} b/{path}\n--- /dev/null\n+++ b/{path}\n");
				for (i, line) in content.lines().enumerate() {
					if i == 0 {
						synthetic.push_str(&format!("@@ -0,0 +1,{} @@\n", content.lines().count()));
					}
					synthetic.push('+');
					synthetic.push_str(line);
					synthetic.push('\n');
				}
				patch = synthetic;
			} else {
				patch = format!("diff --git a/{path} b/{path}\n(new or binary file)\n");
			}
		}

		let (additions, deletions) = count_diff_stats(&patch);
		files.push(DiffFile { path, additions, deletions, patch });
	}

	Ok(files)
}

fn count_diff_stats(patch: &str) -> (usize, usize) {
	let mut add = 0usize;
	let mut del = 0usize;
	for line in patch.lines() {
		if line.starts_with('+') && !line.starts_with("+++") {
			add += 1;
		} else if line.starts_with('-') && !line.starts_with("---") {
			del += 1;
		}
	}
	(add, del)
}

fn run_git(args: &[&str]) -> Result<String, String> {
	let output = Command::new("git").args(args).output().map_err(|e| format!("git failed: {e}"))?;
	if !output.status.success() {
		let err = String::from_utf8_lossy(&output.stderr);
		return Err(format!("git {}: {}", args.join(" "), err.trim()));
	}
	Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn build_tree(files: &[DiffFile]) -> Vec<TreeNode> {
	#[derive(Default)]
	struct Builder {
		dirs: std::collections::BTreeMap<String, Builder>,
		files: Vec<(String, usize)>,
	}

	impl Builder {
		fn insert(&mut self, components: &[&str], file_index: usize) {
			match components {
				[] => {}
				[name] => self.files.push(((*name).to_string(), file_index)),
				[dir, rest @ ..] => {
					self.dirs.entry((*dir).to_string()).or_default().insert(rest, file_index);
				}
			}
		}

		fn into_nodes(self) -> Vec<TreeNode> {
			let mut nodes = Vec::new();
			for (name, child) in self.dirs {
				nodes.push(TreeNode::Dir { name, children: child.into_nodes(), expanded: true });
			}
			for (name, file_index) in self.files {
				nodes.push(TreeNode::File { name, file_index });
			}
			nodes
		}
	}

	let mut root = Builder::default();
	for (idx, file) in files.iter().enumerate() {
		let path = Path::new(&file.path);
		let parts: Vec<&str> = path
			.components()
			.filter_map(|c| match c {
				Component::Normal(s) => s.to_str(),
				_ => None,
			})
			.collect();
		if parts.is_empty() {
			root.files.push((file.path.clone(), idx));
		} else {
			root.insert(&parts, idx);
		}
	}
	root.into_nodes()
}

fn rect_contains(r: Rect, x: u16, y: u16) -> bool {
	r.width > 0
		&& r.height > 0
		&& x >= r.x
		&& x < r.x.saturating_add(r.width)
		&& y >= r.y
		&& y < r.y.saturating_add(r.height)
}

/// Render the full-screen differ into `area`.
/// Updates hit-test rects on `state` so mouse handlers match paint exactly.
pub fn render_diff_view(state: &mut DiffState, theme: &ChatTheme, area: Rect, buf: &mut Buffer) {
	for y in area.top()..area.bottom() {
		for x in area.left()..area.right() {
			buf[(x, y)].reset();
			buf[(x, y)].set_bg(theme.bg);
		}
	}

	let chunks = Layout::default()
		.direction(Direction::Vertical)
		.constraints([Constraint::Length(1), Constraint::Min(3), Constraint::Length(1)])
		.split(area);

	// Global top bar: file count + aggregate Line / Addition / Deletion
	let title = format!("Diffs  ·  {} files  ·  {}", state.files.len(), state.summary_label());
	Paragraph::new(Span::styled(
		title,
		Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
	))
	.render(chunks[0], buf);

	// Body: tree | diff
	let body = Layout::default()
		.direction(Direction::Horizontal)
		.constraints([Constraint::Percentage(32), Constraint::Percentage(68)])
		.split(chunks[1]);

	state.tree_area = body[0];
	state.patch_area = body[1];

	render_tree_pane(state, theme, body[0], buf);
	render_diff_pane(state, theme, body[1], buf);

	// Footer: path + key hints
	let footer = if let Some(err) = &state.error {
		format!("Error: {err}")
	} else if state.files.is_empty() {
		"Working tree clean  ·  Esc close  ·  r refresh".to_string()
	} else {
		let path = state.files.get(state.selected_file).map(|f| f.path.as_str()).unwrap_or("");
		format!("{path}  ·  ←/→ panels  ·  Enter open  ·  Esc close  ·  r refresh")
	};
	Paragraph::new(Span::styled(footer, Style::default().fg(theme.border))).render(chunks[2], buf);
}

fn render_tree_pane(state: &mut DiffState, theme: &ChatTheme, area: Rect, buf: &mut Buffer) {
	let block = Block::default()
		.borders(Borders::ALL)
		.border_type(ratatui::widgets::BorderType::Rounded)
		.border_style(Style::default().fg(if state.focus_tree { theme.accent } else { theme.border }))
		.title(Span::styled(" Files ", Style::default().fg(theme.fg)));
	let inner = block.inner(area);
	state.tree_inner = inner;
	block.render(area, buf);

	let rows = state.visible_tree_rows();
	let viewport = inner.height as usize;
	let max_scroll = rows.len().saturating_sub(viewport.max(1));
	let scroll = state.tree_scroll.min(max_scroll);

	for (i, row) in rows.iter().skip(scroll).take(viewport).enumerate() {
		let y = inner.y + i as u16;
		if y >= inner.bottom() {
			break;
		}
		let abs_index = scroll + i;
		let selected = abs_index == state.tree_cursor;
		let is_active_file = row.file_index == Some(state.selected_file);

		let glyph = if row.is_dir {
			if row.expanded { "▼ " } else { "▶ " }
		} else if is_active_file {
			"● "
		} else {
			"  "
		};
		let indent = "  ".repeat(row.depth);
		let stats = row
			.file_index
			.and_then(|idx| state.files.get(idx))
			.map(|f| format!(" (+{}, -{})", f.additions, f.deletions))
			.unwrap_or_default();

		// Clip label to pane so long paths don't wrap and break hit rows
		let raw = format!("{indent}{glyph}{}{stats}", row.label);
		let max_w = inner.width as usize;
		let label: String = if raw.chars().count() > max_w {
			raw.chars().take(max_w.saturating_sub(1)).collect::<String>() + "…"
		} else {
			raw
		};
		let style = if selected {
			Style::default().fg(theme.bg).bg(theme.accent).add_modifier(Modifier::BOLD)
		} else if is_active_file {
			Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)
		} else if row.is_dir {
			Style::default().fg(theme.fg).add_modifier(Modifier::BOLD)
		} else {
			Style::default().fg(theme.fg)
		};

		// Paint full row so the entire width is clickable / highlighted
		for x in inner.x..inner.right() {
			buf[(x, y)].reset();
			if selected {
				buf[(x, y)].set_bg(theme.accent);
			} else {
				buf[(x, y)].set_bg(theme.bg);
			}
		}
		Paragraph::new(Span::styled(label, style))
			.render(Rect { x: inner.x, y, width: inner.width, height: 1 }, buf);
	}

	if max_scroll > 0 {
		crate::components::render_scrollbar_track_hover(
			inner,
			buf,
			rows.len(),
			scroll,
			state.tree_scrollbar_hovered,
			1,
		);
	}
}

fn render_diff_pane(state: &mut DiffState, theme: &ChatTheme, area: Rect, buf: &mut Buffer) {
	// Code-only pane — no hunk headers, no stats strip. Path is border title only.
	let file_title = state
		.files
		.get(state.selected_file)
		.map(|f| {
			Path::new(f.path.as_str())
				.file_name()
				.and_then(|n| n.to_str())
				.unwrap_or(f.path.as_str())
				.to_string()
		})
		.unwrap_or_else(|| "Diff".into());

	let block = Block::default()
		.borders(Borders::ALL)
		.border_type(ratatui::widgets::BorderType::Rounded)
		.border_style(Style::default().fg(if !state.focus_tree { theme.accent } else { theme.border }))
		.title(Span::styled(format!(" {file_title} "), Style::default().fg(theme.fg)));
	let inner = block.inner(area);
	block.render(area, buf);
	state.patch_inner = inner;

	let rows = state.selected_display_rows();
	if rows.is_empty() {
		Paragraph::new(Span::styled(
			"No diff for this file.",
			Style::default().fg(theme.border).add_modifier(Modifier::ITALIC),
		))
		.render(inner, buf);
		return;
	}

	let viewport = inner.height as usize;
	let max_scroll = rows.len().saturating_sub(viewport.max(1));
	let scroll = state.diff_scroll.min(max_scroll);

	let add_bg = Color::Rgb(0x0d, 0x2a, 0x0d);
	let del_bg = Color::Rgb(0x2a, 0x0d, 0x0d);
	let add_fg = Color::Rgb(0x4a, 0xe5, 0x8a);
	let del_fg = Color::Rgb(0xff, 0x5c, 0x5c);

	let file_path = state.files.get(state.selected_file).map(|f| f.path.as_str()).unwrap_or("");
	let (syntax_set, syn_theme) = syntect_engine();
	let syntax = syntax_for_path(syntax_set, file_path);
	let mut highlighter = HighlightLines::new(syntax, syn_theme);

	// Advance syntect through scrolled-off rows so highlighting stays correct
	for row in rows.iter().take(scroll) {
		let _ = highlighter.highlight_line(&format!("{}\n", row.text), syntax_set);
	}

	for (display_row, row) in rows.iter().skip(scroll).take(viewport).enumerate() {
		let y = inner.y + display_row as u16;
		if y >= inner.bottom() {
			break;
		}

		let (fg, bg, marker) = match row.kind {
			DiffRowKind::Add => (add_fg, add_bg, "+"),
			DiffRowKind::Del => (del_fg, del_bg, "-"),
			DiffRowKind::Context => (theme.fg, theme.bg, " "),
		};
		let num_fg = match row.kind {
			DiffRowKind::Add => add_fg,
			DiffRowKind::Del => del_fg,
			DiffRowKind::Context => theme.border,
		};

		for x in inner.x..inner.right() {
			buf[(x, y)].reset();
			buf[(x, y)].set_bg(bg);
		}

		let num = format!("{:>4}", row.line_no);
		let mut spans = vec![
			Span::styled(num, Style::default().fg(num_fg).bg(bg).add_modifier(Modifier::BOLD)),
			Span::styled(marker.to_string(), Style::default().fg(fg).bg(bg)),
		];
		spans.extend(highlight_with_syntect(&mut highlighter, syntax_set, &row.text, fg, Some(bg)));
		Paragraph::new(Line::from(spans))
			.render(Rect { x: inner.x, y, width: inner.width, height: 1 }, buf);
	}

	if max_scroll > 0 {
		crate::components::render_scrollbar_track_hover(
			inner,
			buf,
			rows.len(),
			scroll,
			state.patch_scrollbar_hovered,
			1,
		);
	}
}

/// Update add/del line counters from a single patch line (including `@@` headers).
fn apply_hunk_line_counters(line: &str, cur_add: &mut usize, cur_del: &mut usize) {
	if line.starts_with("@@") {
		if let Some(rest) = line.strip_prefix("@@ ") {
			let parts: Vec<&str> = rest.split_whitespace().collect();
			// @@ -old,count +new,count @@
			for p in parts {
				if let Some(old) = p.strip_prefix('-') {
					*cur_del = old.split(',').next().unwrap_or("1").parse().unwrap_or(1);
				} else if let Some(new) = p.strip_prefix('+') {
					*cur_add = new.split(',').next().unwrap_or("1").parse().unwrap_or(1);
				}
			}
		}
	} else if line.starts_with('+') && !line.starts_with("+++") {
		*cur_add = cur_add.saturating_add(1);
	} else if line.starts_with('-') && !line.starts_with("---") {
		*cur_del = cur_del.saturating_add(1);
	} else if !line.starts_with("diff ")
		&& !line.starts_with("index ")
		&& !line.starts_with("---")
		&& !line.starts_with("+++")
	{
		// context
		*cur_add = cur_add.saturating_add(1);
		*cur_del = cur_del.saturating_add(1);
	}
}

pub(crate) fn syntax_for_path<'a>(
	ss: &'a SyntaxSet,
	path: &str,
) -> &'a syntect::parsing::SyntaxReference {
	let p = Path::new(path);
	// Try full file name (e.g. Cargo.toml, Dockerfile)
	if let Some(name) = p.file_name().and_then(|n| n.to_str())
		&& let Some(s) = ss.find_syntax_by_extension(name)
	{
		return s;
	}
	if let Some(ext) = p.extension().and_then(|e| e.to_str())
		&& let Some(s) = ss.find_syntax_by_extension(ext)
	{
		return s;
	}
	// Common aliases
	let lower = path.to_ascii_lowercase();
	if lower.ends_with(".rs")
		&& let Some(s) = ss.find_syntax_by_name("Rust")
	{
		return s;
	}
	if (lower.ends_with(".ts") || lower.ends_with(".tsx"))
		&& let Some(s) = ss.find_syntax_by_extension("ts")
	{
		return s;
	}
	if (lower.ends_with(".js") || lower.ends_with(".jsx") || lower.ends_with(".mjs"))
		&& let Some(s) = ss.find_syntax_by_extension("js")
	{
		return s;
	}
	ss.find_syntax_plain_text()
}

pub(crate) fn syntect_color_to_ratatui(c: syntect::highlighting::Color) -> Color {
	// Ignore fully transparent
	if c.a == 0 {
		return Color::Reset;
	}
	Color::Rgb(c.r, c.g, c.b)
}

/// Highlight one source line with syntect; force background to match add/del/context.
pub(crate) fn highlight_with_syntect(
	highlighter: &mut HighlightLines<'_>,
	syntax_set: &SyntaxSet,
	line: &str,
	fallback_fg: Color,
	bg: Option<Color>,
) -> Vec<Span<'static>> {
	// syntect expects a trailing newline for correct state
	let input = if line.ends_with('\n') { line.to_string() } else { format!("{line}\n") };
	let Ok(regions) = highlighter.highlight_line(&input, syntax_set) else {
		let mut style = Style::default().fg(fallback_fg);
		if let Some(c) = bg {
			style = style.bg(c);
		}
		return vec![Span::styled(line.to_string(), style)];
	};
	let mut spans = Vec::with_capacity(regions.len());
	for (style, text) in regions {
		// Drop the synthetic trailing newline so we don't paint an extra row
		let text = text.trim_end_matches('\n');
		if text.is_empty() {
			continue;
		}
		let fg = syntect_color_to_ratatui(style.foreground);
		let mut s = Style::default();
		if let Some(c) = bg {
			s = s.bg(c);
		}
		if fg != Color::Reset {
			s = s.fg(fg);
		} else {
			s = s.fg(fallback_fg);
		}
		if style.font_style.contains(syntect::highlighting::FontStyle::BOLD) {
			s = s.add_modifier(Modifier::BOLD);
		}
		if style.font_style.contains(syntect::highlighting::FontStyle::ITALIC) {
			s = s.add_modifier(Modifier::ITALIC);
		}
		if style.font_style.contains(syntect::highlighting::FontStyle::UNDERLINE) {
			s = s.add_modifier(Modifier::UNDERLINED);
		}
		spans.push(Span::styled(text.to_string(), s));
	}
	if spans.is_empty() {
		let mut style = Style::default().fg(fallback_fg);
		if let Some(c) = bg {
			style = style.bg(c);
		}
		spans.push(Span::styled(line.to_string(), style));
	}
	spans
}

/// Lightweight numstat-only refresh for bottom-bar counters (no full patches).
pub fn quick_diff_stats() -> (usize, usize) {
	let mut add = 0usize;
	let mut del = 0usize;
	for args in [&["diff", "--numstat"][..], &["diff", "--cached", "--numstat"][..]] {
		if let Ok(out) = run_git(args) {
			for line in out.lines() {
				let parts: Vec<_> = line.split('\t').collect();
				if parts.len() >= 2 {
					add += parts[0].parse::<usize>().unwrap_or(0);
					del += parts[1].parse::<usize>().unwrap_or(0);
				}
			}
		}
	}
	// Count untracked roughly via status
	if let Ok(status) = run_git(&["status", "--porcelain"]) {
		for line in status.lines() {
			if let Some(rest) = line.strip_prefix("??") {
				let path = rest.trim();
				if let Ok(content) = std::fs::read_to_string(path) {
					add += content.lines().count();
				}
			}
		}
	}
	(add, del)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn build_tree_nests_paths() {
		let files = vec![
			DiffFile { path: "src/a.rs".into(), additions: 1, deletions: 0, patch: "+x\n".into() },
			DiffFile { path: "src/b/c.rs".into(), additions: 2, deletions: 1, patch: "+y\n".into() },
		];
		let tree = build_tree(&files);
		assert_eq!(tree.len(), 1);
		match &tree[0] {
			TreeNode::Dir { name, children, .. } => {
				assert_eq!(name, "src");
				assert!(children.len() >= 1);
			}
			_ => panic!("expected dir"),
		}
	}

	#[test]
	fn count_stats_ignores_headers() {
		let patch = "--- a/x\n+++ b/x\n@@ -1 +1 @@\n-old\n+new\n";
		assert_eq!(count_diff_stats(patch), (1, 1));
	}
}
