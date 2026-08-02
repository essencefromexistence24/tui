use std::path::PathBuf;

use anyhow::Result;
use crossterm::event;
use ratatui::{
    backend::{Backend, ClearType, WindowSize},
    buffer::{Buffer, Cell},
    layout::{Position, Rect, Size},
    style::Color,
};
use tracing::error;

use dx::app::Editor;
use dx::config::Config;
use dx::config_io::DirectoryContext;
use dx::services::authority::Authority;
use dx::services::env_provider::EnvProvider;
use dx::services::workspace_trust::WorkspaceTrust;
use dx::view::color_support::ColorCapability;

/// A ratatui Backend that stores rendered output in a Buffer.
/// Used to capture the editor's rendered output into an external buffer.
struct CaptureBackend {
    size: Size,
    cursor_pos: Position,
    cursor_set: bool,
}

impl Backend for CaptureBackend {
    type Error = std::io::Error;

    fn draw<'a, I>(&mut self, _content: I) -> Result<(), Self::Error>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        Ok(())
    }

    fn hide_cursor(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn show_cursor(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn get_cursor_position(&mut self) -> Result<Position, Self::Error> {
        Ok(self.cursor_pos)
    }

    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> Result<(), Self::Error> {
        self.cursor_pos = position.into();
        self.cursor_set = true;
        Ok(())
    }

    fn clear(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn clear_region(&mut self, _clear_type: ClearType) -> Result<(), Self::Error> {
        Ok(())
    }

    fn window_size(&mut self) -> Result<WindowSize, Self::Error> {
        Ok(WindowSize {
            columns_rows: self.size,
            pixels: ratatui::layout::Size::new(0, 0),
        })
    }

    fn size(&self) -> Result<Size, Self::Error> {
        Ok(self.size)
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

pub struct EditorAdapter {
    editor: Option<Editor>,
    needs_init: bool,
    last_cursor: Option<(Position, crossterm::cursor::SetCursorStyle)>,
    pending_theme: Option<HostTheme>,
    last_applied_theme: Option<HostTheme>,
}

#[derive(Clone, PartialEq, Eq)]
struct HostTheme {
    base: &'static str,
    overrides: Vec<(&'static str, Color)>,
}

impl EditorAdapter {
    pub fn new() -> Self {
        Self {
            editor: None,
            needs_init: false,
            last_cursor: None,
            pending_theme: None,
            last_applied_theme: None,
        }
    }

    pub fn schedule_init(&mut self) {
        self.needs_init = true;
    }

    pub fn ensure_initialized(&mut self) -> Result<()> {
        if self.editor.is_some() {
            return Ok(());
        }

        let config = Config::default();
        let (width, height) = crossterm::terminal::size().unwrap_or((80, 24));
        let working_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let dir_context = match DirectoryContext::from_system() {
            Ok(dc) => dc,
            Err(e) => {
                error!("Failed to create DirectoryContext: {e}");
                return Err(e.into());
            }
        };
        let color_capability = ColorCapability::TrueColor;
        let authority = Authority::local(
            std::sync::Arc::new(WorkspaceTrust::permissive()),
            std::sync::Arc::new(EnvProvider::inactive()),
        );
        let defer_plugin_load = false;

        match Editor::with_working_dir_opts(
            config,
            width,
            height,
            Some(working_dir),
            dir_context,
            true,
            color_capability,
            authority,
            defer_plugin_load,
        ) {
            Ok(mut editor) => {
                // Fresh session with no opened files → show the left file explorer
                // sidebar and focus it so folder expand/collapse (accordion) works.
                editor.apply_active_window_explorer_default(
                    /*opened_files*/ false, /*workspace_restored*/ false,
                );
                editor.show_file_explorer();
                editor.focus_file_explorer();
                // Drain async explorer init so the tree is ready on first paint.
                let _ = editor.process_async_messages();
                // Apply a theme requested before the editor finished init.
                if let Some(theme) = self.pending_theme.clone()
                    && let Some(applied) = editor.apply_theme_overrides_external(
                        theme.base,
                        "grok-build-active",
                        theme.overrides.iter().copied(),
                    )
                {
                    if applied != theme.overrides.len() {
                        error!(
                            applied,
                            requested = theme.overrides.len(),
                            "Some Grok theme roles were not recognized by the editor"
                        );
                    }
                    self.last_applied_theme = Some(theme);
                    self.pending_theme = None;
                }
                self.editor = Some(editor);
                self.needs_init = false;
                Ok(())
            }
            Err(e) => {
                error!("Failed to initialize editor: {e}");
                Err(e)
            }
        }
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        self.last_cursor = None;

        if self.needs_init
            && let Err(e) = self.ensure_initialized()
        {
            error!("Editor init deferred: {e}");
            return;
        }

        let Some(editor) = &mut self.editor else {
            return;
        };

        if area.width == 0 || area.height == 0 {
            return;
        }

        let backend = CaptureBackend {
            size: Size::new(area.width, area.height),
            cursor_pos: Position::new(0, 0),
            cursor_set: false,
        };
        let mut terminal = match ratatui::Terminal::new(backend) {
            Ok(t) => t,
            Err(e) => {
                error!("Failed to create terminal for editor render: {e}");
                return;
            }
        };

        let _guard = editor.tokio_runtime().map(|rt| rt.enter());
        if let Ok(completed) = terminal.draw(|frame| editor.render(frame)) {
            let rendered = completed.buffer;
            let render_area = rendered.area;
            for y in 0..area.height.min(render_area.height) {
                for x in 0..area.width.min(render_area.width) {
                    let default_cell = Cell::default();
                    let cell = rendered.cell((x, y)).unwrap_or(&default_cell);
                    buf[(area.x + x, area.y + y)] = cell.clone();
                }
            }

            // Forward the editor's hardware cursor position.
            let cp = terminal.backend().cursor_pos;
            let abs_x = area.x + cp.x;
            let abs_y = area.y + cp.y;
            self.last_cursor = Some((
                Position::new(abs_x, abs_y),
                crossterm::cursor::SetCursorStyle::SteadyBlock,
            ));
        }
    }

    /// Return the last known cursor position and style for hardware cursor placement.
    pub fn cursor(&self) -> Option<(Position, crossterm::cursor::SetCursorStyle)> {
        self.last_cursor
    }

    pub fn handle_event(&mut self, event: event::Event) -> Result<bool> {
        if self.needs_init {
            self.ensure_initialized()?;
        }

        let Some(editor) = &mut self.editor else {
            return Ok(false);
        };

        tokio::task::block_in_place(|| editor.handle_input_event(event))
    }

    pub fn tick(&mut self) -> Result<bool> {
        if self.needs_init {
            self.ensure_initialized()?;
        }

        let Some(editor) = &mut self.editor else {
            return Ok(true);
        };

        let _processed = tokio::task::block_in_place(|| editor.process_async_messages());
        Ok(true)
    }

    pub fn is_initialized(&self) -> bool {
        self.editor.is_some()
    }

    /// Apply the host's complete semantic palette to the embedded editor.
    /// The built-in base supplies non-color behavior; every rendered color
    /// role is then replaced by the host values. Re-applying an unchanged
    /// palette is a no-op.
    pub fn apply_host_theme(&mut self, base: &'static str, overrides: Vec<(&'static str, Color)>) {
        let theme = HostTheme { base, overrides };
        if self.last_applied_theme.as_ref() == Some(&theme) {
            return;
        }
        if let Some(editor) = &mut self.editor {
            if let Some(applied) = editor.apply_theme_overrides_external(
                theme.base,
                "grok-build-active",
                theme.overrides.iter().copied(),
            ) {
                if applied != theme.overrides.len() {
                    error!(
                        applied,
                        requested = theme.overrides.len(),
                        "Some Grok theme roles were not recognized by the editor"
                    );
                }
                self.last_applied_theme = Some(theme);
                self.pending_theme = None;
            }
        } else {
            self.pending_theme = Some(theme);
        }
    }

    #[allow(dead_code)]
    pub fn editor_mut(&mut self) -> Option<&mut Editor> {
        self.editor.as_mut()
    }
}
