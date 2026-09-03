use std::io::Write;

use crossterm::execute;

/// Clears the terminal screen.
///
/// # Errors
///
/// Returns an error if writing to the terminal fails.
#[inline]
pub fn terminal_clear(mut w: impl Write) -> std::io::Result<()> {
	execute!(
		w,
		crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
		crossterm::style::Print("\n")
	)
}

