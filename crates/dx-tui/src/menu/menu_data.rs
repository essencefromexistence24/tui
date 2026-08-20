// Menu data structures and initialization
use super::keyboard_mappings::KeyboardMappings;
use super::menu_effects::EffectsRepository;
use super::submenus;
use crate::theme::ChatTheme;
use ratatui::layout::Rect;
use tachyonfx::{Duration, Effect, SimpleRng};

/// Dynamic list menus opened by product keys (0 = models, 1 = channels).
pub const DYNAMIC_MODELS: usize = 900;
pub const DYNAMIC_CHANNELS: usize = 901;

pub struct Menu {
    pub active_effect: (&'static str, Effect),
    pub last_tick: Duration,
    pub auto_cycle_timer: Duration,
    pub(super) effects: EffectsRepository,
    pub theme: ChatTheme,
    pub(super) rng: SimpleRng,
    pub selected_menu_item: usize,
    pub hovered_menu_item: Option<usize>,
    pub menu_items: Vec<(String, String)>,
    pub scroll_offset: usize,
    pub menu_item_areas: Vec<Rect>,
    pub menu_area: Rect,
    pub current_submenu: Option<usize>,
    pub(super) main_menu: Vec<(&'static str, &'static str)>,
    pub(super) submenus: Vec<Vec<(&'static str, &'static str)>>,
    pub recording_mode: bool,
    pub keyboard_mappings: KeyboardMappings,
    pub opened_directly: bool, // Track if submenu was opened directly via shortcut
    /// Title override for dynamic product menus (Models / Channels).
    pub custom_title: Option<String>,
}

impl Menu {
    pub fn new(theme: ChatTheme) -> Self {
        let mut rng = SimpleRng::default();
        let effects = EffectsRepository::new(theme, &mut rng);
        let active_effect = effects.get_random_opening_effect(&mut rng);

        let main_menu = get_main_menu();
        let submenus = submenus::get_all_submenus();
        let keyboard_mappings = KeyboardMappings::new();

        Self {
            active_effect,
            last_tick: Duration::ZERO,
            auto_cycle_timer: Duration::ZERO,
            effects,
            theme,
            rng,
            selected_menu_item: 0,
            hovered_menu_item: None,
            menu_items: main_menu
                .iter()
                .map(|(a, b)| (a.to_string(), b.to_string()))
                .collect(),
            scroll_offset: 0,
            menu_item_areas: Vec::new(),
            menu_area: Rect::default(),
            current_submenu: None,
            main_menu,
            submenus,
            recording_mode: false,
            keyboard_mappings,
            opened_directly: false,
            custom_title: None,
        }
    }

    pub fn update(&mut self, elapsed: std::time::Duration) {
        self.last_tick = Duration::from_millis(elapsed.as_millis() as u32);
    }

    /// Open a tachyon-style list (same chrome as Theme / Providers menus).
    pub fn open_dynamic_list(
        &mut self,
        kind: usize,
        title: impl Into<String>,
        items: Vec<(String, String)>,
    ) {
        self.opened_directly = true;
        self.current_submenu = Some(kind);
        self.custom_title = Some(title.into());
        self.menu_items = items;
        self.selected_menu_item = 0;
        self.scroll_offset = 0;
        self.hovered_menu_item = None;
    }

    pub fn is_dynamic_models(&self) -> bool {
        self.current_submenu == Some(DYNAMIC_MODELS)
    }

    pub fn is_dynamic_channels(&self) -> bool {
        self.current_submenu == Some(DYNAMIC_CHANNELS)
    }

    /// Selected item payload (second column) for dynamic lists.
    pub fn selected_payload(&self) -> Option<&str> {
        self.menu_items
            .get(self.selected_menu_item)
            .map(|(_, id)| id.as_str())
    }
}

fn get_main_menu() -> Vec<(&'static str, &'static str)> {
    vec![
        ("1. Theme", ""),
        ("2. Keyboard Shortcuts", ""),
        ("3. Providers", ""),
        ("4. Plugins & Apps", ""),
        ("5. Skills", ""),
        ("6. Sandbox", ""),
        ("7. Web Search", ""),
        ("8. MCP Servers", ""),
        ("9. Memory & History", ""),
        ("10. Multi-Agent", ""),
        ("11. Notifications", ""),
        ("12. Voice / Realtime", ""),
        ("13. Image & Vision", ""),
        ("14. Profiles", ""),
        ("15. Worktree", ""),
        ("16. Authentication", ""),
        ("17. Network & Proxy", ""),
        ("18. Hooks & Events", ""),
        ("19. Session Resume", ""),
        ("20. Approval Policy", ""),
        ("21. Shell Environment", ""),
        ("22. Execution Rules", ""),
        ("23. Project Trust", ""),
        ("24. Developer Instructions", ""),
        ("25. Feature Flags", ""),
        ("26. DX Tools", ""),
        // TODO: Add openclaw sidebar entry
        // TODO: Add hermes-agent sidebar entry
        // TODO: Add deer-flow sidebar entry
    ]
}
