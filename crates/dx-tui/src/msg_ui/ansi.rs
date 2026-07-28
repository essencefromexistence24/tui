//! Minimal ANSI SGR → ratatui spans for tool/terminal output.

#![allow(dead_code)]
//! Strips cursor/clear sequences; maps common colors. No external deps.

use ratatui::{
	style::{Color, Modifier, Style},
	text::{Line, Span},
};

/// Render one logical line of terminal output (may include CSI).
pub fn ansi_line(input: &str, base: Style, gutter: Style) -> Line<'static> {
	let mut spans = vec![Span::styled("  │ ", gutter)];
	spans.extend(ansi_spans(input, base));
	if spans.len() == 1 {
		spans.push(Span::styled(String::new(), base));
	}
	Line::from(spans)
}

/// Parse SGR-ish ANSI into styled spans. Unknown CSI is dropped.
pub fn ansi_spans(input: &str, base: Style) -> Vec<Span<'static>> {
	let mut spans = Vec::new();
	let mut buf = String::new();
	let mut style = base;
	let chars: Vec<char> = input.chars().collect();
	let mut i = 0usize;

	let flush = |buf: &mut String, spans: &mut Vec<Span<'static>>, style: Style| {
		if !buf.is_empty() {
			spans.push(Span::styled(std::mem::take(buf), style));
		}
	};

	while i < chars.len() {
		if chars[i] == '\u{1b}' && i + 1 < chars.len() && chars[i + 1] == '[' {
			flush(&mut buf, &mut spans, style);
			i += 2;
			let mut params = String::new();
			while i < chars.len() {
				let c = chars[i];
				i += 1;
				if c.is_ascii_alphabetic() {
					if c == 'm' {
						style = apply_sgr(&params, base, style);
					}
					// Other CSI (H, J, K, …) — ignore
					break;
				}
				params.push(c);
			}
			continue;
		}
		// OSC / other ESC sequences: skip until BEL or ST
		if chars[i] == '\u{1b}' {
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
		// Drop CR
		if chars[i] == '\r' {
			i += 1;
			continue;
		}
		buf.push(chars[i]);
		i += 1;
	}
	flush(&mut buf, &mut spans, style);
	spans
}

/// Strip all ANSI for plain-text copy / previews.
pub fn strip_ansi(input: &str) -> String {
	ansi_spans(input, Style::default()).into_iter().map(|s| s.content.to_string()).collect()
}

fn apply_sgr(params: &str, base: Style, current: Style) -> Style {
	if params.is_empty() || params == "0" {
		return base;
	}
	let mut style = current;
	for p in params.split(';') {
		let code: u8 = p.parse().unwrap_or(0);
		match code {
			0 => style = base,
			1 => style = style.add_modifier(Modifier::BOLD),
			2 => style = style.add_modifier(Modifier::DIM),
			3 => style = style.add_modifier(Modifier::ITALIC),
			4 => style = style.add_modifier(Modifier::UNDERLINED),
			7 => style = style.add_modifier(Modifier::REVERSED),
			9 => style = style.add_modifier(Modifier::CROSSED_OUT),
			22 => {
				style = style.remove_modifier(Modifier::BOLD);
				style = style.remove_modifier(Modifier::DIM);
			}
			23 => {
				style = style.remove_modifier(Modifier::ITALIC);
			}
			24 => {
				style = style.remove_modifier(Modifier::UNDERLINED);
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
			40 => style = style.bg(Color::Black),
			41 => style = style.bg(Color::Red),
			42 => style = style.bg(Color::Green),
			43 => style = style.bg(Color::Yellow),
			44 => style = style.bg(Color::Blue),
			45 => style = style.bg(Color::Magenta),
			46 => style = style.bg(Color::Cyan),
			47 => style = style.bg(Color::Gray),
			49 => style = Style { bg: base.bg, ..style },
			90 => style = style.fg(Color::DarkGray),
			91 => style = style.fg(Color::LightRed),
			92 => style = style.fg(Color::LightGreen),
			93 => style = style.fg(Color::LightYellow),
			94 => style = style.fg(Color::LightBlue),
			95 => style = style.fg(Color::LightMagenta),
			96 => style = style.fg(Color::LightCyan),
			97 => style = style.fg(Color::White),
			// 38;5;n / 48;5;n handled partially via multi-param below
			_ => {}
		}
	}
	// 256-color: 38;5;n or 48;5;n
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
		if parts[i] == "48" && i + 2 < parts.len() && parts[i + 1] == "5" {
			if let Ok(n) = parts[i + 2].parse::<u8>() {
				style = style.bg(xterm256(n));
			}
			i += 3;
			continue;
		}
		// Truecolor 38;2;r;g;b
		if parts[i] == "38" && i + 4 < parts.len() && parts[i + 1] == "2" {
			if let (Ok(r), Ok(g), Ok(b)) =
				(parts[i + 2].parse::<u8>(), parts[i + 3].parse::<u8>(), parts[i + 4].parse::<u8>())
			{
				style = style.fg(Color::Rgb(r, g, b));
			}
			i += 5;
			continue;
		}
		if parts[i] == "48" && i + 4 < parts.len() && parts[i + 1] == "2" {
			if let (Ok(r), Ok(g), Ok(b)) =
				(parts[i + 2].parse::<u8>(), parts[i + 3].parse::<u8>(), parts[i + 4].parse::<u8>())
			{
				style = style.bg(Color::Rgb(r, g, b));
			}
			i += 5;
			continue;
		}
		i += 1;
	}
	style
}

fn xterm256(n: u8) -> Color {
	// Standard xterm-256 cube + grayscale (good enough for logs)
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
	fn strips_and_colors() {
		let s = "\u{1b}[31mred\u{1b}[0m plain";
		let plain = strip_ansi(s);
		assert_eq!(plain, "red plain");
		let spans = ansi_spans(s, Style::default());
		assert!(spans.len() >= 2);
	}
}
