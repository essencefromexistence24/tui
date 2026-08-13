//! In-process host for the File Browser engine.
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
use fb_config::LAYOUT;
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
    style::{Color, Style},
    text::Line,
    widgets::{Paragraph, Widget, Wrap},
};
use std::{cell::RefCell, rc::Rc, sync::OnceLock};
use tokio::sync::mpsc;

static INITIALIZED: OnceLock<Result<(), String>> = OnceLock::new();

/// Format a ratatui color as a YAZI/Lua TOML color value.
///
/// The active pager theme can contain RGB, ANSI named, reset, or indexed
/// colors depending on terminal capabilities. Dropping the non-RGB variants
/// makes the file browser silently fall back to its embedded preset, which is
/// why this conversion must preserve every color representation.
fn color_value(color: Color) -> Option<String> {
    match color {
        Color::Reset => Some("reset".to_owned()),
        Color::Black => Some("black".to_owned()),
        Color::Red => Some("red".to_owned()),
        Color::Green => Some("green".to_owned()),
        Color::Yellow => Some("yellow".to_owned()),
        Color::Blue => Some("blue".to_owned()),
        Color::Magenta => Some("magenta".to_owned()),
        Color::Cyan => Some("cyan".to_owned()),
        Color::Gray => Some("gray".to_owned()),
        Color::DarkGray => Some("darkgray".to_owned()),
        Color::LightRed => Some("lightred".to_owned()),
        Color::LightGreen => Some("lightgreen".to_owned()),
        Color::LightYellow => Some("lightyellow".to_owned()),
        Color::LightBlue => Some("lightblue".to_owned()),
        Color::LightMagenta => Some("lightmagenta".to_owned()),
        Color::LightCyan => Some("lightcyan".to_owned()),
        Color::White => Some("white".to_owned()),
        Color::Indexed(index) => Some(index.to_string()),
        Color::Rgb(r, g, b) => Some(format!("#{r:02x}{g:02x}{b:02x}")),
    }
}

/// Build a TOML theme override that maps the active pager palette onto the
/// YAZI file-browser theme. Layout symbols, strings, and the syntect path keep
/// their embedded defaults, while chrome, file rows, and icons all receive
/// colors from the live Grok palette.
fn fb_theme_override(theme: &crate::theme::Theme) -> String {
    let style = |fg: Option<String>, bg: Option<String>, bold: bool| {
        let mut out = String::from("{");
        let mut first = true;
        for (key, value) in [("fg", fg), ("bg", bg)] {
            if let Some(value) = value {
                if !first {
                    out.push_str(", ");
                }
                out.push_str(key);
                out.push_str(" = \"");
                out.push_str(&value);
                out.push('"');
                first = false;
            }
        }
        if bold {
            if !first {
                out.push_str(", ");
            }
            out.push_str("bold = true");
        }
        out.push('}');
        out
    };
    let color = |c: Color| color_value(c);
    let bg = color(theme.bg_base);
    let surface = color(theme.bg_light);
    let raised = color(theme.bg_dark);
    let bg_hl = color(theme.bg_highlight);
    let hover = color(theme.bg_hover);
    let selected = color(theme.bg_visual);
    let fg = color(theme.text_primary);
    let secondary = color(theme.text_secondary);
    let dim = color(theme.gray_dim);
    let bright = color(theme.gray_bright);
    let accent = color(theme.accent_user);
    let assistant = color(theme.accent_assistant);
    let tool = color(theme.accent_tool);
    let system = color(theme.accent_system);
    let ok = color(theme.accent_success);
    let running = color(theme.accent_running);
    let err = color(theme.accent_error);
    let warn = color(theme.warning);
    let fuzzy = color(theme.fuzzy_accent);
    let border_color = color(theme.selection_border);
    let file = color(theme.text_primary).expect("every ratatui color has a TOML value");
    let directory = color(theme.accent_user).expect("every ratatui color has a TOML value");
    let image = color(theme.accent_assistant).expect("every ratatui color has a TOML value");
    let media = color(theme.accent_assistant).expect("every ratatui color has a TOML value");
    let archive = color(theme.accent_error).expect("every ratatui color has a TOML value");
    let document = color(theme.text_secondary).expect("every ratatui color has a TOML value");
    let executable = color(theme.accent_success).expect("every ratatui color has a TOML value");
    let link = color(theme.gray_dim).expect("every ratatui color has a TOML value");

    format!(
        r#"
[app]
overall = {app}

[mgr]
cwd = {cwd}
find_keyword = {find_kw}
find_position = {find_pos}
symlink_target = {symlink}
marker_copied = {mcopied}
marker_cut = {mcut}
marker_marked = {mmarked}
marker_selected = {mselected}
count_copied = {ccopied}
count_cut = {ccut}
count_selected = {cselected}
border_style = {border}

[tabs]
active = {tabs_active}
inactive = {tabs_inactive}

[mode]
normal_main = {mode_main}
normal_alt = {mode_alt}
select_main = {select_main}
select_alt = {select_alt}
unset_main = {unset_main}
unset_alt = {unset_alt}

[indicator]
parent = {{ reversed = true }}
current = {{ reversed = true }}
preview = {{ underline = true }}

[status]
overall = {status_overall}
perm_sep = {perm_sep}
perm_type = {perm_type}
perm_read = {perm_read}
perm_write = {perm_write}
perm_exec = {perm_exec}
progress_label = {{ bold = true }}
progress_normal = {progress_normal}
progress_error = {progress_error}

[which]
cols = 3
mask = {which_mask}
cand = {which_cand}
rest = {which_rest}
desc = {which_desc}

[confirm]
border = {border}
title = {title}
body = {body}
list = {body}
btn_yes = {btn_yes}
btn_no = {btn_no}

[spot]
border = {border}
title = {title}
tbl_col = {tbl_col}
tbl_cell = {body}

[notify]
title_info = {notify_info}
title_warn = {notify_warn}
title_error = {notify_error}

[pick]
border = {border}
active = {pick_active}
inactive = {pick_inactive}

[input]
border = {border}
title = {title}
value = {body}
selected = {input_selected}

[cmp]
border = {border}
active = {pick_active}
inactive = {pick_inactive}

[tasks]
border = {border}
title = {title}
hovered = {tasks_hovered}

[help]
on = {help_on}
run = {body}
desc = {body}
hovered = {tasks_hovered}
footer = {footer}

[filetype]
rules = [
  {{ mime = "image/*", fg = "{image}" }},
  {{ mime = "{{audio,video}}/*", fg = "{media}" }},
  {{ mime = "application/{{zip,rar,7z*,tar,gzip,xz,zstd,bzip*,lzma,compress,archive,cpio,arj,xar,ms-cab*}}", fg = "{archive}" }},
  {{ mime = "application/{{pdf,doc,rtf}}", fg = "{document}" }},
  {{ mime = "vfs/{{absent,stale}}", fg = "{link}" }},
  {{ url = "*", is = "orphan", bg = "{archive}" }},
  {{ url = "*", is = "exec", fg = "{executable}" }},
  {{ url = "*", is = "dummy", bg = "{archive}" }},
  {{ url = "*/", is = "dummy", bg = "{archive}" }},
  {{ url = "*/", fg = "{directory}" }},
  {{ url = "*", fg = "{file}" }},
]

[icon]
conds = [
  {{ if = "orphan", text = "", fg = "{archive}" }},
  {{ if = "link", text = "", fg = "{link}" }},
  {{ if = "block", text = "", fg = "{image}" }},
  {{ if = "char", text = "", fg = "{image}" }},
  {{ if = "fifo", text = "", fg = "{image}" }},
  {{ if = "sock", text = "", fg = "{image}" }},
  {{ if = "sticky", text = "", fg = "{image}" }},
  {{ if = "dummy", text = "", fg = "{archive}" }},
  {{ if = "dir & hovered", text = "", fg = "{file}" }},
  {{ if = "dir", text = "", fg = "{directory}" }},
  {{ if = "exec", text = "", fg = "{executable}" }},
  {{ if = "!dir", text = "", fg = "{file}" }},
]
"#,
        app = style(fg.clone(), bg.clone(), false),
        cwd = style(accent.clone(), None, true),
        find_kw = style(fuzzy.clone(), None, true),
        find_pos = style(secondary.clone(), None, true),
        symlink = style(accent.clone(), None, true),
        mcopied = style(ok.clone(), ok.clone(), false),
        mcut = style(err.clone(), err.clone(), false),
        mmarked = style(assistant.clone(), assistant.clone(), false),
        mselected = style(accent.clone(), accent.clone(), false),
        ccopied = style(fg.clone(), ok.clone(), false),
        ccut = style(fg.clone(), err.clone(), false),
        cselected = style(bg.clone(), accent.clone(), false),
        border = style(border_color, None, false),
        tabs_active = style(fg.clone(), bg_hl.clone(), true),
        tabs_inactive = style(secondary.clone(), raised.clone(), false),
        mode_main = style(bg.clone(), accent.clone(), true),
        mode_alt = style(secondary.clone(), surface.clone(), false),
        select_main = style(fg.clone(), selected.clone(), true),
        select_alt = style(assistant.clone(), surface.clone(), false),
        unset_main = style(fg.clone(), err.clone(), true),
        unset_alt = style(err.clone(), surface.clone(), false),
        status_overall = style(None, raised.clone(), false),
        perm_sep = style(dim.clone(), None, false),
        perm_type = style(tool.clone(), None, false),
        perm_read = style(warn.clone(), None, false),
        perm_write = style(err.clone(), None, false),
        perm_exec = style(ok.clone(), None, false),
        progress_normal = style(running.clone(), bg.clone(), false),
        progress_error = style(warn.clone(), err.clone(), false),
        which_mask = style(None, surface.clone(), false),
        which_cand = style(fuzzy, None, false),
        which_rest = style(dim.clone(), None, false),
        which_desc = style(secondary.clone(), None, false),
        title = style(system, None, true),
        body = style(fg.clone(), None, false),
        tbl_col = style(bright, None, false),
        btn_yes = style(bg.clone(), ok.clone(), true),
        btn_no = style(bg.clone(), err.clone(), true),
        notify_info = style(running, None, false),
        notify_warn = style(warn.clone(), None, false),
        notify_error = style(err.clone(), None, false),
        pick_active = style(fg.clone(), selected.clone(), false),
        pick_inactive = style(secondary, None, false),
        input_selected = style(fg.clone(), selected, false),
        tasks_hovered = style(None, hover, false),
        help_on = style(ok.clone(), None, false),
        footer = style(dim.clone(), None, false),
        file = file,
        directory = directory,
        image = image,
        media = media,
        archive = archive,
        document = document,
        executable = executable,
        link = link,
    )
}

struct BrowserEngine {
    core: Option<Core>,
    term: Option<fb_term::Term>,
    events: Option<mpsc::UnboundedReceiver<FbEvent>>,
    error: Option<String>,
    last_area: Rect,
    applied_theme: Option<(bool, String)>,
}

impl Default for BrowserEngine {
    fn default() -> Self {
        Self {
            core: None,
            term: None,
            events: None,
            error: None,
            last_area: Rect::default(),
            applied_theme: None,
        }
    }
}

impl BrowserEngine {
    pub fn ensure_initialized(&mut self) {
        let pager_theme = crate::theme::Theme::current();
        let override_toml = fb_theme_override(&pager_theme);
        let light = !pager_theme.is_dark();
        let result = INITIALIZED.get_or_init(|| {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                fb_shared::init();
                fb_tty::init();
                fb_term::init();
                fb_fs::init();
                fb_config::init_embedded().map_err(|error| error.to_string())?;
                fb_vfs::init();
                fb_adapter::init_embedded().map_err(|error| error.to_string())?;
                fb_config::override_theme(light, &override_toml)
                    .map_err(|error| error.to_string())?;
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
                Err(format!("File Browser initialization failed: {message}"))
            })
        });
        match result {
            Ok(()) if self.core.is_none() => {
                self.applied_theme = Some((light, override_toml));
                let mut core = Core::make();
                let mut term = None;
                let cx = &mut fb_actor::Ctx::active(&mut core, &mut term);
                if let Err(error) = act!(app:bootstrap, cx) {
                    self.error = Some(format!("File Browser bootstrap failed: {error}"));
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
        // The embedded browser renders directly into Grok's frame and does
        // not run the standalone terminal's background clear. Fill the whole
        // surface first so cells that are not touched by a browser widget
        // still use the active Grok theme instead of a previous frame's
        // colors (or Ratatui's reset color).
        let theme = crate::theme::Theme::current();
        buf.set_style(
            area,
            Style::default().fg(theme.text_primary).bg(theme.bg_base),
        );
        self.ensure_initialized();
        // Full theme sync: push the active pager palette into the YAZI Lua
        // theme so the file browser follows the global theme. Compare the
        // generated values, not just the theme name: terminal quantization,
        // auto light/dark changes, and runtime palette updates can all change
        // the colors while the canonical name stays the same.
        let override_toml = fb_theme_override(&theme);
        let light = !theme.is_dark();
        let theme_signature = (light, override_toml.clone());
        if self.applied_theme.as_ref() != Some(&theme_signature) {
            if let Err(error) = fb_config::override_theme(light, &override_toml) {
                tracing::warn!(%error, "File Browser theme override failed");
            } else {
                self.applied_theme = Some(theme_signature);
            }
        }
        if let Err(error) = self.pump_events() {
            self.error = Some(format!("File Browser event error: {error}"));
        }
        let Some(core) = self.core.as_mut() else {
            let message = self
                .error
                .as_deref()
                .unwrap_or("File Browser failed to initialize");
            Paragraph::new(Line::from(message))
                .wrap(Wrap { trim: false })
                .render(area, buf);
            return;
        };

        // DX's Lua components read the live `cx` global. Standalone DX wraps
        // every frame in `Lives::scope`; the embedded host must preserve that
        // exact render contract as well.
        let previous_preview_area = LAYOUT.get().preview;
        let lua_result = fb_actor::lives::Lives::scope(core, || {
            let lua_area = fb_binding::elements::Rect::from(area);
            let root = LUA
                .globals()
                .raw_get::<Table>("Root")?
                .call_method::<Table>("new", lua_area)?;
            let components: Table = root.call_method("reflow", ())?;
            let mut layout = LAYOUT.get();
            for component in components.sequence_values::<mlua::Value>() {
                let mlua::Value::Table(component) = component? else {
                    continue;
                };
                let Some(id) = component.raw_get::<Option<mlua::String>>("_id")? else {
                    continue;
                };
                let Some(component_area) = component
                    .raw_get::<Option<fb_binding::elements::Rect>>("_area")?
                    .map(|area| *area)
                else {
                    continue;
                };
                match id.as_bytes().as_ref() {
                    b"current" => layout.current = component_area,
                    b"preview" => layout.preview = component_area,
                    b"progress" => layout.progress = component_area,
                    _ => {}
                }
            }
            if layout != LAYOUT.get() {
                LAYOUT.set(layout);
            }
            let elements = root.call_method("redraw", ())?;
            render_once(elements, buf, |position| core.mgr.area(position));
            Ok::<(), mlua::Error>(())
        });
        if let Err(error) = lua_result {
            let cwd = core.mgr.cwd();
            Paragraph::new(format!(
                "File Browser\n\n{}\n\nFile Browser layout error: {error}",
                cwd.os_str().to_string_lossy()
            ))
            .wrap(Wrap { trim: false })
            .render(area, buf);
            return;
        }

        if previous_preview_area != LAYOUT.get().preview {
            let cx = &mut fb_actor::Ctx::active(core, &mut self.term);
            if let Err(error) = act!(mgr:peek, cx) {
                self.error = Some(format!("File Browser preview failed: {error}"));
            }
        }
        mgr::Preview::new(core).render(area, buf);
        fb_adapter::render_embedded_image(area, buf);
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

    #[test]
    fn active_theme_colors_round_trip_to_file_browser_values() {
        assert_eq!(color_value(Color::Reset).as_deref(), Some("reset"));
        assert_eq!(color_value(Color::DarkGray).as_deref(), Some("darkgray"));
        assert_eq!(color_value(Color::Indexed(42)).as_deref(), Some("42"));
        assert_eq!(
            color_value(Color::Rgb(12, 34, 56)).as_deref(),
            Some("#0c2238")
        );
    }

    #[test]
    fn theme_carousel_browser_uses_live_primary_and_semantic_colors() {
        let mut theme = crate::theme::Theme::tokyonight();
        theme.bg_base = Color::Rgb(1, 2, 3);
        theme.bg_hover = Color::Rgb(4, 5, 6);
        theme.bg_visual = Color::Rgb(7, 8, 9);
        theme.fuzzy_accent = Color::Rgb(10, 11, 12);
        theme.accent_error = Color::Rgb(13, 14, 15);
        theme.accent_user = Color::Rgb(16, 17, 18);
        // TokyoNight's path role is warm/orange; directory and symlink text
        // must follow the selected primary accent instead.
        theme.path = Color::Rgb(255, 128, 0);

        let value: toml::Value = toml::from_str(&fb_theme_override(&theme)).expect("valid TOML");
        assert_eq!(value["app"]["overall"]["bg"].as_str(), Some("#010203"));
        assert_eq!(value["tasks"]["hovered"]["bg"].as_str(), Some("#040506"));
        assert_eq!(value["pick"]["active"]["bg"].as_str(), Some("#070809"));
        assert_eq!(value["which"]["cand"]["fg"].as_str(), Some("#0a0b0c"));
        assert_eq!(
            value["notify"]["title_error"]["fg"].as_str(),
            Some("#0d0e0f")
        );
        assert_eq!(
            value["mgr"]["symlink_target"]["fg"].as_str(),
            Some("#101112")
        );
        assert_eq!(
            value["filetype"]["rules"][9]["fg"].as_str(),
            Some("#101112")
        );
        let primary = color_value(theme.text_primary);
        assert_eq!(value["app"]["overall"]["fg"].as_str(), primary.as_deref());
        assert_eq!(
            value["filetype"]["rules"][10]["fg"].as_str(),
            primary.as_deref()
        );
        assert!(
            fb_config::validate_theme_override(!theme.is_dark(), &fb_theme_override(&theme))
                .is_ok()
        );
    }
}
