#![allow(dead_code)]

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use ratatui::{
	style::{Color, Modifier, Style},
	text::{Line, Span},
};
use unicode_width::UnicodeWidthStr;

/// Colors for enhanced markdown rendering
const H1_FG: Color = Color::Rgb(0xff, 0xa0, 0x50);
const H2_FG: Color = Color::Rgb(0xff, 0xcc, 0x66);
const H3_FG: Color = Color::Rgb(0xa0, 0xd0, 0xff);
const CODE_BG: Color = Color::Rgb(0x1a, 0x1a, 0x2e);
const CODE_FG: Color = Color::Rgb(0xc0, 0xd0, 0xff);
const INLINE_CODE_FG: Color = Color::Rgb(0xff, 0x88, 0xaa);
const BLOCKQUOTE_BAR: Color = Color::Rgb(0x66, 0xbb, 0x66);
const BLOCKQUOTE_FG: Color = Color::Rgb(0x88, 0xcc, 0x88);
const LINK_FG: Color = Color::Rgb(0x44, 0xaa, 0xff);
const HR_FG: Color = Color::Rgb(0x55, 0x55, 0x77);
const TABLE_BORDER: Color = Color::White;
const TABLE_HEADER_BG: Color = Color::Rgb(0x33, 0x33, 0x33);
const LIST_MARKER: Color = Color::Rgb(0x88, 0xbb, 0xff);

fn hard_wrap_text(text: &str, width: usize) -> Vec<String> {
	if text.is_empty() || width == 0 {
		return vec![text.to_string()];
	}
	use unicode_width::UnicodeWidthChar;
	let mut out = Vec::new();
	for line in text.lines() {
		if line.is_empty() {
			out.push(String::new());
			continue;
		}
		let mut current = String::new();
		let mut cols = 0usize;
		for ch in line.chars() {
			let cw = UnicodeWidthChar::width(ch).unwrap_or(0).max(1);
			if cols + cw > width && !current.is_empty() {
				out.push(std::mem::take(&mut current));
				cols = 0;
			}
			current.push(ch);
			cols += cw;
			if cols >= width {
				out.push(std::mem::take(&mut current));
				cols = 0;
			}
		}
		if !current.is_empty() {
			out.push(current);
		}
	}
	if out.is_empty() {
		out.push(String::new());
	}
	out
}

/// Parse block markdown (including tables) into ratatui Lines.
/// `max_width` constrains table column widths when Some.
pub fn render_markdown_blocks(
	input: &str,
	base_style: Style,
	max_width: Option<usize>,
) -> Vec<Line<'static>> {
	let mut options = Options::empty();
	options.insert(Options::ENABLE_TABLES);
	options.insert(Options::ENABLE_STRIKETHROUGH);

	let parser = Parser::new_ext(input, options);

	let mut lines: Vec<Line<'static>> = Vec::new();
	let mut current_spans: Vec<Span<'static>> = Vec::new();
	let mut current_style = base_style;
	let mut style_stack = vec![base_style];

	let mut in_table = false;
	let mut table_rows: Vec<Vec<Vec<Span<'static>>>> = Vec::new();
	let mut current_row: Vec<Vec<Span<'static>>> = Vec::new();
	let mut current_cell: Vec<Span<'static>> = Vec::new();

	let mut list_depth: usize = 0;
	let mut list_index = vec![];
	let mut in_blockquote = false;
	let mut in_code_block = false;
	let mut code_line: usize = 0;
	let mut code_highlighter: Option<syntect::easy::HighlightLines<'static>> = None;

	fn code_gutter(line_n: usize) -> Vec<Span<'static>> {
		let num =
			if line_n < 9999 { format!("{:>4}", line_n) } else { format!("{:>4}", line_n % 10000) };
		vec![Span::styled(format!("{num}│ "), Style::default().fg(Color::Rgb(0x55, 0x66, 0x77)))]
	}

	for event in parser {
		match event {
			Event::Start(tag) => match tag {
				Tag::Strong => {
					current_style = current_style.add_modifier(Modifier::BOLD);
					style_stack.push(current_style);
				}
				Tag::Emphasis => {
					current_style = current_style.add_modifier(Modifier::ITALIC);
					style_stack.push(current_style);
				}
				Tag::Strikethrough => {
					current_style = current_style.add_modifier(Modifier::CROSSED_OUT);
					style_stack.push(current_style);
				}
				Tag::CodeBlock(pulldown_cmark::CodeBlockKind::Fenced(lang)) => {
					// Flush any pending paragraph before the fence.
					if !current_spans.is_empty() {
						lines.push(Line::from(std::mem::take(&mut current_spans)));
					}
					in_code_block = true;
					code_line = 0;
					current_style = base_style.fg(CODE_FG);
					style_stack.push(current_style);
					let lang = lang.into_string();
					let label = if lang.is_empty() { "code".to_string() } else { lang.clone() };

					let (ss, theme) = crate::diff_view::syntect_engine();
					let syntax = crate::diff_view::syntax_for_path(ss, &format!("dummy.{}", lang));
					code_highlighter = Some(syntect::easy::HighlightLines::new(syntax, theme));
					lines.push(Line::from(vec![Span::styled(
						format!("  ┌─ {label} ─"),
						Style::default().fg(Color::Rgb(0x88, 0x99, 0xaa)).add_modifier(Modifier::BOLD),
					)]));
				}
				Tag::CodeBlock(pulldown_cmark::CodeBlockKind::Indented) => {
					if !current_spans.is_empty() {
						lines.push(Line::from(std::mem::take(&mut current_spans)));
					}
					in_code_block = true;
					code_line = 0;
					current_style = base_style.fg(CODE_FG);
					style_stack.push(current_style);
					code_highlighter = None;
				}
				Tag::Heading { level, .. } => {
					let h_color = match level as u32 {
						1 => H1_FG,
						2 => H2_FG,
						_ => H3_FG,
					};
					current_style = base_style.fg(h_color).add_modifier(Modifier::BOLD);
					style_stack.push(current_style);
					// Blank line before heading
					if !lines.is_empty() {
						lines.push(Line::from(vec![]));
					}
				}
				Tag::BlockQuote(_) => {
					in_blockquote = true;
					current_style = base_style.fg(BLOCKQUOTE_FG).add_modifier(Modifier::ITALIC);
					style_stack.push(current_style);
				}
				Tag::List(start) => {
					list_depth += 1;
					list_index.push(start);
				}
				Tag::Item => {
					let mut prefix = "• ".to_string();
					if let Some(idx) = list_index.last_mut()
						&& let Some(n) = *idx
					{
						prefix = format!("{}. ", n);
						*idx = Some(n + 1);
					}
					let indent = list_depth.saturating_sub(1) * 2;
					current_spans.push(Span::styled(
						format!("{}{}", " ".repeat(indent), prefix),
						base_style.fg(LIST_MARKER).add_modifier(Modifier::BOLD),
					));
				}
				Tag::Table(_) => {
					in_table = true;
					table_rows.clear();
				}
				Tag::TableRow => {
					current_row.clear();
				}
				Tag::TableCell => {
					current_cell.clear();
				}
				_ => {}
			},
			Event::End(tag) => match tag {
				TagEnd::CodeBlock => {
					// Flush last code line, then close the fence chrome.
					if !current_spans.is_empty() {
						code_line += 1;
						let mut spans = code_gutter(code_line);
						spans.append(&mut current_spans);
						lines.push(Line::from(spans));
					}
					lines.push(Line::from(vec![Span::styled(
						"  └────────",
						Style::default().fg(Color::Rgb(0x55, 0x66, 0x77)),
					)]));
					in_code_block = false;
					code_highlighter = None;
					style_stack.pop();
					current_style = *style_stack.last().unwrap_or(&base_style);
				}
				TagEnd::Strong | TagEnd::Emphasis | TagEnd::Strikethrough => {
					style_stack.pop();
					current_style = *style_stack.last().unwrap_or(&base_style);
				}
				TagEnd::Heading(_) => {
					style_stack.pop();
					current_style = *style_stack.last().unwrap_or(&base_style);
					lines.push(Line::from(std::mem::take(&mut current_spans)));
					lines.push(Line::from(vec![]));
				}
				TagEnd::BlockQuote(_) => {
					in_blockquote = false;
					style_stack.pop();
					current_style = *style_stack.last().unwrap_or(&base_style);
					if !current_spans.is_empty() {
						let mut spans = vec![Span::styled("▎ ", base_style.fg(BLOCKQUOTE_BAR))];
						spans.append(&mut current_spans);
						lines.push(Line::from(spans));
					}
				}
				TagEnd::List(_) => {
					list_depth -= 1;
					list_index.pop();
				}
				TagEnd::Item if !current_spans.is_empty() => {
					lines.push(Line::from(std::mem::take(&mut current_spans)));
				}
				TagEnd::Paragraph => {
					if !in_table && !current_spans.is_empty() {
						if in_blockquote {
							let mut spans = vec![Span::styled("▎ ", base_style.fg(BLOCKQUOTE_BAR))];
							spans.append(&mut current_spans);
							lines.push(Line::from(spans));
						} else {
							lines.push(Line::from(std::mem::take(&mut current_spans)));
						}
					}
					if list_depth == 0 {
						lines.push(Line::from(vec![]));
					}
				}
				TagEnd::TableCell => {
					current_row.push(std::mem::take(&mut current_cell));
				}
				TagEnd::TableRow | TagEnd::TableHead => {
					table_rows.push(std::mem::take(&mut current_row));
				}
				TagEnd::Table => {
					in_table = false;
					if !table_rows.is_empty() {
						render_table(&table_rows, &mut lines, base_style, max_width);
						lines.push(Line::from(vec![]));
					}
				}
				_ => {}
			},
			Event::Text(text) => {
				let style = current_style;
				let raw = text.into_string();
				// Never strip markers inside fenced/indented code — show literal source.
				let clean = if in_code_block {
					raw
				} else {
					raw
						.replace("**", "")
						.replace("__", "")
						.replace("~~", "")
						.replace('`', "")
						.replace("_ ", " ")
						.replace(" _", " ")
				};
				// Code blocks often arrive as one multi-line Text event.
				if in_code_block && clean.contains('\n') {
					for (i, part) in clean.split('\n').enumerate() {
						if i > 0 || !current_spans.is_empty() {
							code_line += 1;
							let mut spans = code_gutter(code_line);
							spans.append(&mut current_spans);
							if spans.len() == 1 {
								// empty line inside fence
								spans.push(Span::styled(String::new(), Style::default()));
							}
							lines.push(Line::from(spans));
						}
						if !part.is_empty() {
							if let Some(ref mut hl) = code_highlighter {
								let (ss, _) = crate::diff_view::syntect_engine();
								let hl_spans =
									crate::diff_view::highlight_with_syntect(hl, ss, part, CODE_FG, None);
								current_spans.extend(hl_spans);
							} else {
								current_spans.push(Span::styled(part.to_string(), Style::default().fg(CODE_FG)));
							}
						}
					}
				} else if !clean.is_empty() {
					if in_table {
						current_cell.push(Span::styled(clean, style));
					} else if in_code_block {
						if let Some(ref mut hl) = code_highlighter {
							let (ss, _) = crate::diff_view::syntect_engine();
							let hl_spans =
								crate::diff_view::highlight_with_syntect(hl, ss, &clean, CODE_FG, None);
							current_spans.extend(hl_spans);
						} else {
							current_spans.push(Span::styled(clean, Style::default().fg(CODE_FG)));
						}
					} else {
						current_spans.push(Span::styled(clean, style));
					}
				}
			}
			Event::Code(text) => {
				// Inline code — keep backticks as chrome, not stripped
				let span = Span::styled(
					text.into_string(),
					base_style.fg(INLINE_CODE_FG).add_modifier(Modifier::DIM),
				);
				if in_table {
					current_cell.push(span);
				} else {
					current_spans.push(span);
				}
			}
			Event::SoftBreak | Event::HardBreak => {
				if in_table {
					current_cell.push(Span::styled(" ", current_style));
				} else if in_code_block {
					code_line += 1;
					let mut spans = code_gutter(code_line);
					spans.append(&mut current_spans);
					lines.push(Line::from(spans));
				} else if in_blockquote {
					let mut spans = vec![Span::styled("▎ ", base_style.fg(BLOCKQUOTE_BAR))];
					spans.append(&mut current_spans);
					lines.push(Line::from(spans));
				} else {
					lines.push(Line::from(std::mem::take(&mut current_spans)));
				}
			}
			Event::Rule => {
				lines.push(Line::from(Span::styled("━".repeat(24), base_style.fg(HR_FG))));
			}
			_ => {}
		}
	}

	if !current_spans.is_empty() {
		lines.push(Line::from(std::mem::take(&mut current_spans)));
	}

	while lines.last().is_some_and(|l| l.spans.is_empty()) {
		lines.pop();
	}

	// Final safety: hard-wrap any remaining over-wide lines to max_width so
	// nothing paints outside the message column (ghost / fixed chars).
	if let Some(mw) = max_width
		&& mw > 0
	{
		lines = wrap_lines_to_width(lines, mw);
	}

	lines
}

/// Wrap markdown lines to `width` display columns (preserves span styles).
fn wrap_lines_to_width(lines: Vec<Line<'static>>, width: usize) -> Vec<Line<'static>> {
	use unicode_width::UnicodeWidthChar;
	let mut out = Vec::new();
	for line in lines {
		let total: usize = line.spans.iter().map(|s| s.content.width()).sum();
		if total <= width {
			out.push(line);
			continue;
		}
		let mut cur: Vec<Span<'static>> = Vec::new();
		let mut cur_w = 0usize;
		for span in line.spans {
			let style = span.style;
			let mut rest = span.content.as_ref();
			while !rest.is_empty() {
				let avail = width.saturating_sub(cur_w);
				if avail == 0 {
					out.push(Line::from(std::mem::take(&mut cur)));
					cur_w = 0;
					continue;
				}
				let mut take_cols = 0usize;
				let mut take_bytes = 0usize;
				for (i, ch) in rest.char_indices() {
					let cw = UnicodeWidthChar::width(ch).unwrap_or(0).max(1);
					if take_cols + cw > avail {
						break;
					}
					take_cols += cw;
					take_bytes = i + ch.len_utf8();
				}
				if take_bytes == 0 {
					if cur_w > 0 {
						out.push(Line::from(std::mem::take(&mut cur)));
						cur_w = 0;
						continue;
					}
					let ch = rest.chars().next().unwrap();
					let bl = ch.len_utf8();
					cur.push(Span::styled(rest[..bl].to_string(), style));
					out.push(Line::from(std::mem::take(&mut cur)));
					cur_w = 0;
					rest = &rest[bl..];
					continue;
				}
				cur.push(Span::styled(rest[..take_bytes].to_string(), style));
				cur_w += take_cols;
				rest = &rest[take_bytes..];
				if cur_w >= width {
					out.push(Line::from(std::mem::take(&mut cur)));
					cur_w = 0;
				}
			}
		}
		if !cur.is_empty() {
			out.push(Line::from(cur));
		}
	}
	out
}

fn render_table(
	table_rows: &[Vec<Vec<Span<'static>>>],
	lines: &mut Vec<Line<'static>>,
	base_style: Style,
	max_width: Option<usize>,
) {
	if table_rows.is_empty() {
		return;
	}

	let cols = table_rows[0].len();
	if cols == 0 {
		return;
	}

	// Calculate ideal column widths from content
	let mut ideal = vec![0usize; cols];
	for row in table_rows {
		for (i, cell) in row.iter().enumerate() {
			if i < cols {
				let w: usize = cell.iter().map(|s| s.content.width()).sum();
				ideal[i] = ideal[i].max(w);
			}
		}
	}

	// Constrain columns to available width
	let sep_chars = 3 * cols + 2; // "│ " + cols * (" │ ") = 2 + 3*cols
	let available_width = max_width.map(|w| w.saturating_sub(sep_chars));
	let col_widths: Vec<usize> = if let Some(avail) = available_width {
		let total_ideal: usize = ideal.iter().sum();
		if total_ideal > avail {
			let mut widths = ideal.clone();
			let overflow_target = total_ideal.saturating_sub(avail);

			for (i, &w) in ideal.iter().enumerate() {
				let shrink = (w * overflow_target) / total_ideal.max(1);
				widths[i] = w.saturating_sub(shrink).max(1);
			}

			let mut current_sum: usize = widths.iter().sum();
			while current_sum > avail {
				let mut max_idx = 0;
				let mut max_w = 0;
				for (i, &w) in widths.iter().enumerate() {
					if w > max_w {
						max_w = w;
						max_idx = i;
					}
				}
				if widths[max_idx] > 1 {
					widths[max_idx] -= 1;
					current_sum -= 1;
				} else {
					break;
				}
			}
			widths
		} else {
			ideal
		}
	} else {
		ideal
	};

	let border_style = base_style.fg(TABLE_BORDER);
	// Helper: build a separator line
	let make_border = |left: &str, mid: &str, right: &str| -> Line<'static> {
		let mut sep = Vec::new();
		sep.push(Span::styled(left.to_string(), border_style));
		for (i, w) in col_widths.iter().enumerate() {
			sep.push(Span::styled("─".repeat(*w), border_style));
			if i < cols - 1 {
				sep.push(Span::styled(mid.to_string(), border_style));
			}
		}
		sep.push(Span::styled(right.to_string(), border_style));
		Line::from(sep)
	};

	// Wrap cell text to column width
	type WrappedCell = Vec<String>;
	type WrappedRow = Vec<WrappedCell>;
	let mut wrapped: Vec<WrappedRow> = Vec::new();

	for row in table_rows.iter() {
		let mut wrapped_row: WrappedRow = Vec::new();
		let max_sub: usize;
		{
			let mut sub_lines = Vec::new();
			for (i, cell) in row.iter().enumerate() {
				if i < cols {
					let text: String = cell.iter().map(|s| s.content.as_ref()).collect();
					let wrapped_lines = hard_wrap_text(&text, col_widths[i]);
					sub_lines.push(wrapped_lines.len());
					wrapped_row.push(wrapped_lines);
				}
			}
			max_sub = sub_lines.into_iter().max().unwrap_or(1);
		}
		// Pad all cells to same height
		for cell in wrapped_row.iter_mut() {
			while cell.len() < max_sub {
				cell.push(String::new());
			}
		}
		wrapped.push(wrapped_row);
	}

	// Determine max sub-lines per row for multi-line handling
	let row_heights: Vec<usize> =
		wrapped.iter().map(|wr| wr.first().map_or(1, |c| c.len())).collect();

	// Top border
	lines.push(make_border("┌─", "─┬─", "─┐"));

	for (r_idx, row_data) in wrapped.iter().enumerate() {
		let sub_count = row_heights[r_idx];
		let is_header = r_idx == 0 && !table_rows[0].is_empty();

		for sub_idx in 0..sub_count {
			let mut row_spans: Vec<Span<'static>> = Vec::new();
			row_spans.push(Span::styled("│ ", border_style));

			for (i, cell_lines) in row_data.iter().enumerate() {
				if i >= cols {
					break;
				}
				let cell_text = cell_lines.get(sub_idx).map(|s| s.as_str()).unwrap_or("");
				let display = cell_text.chars().take(col_widths[i]).collect::<String>();
				let cell_w = display.width();
				let pad = col_widths[i].saturating_sub(cell_w);

				let cell_style =
					if is_header { base_style.add_modifier(Modifier::BOLD) } else { base_style };
				row_spans.push(Span::styled(display, cell_style));
				if pad > 0 {
					row_spans.push(Span::styled(" ".repeat(pad), cell_style));
				}
				if i < cols - 1 {
					row_spans.push(Span::styled(" │ ", border_style));
				} else {
					row_spans.push(Span::styled(" │", border_style));
				}
			}
			lines.push(Line::from(row_spans));
		}

		// Row separator (except after last row)
		if r_idx < wrapped.len() - 1 {
			lines.push(make_border("├─", "─┼─", "─┤"));
		}
	}

	// Bottom border
	lines.push(make_border("└─", "─┴─", "─┘"));
}
