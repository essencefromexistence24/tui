//! Minimal VT cell grid for in-card PTY rendering (clear, cursor, SGR, basic CU*).
//! Not a full xterm — enough for readable vim/htop-ish output in the transcript.

use ratatui::{
	style::{Color, Modifier, Style},
	text::{Line, Span},
};

#[derive(Debug, Clone)]
struct Cell {
	ch: char,
	style: Style,
}

impl Default for Cell {
	fn default() -> Self {
		Self { ch: ' ', style: Style::default() }
	}
}

/// Fixed-size terminal screen buffer.
#[derive(Debug, Clone)]
pub struct VtGrid {
	pub cols: usize,
	pub rows: usize,
	cells: Vec<Cell>,
	cx: usize,
	cy: usize,
	pen: Style,
	/// Scrollback of completed lines (when cursor advances past bottom).
	scrollback: Vec<Line<'static>>,
	base: Style,
}

impl VtGrid {
	pub fn new(cols: usize, rows: usize, base: Style) -> Self {
		let cols = cols.max(20);
		let rows = rows.max(5);
		Self {
			cols,
			rows,
			cells: vec![Cell { ch: ' ', style: base }; cols * rows],
			cx: 0,
			cy: 0,
			pen: base,
			scrollback: Vec::new(),
			base,
		}
	}

	pub fn resize(&mut self, cols: usize, rows: usize) {
		let cols = cols.max(20);
		let rows = rows.max(5);
		if cols == self.cols && rows == self.rows {
			return;
		}
		let mut next = vec![Cell { ch: ' ', style: self.base }; cols * rows];
		let copy_rows = self.rows.min(rows);
		let copy_cols = self.cols.min(cols);
		for r in 0..copy_rows {
			for c in 0..copy_cols {
				next[r * cols + c] = self.cells[r * self.cols + c].clone();
			}
		}
		self.cells = next;
		self.cols = cols;
		self.rows = rows;
		self.cx = self.cx.min(cols.saturating_sub(1));
		self.cy = self.cy.min(rows.saturating_sub(1));
	}

	fn idx(&self, x: usize, y: usize) -> usize {
		y * self.cols + x
	}

	fn put(&mut self, ch: char) {
		if self.cy >= self.rows {
			self.scroll_up();
		}
		if self.cx >= self.cols {
			self.cx = 0;
			self.cy += 1;
			if self.cy >= self.rows {
				self.scroll_up();
			}
		}
		let y = self.cy.min(self.rows.saturating_sub(1));
		let x = self.cx;
		let cols = self.cols;
		let i = y * cols + x;
		let pen = self.pen;
		if i < self.cells.len() {
			self.cells[i] = Cell { ch, style: pen };
		}
		self.cx += 1;
	}

	fn scroll_up(&mut self) {
		// push top row to scrollback as a Line
		let mut spans = Vec::new();
		let mut buf = String::new();
		let mut st = self.cells.first().map(|c| c.style).unwrap_or(self.base);
		for c in 0..self.cols {
			let cell = &self.cells[c];
			if cell.style != st && !buf.is_empty() {
				spans.push(Span::styled(std::mem::take(&mut buf), st));
				st = cell.style;
			}
			buf.push(cell.ch);
		}
		if !buf.is_empty() {
			spans.push(Span::styled(buf, st));
		}
		self.scrollback.push(Line::from(spans));
		if self.scrollback.len() > 500 {
			self.scrollback.drain(0..self.scrollback.len() - 500);
		}
		// shift rows up
		let row_bytes = self.cols;
		self.cells.rotate_left(row_bytes);
		for c in 0..self.cols {
			let i = (self.rows - 1) * self.cols + c;
			self.cells[i] = Cell { ch: ' ', style: self.base };
		}
		self.cy = self.rows.saturating_sub(1);
	}

	fn clear_screen(&mut self) {
		for cell in &mut self.cells {
			*cell = Cell { ch: ' ', style: self.base };
		}
		self.cx = 0;
		self.cy = 0;
	}

	fn clear_eos(&mut self) {
		let start = self.idx(self.cx, self.cy);
		for i in start..self.cells.len() {
			self.cells[i] = Cell { ch: ' ', style: self.base };
		}
	}

	fn clear_eol(&mut self) {
		let y = self.cy.min(self.rows.saturating_sub(1));
		let cols = self.cols;
		let cx = self.cx;
		let base = self.base;
		for x in cx..cols {
			let i = y * cols + x;
			if i < self.cells.len() {
				self.cells[i] = Cell { ch: ' ', style: base };
			}
		}
	}

	/// Feed raw PTY bytes (may include CSI).
	pub fn feed(&mut self, raw: &str) {
		let chars: Vec<char> = raw.chars().collect();
		let mut i = 0usize;
		while i < chars.len() {
			let ch = chars[i];
			if ch == '\u{1b}' && i + 1 < chars.len() && chars[i + 1] == '[' {
				i += 2;
				let mut params = String::new();
				while i < chars.len() {
					let c = chars[i];
					i += 1;
					if c.is_ascii_alphabetic() {
						self.handle_csi(&params, c);
						break;
					}
					params.push(c);
				}
				continue;
			}
			if ch == '\u{1b}' {
				// skip other ESC sequences (OSC etc.)
				i += 1;
				if i < chars.len() && chars[i] == ']' {
					i += 1;
					while i < chars.len() {
						if chars[i] == '\u{7}' {
							i += 1;
							break;
						}
						if chars[i] == '\u{1b}' && i + 1 < chars.len() && chars[i + 1] == '\\' {
							i += 2;
							break;
						}
						i += 1;
					}
				}
				continue;
			}
			match ch {
				'\n' => {
					self.cx = 0;
					self.cy += 1;
					if self.cy >= self.rows {
						self.scroll_up();
					}
				}
				'\r' => self.cx = 0,
				'\t' => {
					let next = ((self.cx / 8) + 1) * 8;
					while self.cx < next && self.cx < self.cols {
						self.put(' ');
					}
				}
				'\x08' if self.cx > 0 => {
					self.cx -= 1;
					let i = self.idx(self.cx, self.cy.min(self.rows - 1));
					if i < self.cells.len() {
						self.cells[i] = Cell { ch: ' ', style: self.pen };
					}
				}
				c if !c.is_control() => self.put(c),
				_ => {}
			}
			i += 1;
		}
	}

	fn handle_csi(&mut self, params: &str, cmd: char) {
		let nums: Vec<i32> = if params.is_empty() {
			vec![]
		} else {
			params.split(';').map(|p| p.parse().unwrap_or(0)).collect()
		};
		let n = |i: usize, d: i32| nums.get(i).copied().unwrap_or(d).max(0) as usize;
		match cmd {
			'm' => self.pen = apply_sgr(params, self.base, self.pen),
			'H' | 'f' => {
				let row = n(0, 1).saturating_sub(1).min(self.rows.saturating_sub(1));
				let col = n(1, 1).saturating_sub(1).min(self.cols.saturating_sub(1));
				self.cy = row;
				self.cx = col;
			}
			'A' => self.cy = self.cy.saturating_sub(n(0, 1).max(1)),
			'B' => self.cy = (self.cy + n(0, 1).max(1)).min(self.rows.saturating_sub(1)),
			'C' => self.cx = (self.cx + n(0, 1).max(1)).min(self.cols.saturating_sub(1)),
			'D' => self.cx = self.cx.saturating_sub(n(0, 1).max(1)),
			'G' => self.cx = n(0, 1).saturating_sub(1).min(self.cols.saturating_sub(1)),
			'J' => match nums.first().copied().unwrap_or(0) {
				0 => self.clear_eos(),
				1 => {
					// clear from start to cursor
					let end = self.idx(self.cx, self.cy);
					for i in 0..=end.min(self.cells.len().saturating_sub(1)) {
						self.cells[i] = Cell { ch: ' ', style: self.base };
					}
				}
				2 | 3 => self.clear_screen(),
				_ => {}
			},
			'K' => match nums.first().copied().unwrap_or(0) {
				0 => self.clear_eol(),
				1 => {
					let y = self.cy.min(self.rows.saturating_sub(1));
					let max_x = self.cx.min(self.cols.saturating_sub(1));
					let cols = self.cols;
					let base = self.base;
					for x in 0..=max_x {
						let i = y * cols + x;
						if i < self.cells.len() {
							self.cells[i] = Cell { ch: ' ', style: base };
						}
					}
				}
				2 => {
					let y = self.cy.min(self.rows.saturating_sub(1));
					let cols = self.cols;
					let base = self.base;
					for x in 0..cols {
						let i = y * cols + x;
						if i < self.cells.len() {
							self.cells[i] = Cell { ch: ' ', style: base };
						}
					}
				}
				_ => {}
			},
			_ => {}
		}
	}

	/// Visible screen + optional caret as ratatui lines (gutter optional).
	#[allow(dead_code)]
	pub fn to_lines(&self, gutter: Style, show_caret: bool) -> Vec<Line<'static>> {
		let mut out = Vec::new();
		// last few scrollback lines
		let sb = self.scrollback.len().saturating_sub(4);
		for line in &self.scrollback[sb..] {
			let mut spans = vec![Span::styled("  │ ", gutter)];
			spans.extend(line.spans.iter().map(|s| Span::styled(s.content.to_string(), s.style)));
			out.push(Line::from(spans));
		}
		for r in 0..self.rows {
			let mut spans = vec![Span::styled("  │ ", gutter)];
			let mut buf = String::new();
			let mut st = self.cells[r * self.cols].style;
			for c in 0..self.cols {
				let cell = &self.cells[r * self.cols + c];
				let mut ch = cell.ch;
				let mut style = cell.style;
				if show_caret && r == self.cy && c == self.cx {
					style = style.add_modifier(Modifier::REVERSED);
					if ch == ' ' {
						ch = '▌';
					}
				}
				if style != st && !buf.is_empty() {
					spans.push(Span::styled(std::mem::take(&mut buf), st));
					st = style;
				} else if buf.is_empty() {
					st = style;
				}
				buf.push(ch);
			}
			if !buf.is_empty() {
				spans.push(Span::styled(buf, st));
			}
			// skip fully blank rows at bottom (keep one)
			let blank = spans.iter().skip(1).all(|s| s.content.chars().all(|c| c == ' '));
			if blank && r + 1 == self.rows {
				// keep last blank if caret near bottom
			}
			out.push(Line::from(spans));
		}
		out
	}

	/// Flatten screen to plain strings (for PtySnapshot.lines fallback).
	pub fn to_plain_lines(&self) -> Vec<String> {
		let mut out = Vec::new();
		for r in 0..self.rows {
			let mut s = String::new();
			for c in 0..self.cols {
				s.push(self.cells[r * self.cols + c].ch);
			}
			let t = s.trim_end().to_string();
			if !t.is_empty() || r < self.cy + 1 {
				out.push(t);
			}
		}
		// drop trailing empties
		while out.last().is_some_and(|l| l.is_empty()) {
			out.pop();
		}
		out
	}
}

fn apply_sgr(params: &str, base: Style, current: Style) -> Style {
	if params.is_empty() || params == "0" {
		return base;
	}
	let mut style = current;
	for p in params.split(';') {
		match p.parse::<u8>().unwrap_or(0) {
			0 => style = base,
			1 => style = style.add_modifier(Modifier::BOLD),
			2 => style = style.add_modifier(Modifier::DIM),
			3 => style = style.add_modifier(Modifier::ITALIC),
			4 => style = style.add_modifier(Modifier::UNDERLINED),
			7 => style = style.add_modifier(Modifier::REVERSED),
			22 => {
				style = style.remove_modifier(Modifier::BOLD);
				style = style.remove_modifier(Modifier::DIM);
			}
			30 => style = style.fg(Color::Black),
			31 => style = style.fg(Color::Red),
			32 => style = style.fg(Color::Green),
			33 => style = style.fg(Color::Yellow),
			34 => style = style.fg(Color::Blue),
			35 => style = style.fg(Color::Magenta),
			36 => style = style.fg(Color::Cyan),
			37 => style = style.fg(Color::Gray),
			39 => style = style.fg(base.fg.unwrap_or(Color::Reset)),
			90 => style = style.fg(Color::DarkGray),
			91 => style = style.fg(Color::LightRed),
			92 => style = style.fg(Color::LightGreen),
			93 => style = style.fg(Color::LightYellow),
			94 => style = style.fg(Color::LightBlue),
			95 => style = style.fg(Color::LightMagenta),
			96 => style = style.fg(Color::LightCyan),
			97 => style = style.fg(Color::White),
			_ => {}
		}
	}
	// 256 / truecolor simplified
	let parts: Vec<&str> = params.split(';').collect();
	let mut i = 0;
	while i < parts.len() {
		if parts[i] == "38" && i + 2 < parts.len() && parts[i + 1] == "5" {
			if let Ok(n) = parts[i + 2].parse::<u8>() {
				style = style.fg(xterm256(n));
			}
			i += 3;
			continue;
		}
		if parts[i] == "38" && i + 4 < parts.len() && parts[i + 1] == "2" {
			if let (Ok(r), Ok(g), Ok(b)) =
				(parts[i + 2].parse::<u8>(), parts[i + 3].parse::<u8>(), parts[i + 4].parse::<u8>())
			{
				style = style.fg(Color::Rgb(r, g, b));
			}
			i += 5;
			continue;
		}
		i += 1;
	}
	style
}

fn xterm256(n: u8) -> Color {
	match n {
		0 => Color::Black,
		1 => Color::Red,
		2 => Color::Green,
		3 => Color::Yellow,
		4 => Color::Blue,
		5 => Color::Magenta,
		6 => Color::Cyan,
		7 => Color::Gray,
		8 => Color::DarkGray,
		9 => Color::LightRed,
		10 => Color::LightGreen,
		11 => Color::LightYellow,
		12 => Color::LightBlue,
		13 => Color::LightMagenta,
		14 => Color::LightCyan,
		15 => Color::White,
		16..=231 => {
			let idx = n - 16;
			let r = idx / 36;
			let g = (idx % 36) / 6;
			let b = idx % 6;
			let level = |v: u8| if v == 0 { 0 } else { 55 + 40 * v };
			Color::Rgb(level(r), level(g), level(b))
		}
		232..=255 => {
			let v = 8 + 10 * (n - 232);
			Color::Rgb(v, v, v)
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn writes_and_clears() {
		let mut g = VtGrid::new(40, 10, Style::default());
		g.feed("hello");
		assert!(g.to_plain_lines().iter().any(|l| l.contains("hello")));
		g.feed("\u{1b}[2J\u{1b}[H");
		let plain = g.to_plain_lines().join("");
		assert!(!plain.contains("hello"));
	}
}
