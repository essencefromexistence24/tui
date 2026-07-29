//! In-process host for the DX file-browser engine.
//!
//! This adapter deliberately does not call `fb_term::Term::start` or consume
//! Crossterm events. Grok owns both. DX owns its browser `Core`, Lua layout,
//! actions, and widgets, which render into Grok's current Ratatui buffer.

#[path = "../../../../dx-tui/src/file_browser/cmp/mod.rs"]
mod cmp;
#[path = "../../../../dx-tui/src/file_browser/confirm/mod.rs"]
mod confirm;
#[path = "../../../../dx-tui/src/file_browser/help/mod.rs"]
mod help;
#[path = "../../../../dx-tui/src/file_browser/input/mod.rs"]
mod input;
#[path = "../../../../dx-tui/src/file_browser/mgr/mod.rs"]
mod mgr;
#[path = "../../../../dx-tui/src/file_browser/pick/mod.rs"]
mod pick;
#[path = "../../../../dx-tui/src/file_browser/spot/mod.rs"]
mod spot;
#[path = "../../../../dx-tui/src/file_browser/tasks/mod.rs"]
#[allow(dead_code)]
mod tasks;
#[path = "../../../../dx-tui/src/file_browser/which/mod.rs"]
mod which;

use fb_binding::elements::render_once;
use fb_core::Core;
use fb_macro::act;
use fb_plugin::LUA;
use fb_shared::{
    Layer,
    data::Data,
    event::{ActionCow, Event as FbEvent},
    url::UrlLike,
};
use mlua::{ObjectLike, Table};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::Line,
    widgets::{Paragraph, Widget, Wrap},
};
use std::{cell::RefCell, rc::Rc, sync::OnceLock};
use tokio::sync::mpsc;

static INITIALIZED: OnceLock<Result<(), String>> = OnceLock::new();

struct BrowserEngine {
    core: Option<Core>,
    term: Option<fb_term::Term>,
    events: Option<mpsc::UnboundedReceiver<FbEvent>>,
    error: Option<String>,
    last_area: Rect,
}

impl Default for BrowserEngine {
    fn default() -> Self {
        Self {
            core: None,
            term: None,
            events: None,
            error: None,
            last_area: Rect::default(),
        }
    }
}

impl BrowserEngine {
    pub fn ensure_initialized(&mut self) {
        let result = INITIALIZED.get_or_init(|| {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                fb_shared::init();
                fb_tty::init();
                fb_term::init();
                fb_fs::init();
                fb_config::init_embedded().map_err(|error| error.to_string())?;
                fb_vfs::init();
                fb_adapter::init_embedded().map_err(|error| error.to_string())?;
                fb_boot::init_default();
                fb_dds::init();
                fb_dds::serve();
                fb_widgets::init();
                fb_watcher::init();
                fb_plugin::init().map_err(|error| error.to_string())?;
                Ok(())
            }))
            .unwrap_or_else(|panic| {
                let message = panic
                    .downcast_ref::<&str>()
                    .copied()
                    .or_else(|| panic.downcast_ref::<String>().map(String::as_str))
                    .unwrap_or("unknown panic");
                Err(format!("DX file browser initialization failed: {message}"))
            })
        });
        match result {
            Ok(()) if self.core.is_none() => {
                let mut core = Core::make();
                let mut term = None;
                let cx = &mut fb_actor::Ctx::active(&mut core, &mut term);
                if let Err(error) = act!(app:bootstrap, cx) {
                    self.error = Some(format!("DX file browser bootstrap failed: {error}"));
                    return;
                }
                self.core = Some(core);
                self.events = Some(FbEvent::take());
            }
            Ok(()) => {}
            Err(error) => self.error = Some(error.clone()),
        }
    }

    pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> anyhow::Result<()> {
        self.ensure_initialized();
        let Some(core) = self.core.as_mut() else {
            return Ok(());
        };

        let key = fb_config::keymap::Key::from(key);
        if core.help.visible && core.help.r#type(&key)? {
            return Ok(());
        }
        if core.input.visible && core.input.r#type(&key)? {
            return Ok(());
        }

        let layer = core.layer();
        if layer == Layer::Which {
            core.which.r#type(key);
        } else {
            for chord in fb_config::KEYMAP.get(layer) {
                if chord.on.first() != Some(&key) {
                    continue;
                }
                if chord.on.len() > 1 {
                    let cx = &mut fb_actor::Ctx::active(core, &mut self.term);
                    act!(which:activate, cx, (layer, key)).ok();
                } else {
                    FbEvent::Seq(fb_config::keymap::ChordCow::from(chord).into_seq()).emit();
                }
                break;
            }
        }
        self.pump_events()
    }

    pub fn handle_mouse(&mut self, mouse: crossterm::event::MouseEvent) -> anyhow::Result<()> {
        self.ensure_initialized();
        let Some(core) = self.core.as_mut() else {
            return Ok(());
        };

        // The standalone actor asks `Term` for its size. Embedded mode
        // deliberately has no DX-owned terminal, so dispatch the exact same Lua
        // mouse contract against the most recently rendered Grok area.
        let event = fb_binding::MouseEvent::from(mouse);
        let area = fb_binding::elements::Rect::from(self.last_area);
        fb_actor::lives::Lives::scope(core, move || {
            let root = LUA
                .globals()
                .raw_get::<Table>("Root")?
                .call_method::<Table>("new", area)?;
            if matches!(
                event.kind,
                crossterm::event::MouseEventKind::Down(_)
                    if fb_config::YAZI.mgr.mouse_events.get().draggable()
            ) {
                root.raw_set("_drag_start", event)?;
            }
            match event.kind {
                crossterm::event::MouseEventKind::Down(_) => {
                    root.call_method::<()>("click", (event, false))?
                }
                crossterm::event::MouseEventKind::Up(_) => {
                    root.call_method::<()>("click", (event, true))?
                }
                crossterm::event::MouseEventKind::ScrollDown => {
                    root.call_method::<()>("scroll", (event, 1))?
                }
                crossterm::event::MouseEventKind::ScrollUp => {
                    root.call_method::<()>("scroll", (event, -1))?
                }
                crossterm::event::MouseEventKind::ScrollRight => {
                    root.call_method::<()>("touch", (event, 1))?
                }
                crossterm::event::MouseEventKind::ScrollLeft => {
                    root.call_method::<()>("touch", (event, -1))?
                }
                crossterm::event::MouseEventKind::Moved => root.call_method::<()>("move", event)?,
                crossterm::event::MouseEventKind::Drag(_) => {
                    root.call_method::<()>("drag", event)?
                }
            }
            Ok::<(), mlua::Error>(())
        })?;
        self.pump_events()
    }

    pub fn pump_events(&mut self) -> anyhow::Result<()> {
        let mut pending = Vec::new();
        if let Some(events) = self.events.as_mut() {
            while let Ok(event) = events.try_recv() {
                pending.push(event);
            }
        }
        for event in pending {
            match event {
                FbEvent::Call(action) => {
                    self.execute(action)?;
                }
                FbEvent::Seq(mut actions) => {
                    if let Some(action) = actions.pop() {
                        self.execute(action)?;
                    }
                    if !actions.is_empty() {
                        FbEvent::Seq(actions).emit();
                    }
                }
                FbEvent::Paste(text) => {
                    if let Some(core) = self.core.as_mut() {
                        core.input.type_str(&text)?;
                    }
                }
                FbEvent::Key(key) => self.handle_key(key)?,
                FbEvent::Render(_)
                | FbEvent::Resize
                | FbEvent::Focus
                | FbEvent::Mouse(_)
                | FbEvent::Timer => {}
            }
        }
        Ok(())
    }

    fn execute(&mut self, action: ActionCow) -> anyhow::Result<Data> {
        let Some(core) = self.core.as_mut() else {
            return Ok(Data::Nil);
        };
        let layer = action.layer;
        let name = action.name.clone();
        let cx = &mut fb_actor::Ctx::new(&action, core, &mut self.term)?;

        macro_rules! run {
            ($actor_layer:ident, $($actor:ident),+ $(,)?) => {
                {
                    $(if name == stringify!($actor) {
                        return act!($actor_layer:$actor, cx, action);
                    })+
                }
            };
        }

        match layer {
            Layer::App => run!(
                app,
                accept_payload,
                plugin,
                plugin_do,
                update_progress,
                deprecate
            ),
            Layer::Mgr => run!(
                mgr,
                cd,
                update_yanked,
                update_files,
                update_mimes,
                update_paged,
                watch,
                peek,
                seek,
                spot,
                refresh,
                close,
                escape,
                update_peeked,
                update_spotted,
                arrow,
                parent_arrow,
                leave,
                enter,
                back,
                forward,
                reveal,
                follow,
                toggle,
                toggle_all,
                visual_mode,
                open,
                open_do,
                yank,
                unyank,
                paste,
                link,
                hardlink,
                remove,
                remove_do,
                create,
                rename,
                copy,
                hidden,
                linemode,
                search,
                search_do,
                bulk_rename,
                filter,
                filter_do,
                find,
                find_do,
                find_arrow,
                sort,
                tab_create,
                tab_rename,
                tab_close,
                tab_switch,
                tab_swap,
                download,
                upload,
                displace_do
            ),
            Layer::Tasks => run!(
                tasks,
                update_succeed,
                show,
                close,
                arrow,
                inspect,
                cancel,
                process_open,
                open_shell_compat
            ),
            Layer::Spot => run!(spot, arrow, close, swipe, copy),
            Layer::Pick => run!(pick, show, close, arrow),
            Layer::Input => run!(input, escape, show, close, complete),
            Layer::Confirm => run!(confirm, arrow, show, close),
            Layer::Help => run!(help, escape, arrow, filter),
            Layer::Cmp => run!(cmp, trigger, show, close, arrow),
            Layer::Which => run!(which, activate, dismiss),
            Layer::Notify => run!(notify, push, tick),
        }
        if layer == Layer::Input {
            return core.input.execute(action);
        }
        if name == "help" && !matches!(layer, Layer::App | Layer::Help | Layer::Notify) {
            let cx = &mut fb_actor::Ctx::active(core, &mut self.term);
            return act!(help:toggle, cx, layer);
        }
        if layer == Layer::Help && name == "close" {
            let cx = &mut fb_actor::Ctx::active(core, &mut self.term);
            return act!(help:toggle, cx, Layer::Help);
        }
        if name == "plugin" {
            let cx = &mut fb_actor::Ctx::new(&action, core, &mut self.term)?;
            return act!(app:plugin, cx, action);
        }
        Ok(Data::Nil)
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        self.last_area = area;
        self.ensure_initialized();
        if let Err(error) = self.pump_events() {
            self.error = Some(format!("DX file browser event error: {error}"));
        }
        let Some(core) = self.core.as_ref() else {
            let message = self
                .error
                .as_deref()
                .unwrap_or("DX file browser failed to initialize");
            Paragraph::new(Line::from(message))
                .wrap(Wrap { trim: false })
                .render(area, buf);
            return;
        };

        // DX's Lua components read the live `cx` global. Standalone DX wraps
        // every frame in `Lives::scope`; the embedded host must preserve that
        // exact render contract as well.
        let lua_result = fb_actor::lives::Lives::scope(core, || {
            let lua_area = fb_binding::elements::Rect::from(area);
            let root = LUA
                .globals()
                .raw_get::<Table>("Root")?
                .call_method::<Table>("new", lua_area)?;
            render_once(root.call_method("redraw", ())?, buf, |position| {
                core.mgr.area(position)
            });
            Ok::<(), mlua::Error>(())
        });
        if let Err(error) = lua_result {
            let cwd = core.mgr.cwd();
            Paragraph::new(format!(
                "File Browser\n\n{}\n\nDX layout error: {error}",
                cwd.os_str().to_string_lossy()
            ))
            .wrap(Wrap { trim: false })
            .render(area, buf);
            return;
        }

        mgr::Preview::new(core).render(area, buf);
        mgr::Modal::new(core).render(area, buf);
        if core.tasks.visible {
            tasks::Tasks::new(core).render(area, buf);
        }
        if core.active().spot.visible() {
            spot::Spot::new(core).render(area, buf);
        }
        if core.pick.visible {
            pick::Pick::new(core).render(area, buf);
        }
        if core.input.visible {
            input::Input::new(core).render(area, buf);
        }
        if core.confirm.visible {
            confirm::Confirm::new(core).render(area, buf);
        }
        if core.help.visible {
            help::Help::new(core).render(area, buf);
        }
        if core.cmp.visible {
            cmp::Cmp::new(core).render(area, buf);
        }
        if core.which.active {
            which::Which::new(core).render(area, buf);
        }
    }
}

thread_local! {
    /// DX's file-browser event bus is process-global, so its Core must be too.
    /// Every Grok session receives a lightweight handle to this same engine.
    /// The pager drives all handles on its UI thread, matching mlua's
    /// thread-local runtime contract without introducing terminal ownership.
    static SHARED_ENGINE: Rc<RefCell<BrowserEngine>> =
        Rc::new(RefCell::new(BrowserEngine::default()));
}

pub struct FileBrowserSurface {
    engine: Rc<RefCell<BrowserEngine>>,
}

impl Default for FileBrowserSurface {
    fn default() -> Self {
        SHARED_ENGINE.with(|engine| Self {
            engine: Rc::clone(engine),
        })
    }
}

impl FileBrowserSurface {
    pub fn ensure_initialized(&mut self) {
        self.engine.borrow_mut().ensure_initialized();
    }

    pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> anyhow::Result<()> {
        self.engine.borrow_mut().handle_key(key)
    }

    pub fn handle_mouse(&mut self, mouse: crossterm::event::MouseEvent) -> anyhow::Result<()> {
        self.engine.borrow_mut().handle_mouse(mouse)
    }

    pub fn render(&mut self, area: Rect, buf: &mut Buffer) {
        self.engine.borrow_mut().render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_session_handle_shares_the_process_global_browser_engine() {
        let first = FileBrowserSurface::default();
        let second = FileBrowserSurface::default();
        assert!(Rc::ptr_eq(&first.engine, &second.engine));
    }
}
