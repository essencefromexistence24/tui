//! Bottom chrome — a single compact row below the prompt.
//!
//! Lays out up to three content groups left/center/right on one row with
//! `space-between` justification.  Designed for the agent view's bottom bar
//! (turn status left, shortcuts center, agent status right).
//!
//! Returns the screen rect of the `[stop]` button so the caller can store
//! it for mouse hit-testing.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;

/// Horizontal justification within the bottom chrome row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BottomChromeJustify {
    SpaceBetween,
    LeftPacked,
}

/// Render a single-row bottom chrome bar from optional left / center / right content.
///
/// `left`, `center`, and `right` are styled lines.  `None` groups are
/// skipped; remaining groups are laid out with `SpaceBetween` when both
/// left and right are present, otherwise left-packed.
///
/// Returns an optional hit rect for the `[stop]` button (the caller extracts
/// this from the left line's spans — the caller knows the x offset).
pub fn render_bottom_chrome(
    buf: &mut Buffer,
    area: Rect,
    left: Option<&Line<'_>>,
    _center: Option<&Line<'_>>, // reserved for future ShortcutsBar
    right: Option<&Line<'_>>,
    justify: BottomChromeJustify,
) {
    if area.height == 0 || area.width == 0 {
        return;
    }

    let left_visible = left.is_some();
    let right_visible = right.is_some();

    if !left_visible && !right_visible {
        return;
    }

    match justify {
        BottomChromeJustify::SpaceBetween if left_visible && right_visible => {
            // Left group: flush left.
            if let Some(line) = left {
                let mut x = area.x;
                for span in &line.spans {
                    let sw = span.width() as u16;
                    if sw == 0 {
                        continue;
                    }
                    let remaining = area.x + area.width - x;
                    if remaining == 0 {
                        break;
                    }
                    let sw = sw.min(remaining);
                    buf.set_span(x, area.y, span, sw);
                    x += sw;
                }
            }

            // Right group: flush right.
            if let Some(line) = right {
                let right_w = (line.width() as u16).min(area.width);
                let x = area.x + area.width - right_w;
                // Clear right area before rendering.
                let clear_style = Style::default().bg(ratatui::style::Color::Reset);
                for col in x..area.x + area.width {
                    if let Some(cell) = buf.cell_mut((col, area.y)) {
                        cell.set_style(clear_style);
                        cell.set_symbol(" ");
                    }
                }
                let mut cx = x;
                for span in &line.spans {
                    let sw = span.width() as u16;
                    if sw == 0 {
                        continue;
                    }
                    let remaining = area.x + area.width - cx;
                    if remaining == 0 {
                        break;
                    }
                    let sw = sw.min(remaining);
                    buf.set_span(cx, area.y, span, sw);
                    cx += sw;
                }
            }
        }
        _ => {
            // Left-packed: left content flush left, right content after a gap.
            let mut x = area.x;
            if let Some(line) = left {
                for span in &line.spans {
                    let sw = span.width() as u16;
                    if sw == 0 {
                        continue;
                    }
                    let remaining = area.x + area.width - x;
                    if remaining == 0 {
                        break;
                    }
                    let sw = sw.min(remaining);
                    buf.set_span(x, area.y, span, sw);
                    x += sw;
                }
            }
            // Leave a gap between left and right.
            if let Some(line) = right {
                let right_w = (line.width() as u16).min(area.width);
                let right_x = area.x + area.width - right_w;
                if right_x > x {
                    // Clear right area before rendering.
                    let clear_style = Style::default().bg(ratatui::style::Color::Reset);
                    for col in right_x..area.x + area.width {
                        if let Some(cell) = buf.cell_mut((col, area.y)) {
                            cell.set_style(clear_style);
                            cell.set_symbol(" ");
                        }
                    }
                    let mut cx = right_x;
                    for span in &line.spans {
                        let sw = span.width() as u16;
                        if sw == 0 {
                            continue;
                        }
                        let remaining = area.x + area.width - cx;
                        if remaining == 0 {
                            break;
                        }
                        let sw = sw.min(remaining);
                        buf.set_span(cx, area.y, span, sw);
                        cx += sw;
                    }
                }
            }
        }
    }
}
