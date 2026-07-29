// Dynamic keyboard shortcut mappings
use std::collections::HashMap;
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MenuAction {
    ContextControlPanel,
    Theme,
    KeyboardShortcuts,
    Providers,
    PluginsApps,
    Skills,
    Sandbox,
    WebSearch,
    McpServers,
    MemoryHistory,
    MultiAgent,
    Notifications,
    VoiceRealtime,
    ImageVision,
    Profiles,
    Worktree,
    Authentication,
    NetworkProxy,
    HooksEvents,
    SessionResume,
    ApprovalPolicy,
    ShellEnvironment,
    ExecutionRules,
    ProjectTrust,
    DeveloperInstructions,
    FeatureFlags,
    DxTools,
}

impl MenuAction {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::ContextControlPanel => "Context Control Panel",
            Self::Theme => "Theme",
            Self::KeyboardShortcuts => "Keyboard Shortcuts",
            Self::Providers => "Providers",
            Self::PluginsApps => "Plugins & Apps",
            Self::Skills => "Skills",
            Self::Sandbox => "Sandbox",
            Self::WebSearch => "Web Search",
            Self::McpServers => "MCP Servers",
            Self::MemoryHistory => "Memory & History",
            Self::MultiAgent => "Multi-Agent",
            Self::Notifications => "Notifications",
            Self::VoiceRealtime => "Voice / Realtime",
            Self::ImageVision => "Image & Vision",
            Self::Profiles => "Profiles",
            Self::Worktree => "Worktree",
            Self::Authentication => "Authentication",
            Self::NetworkProxy => "Network & Proxy",
            Self::HooksEvents => "Hooks & Events",
            Self::SessionResume => "Session Resume",
            Self::ApprovalPolicy => "Approval Policy",
            Self::ShellEnvironment => "Shell Environment",
            Self::ExecutionRules => "Execution Rules",
            Self::ProjectTrust => "Project Trust",
            Self::DeveloperInstructions => "Developer Instructions",
            Self::FeatureFlags => "Feature Flags",
            Self::DxTools => "DX Tools",
        }
    }

    pub fn all_actions() -> Vec<Self> {
        vec![
            Self::ContextControlPanel,
            Self::Theme,
            Self::KeyboardShortcuts,
            Self::Providers,
            Self::PluginsApps,
            Self::Skills,
            Self::Sandbox,
            Self::WebSearch,
            Self::McpServers,
            Self::MemoryHistory,
            Self::MultiAgent,
            Self::Notifications,
            Self::VoiceRealtime,
            Self::ImageVision,
            Self::Profiles,
            Self::Worktree,
            Self::Authentication,
            Self::NetworkProxy,
            Self::HooksEvents,
            Self::SessionResume,
            Self::ApprovalPolicy,
            Self::ShellEnvironment,
            Self::ExecutionRules,
            Self::ProjectTrust,
            Self::DeveloperInstructions,
            Self::FeatureFlags,
            Self::DxTools,
        ]
    }
}

pub struct KeyboardMappings {
    mappings: Arc<RwLock<HashMap<MenuAction, String>>>,
}

impl KeyboardMappings {
    pub fn new() -> Self {
        let mut mappings = HashMap::new();

        // Default keyboard shortcuts
        mappings.insert(MenuAction::ContextControlPanel, "0 or Ctrl+P".to_string());
        mappings.insert(MenuAction::Theme, "Ctrl+T".to_string());
        mappings.insert(MenuAction::KeyboardShortcuts, "Ctrl+K".to_string());
        mappings.insert(MenuAction::Providers, "Ctrl+Shift+P".to_string());
        mappings.insert(MenuAction::PluginsApps, "Ctrl+Shift+A".to_string());
        mappings.insert(MenuAction::Skills, "Ctrl+Shift+S".to_string());
        mappings.insert(MenuAction::Sandbox, "Ctrl+Shift+B".to_string());
        mappings.insert(MenuAction::WebSearch, "Ctrl+Shift+W".to_string());
        mappings.insert(MenuAction::McpServers, "Ctrl+Shift+M".to_string());
        mappings.insert(MenuAction::MemoryHistory, "Ctrl+Shift+H".to_string());
        mappings.insert(MenuAction::MultiAgent, "Ctrl+Shift+G".to_string());
        mappings.insert(MenuAction::Notifications, "Ctrl+Shift+N".to_string());
        mappings.insert(MenuAction::VoiceRealtime, "Ctrl+Shift+V".to_string());
        mappings.insert(MenuAction::ImageVision, "Ctrl+Shift+I".to_string());
        mappings.insert(MenuAction::Profiles, "Ctrl+Shift+R".to_string());
        mappings.insert(MenuAction::Worktree, "Ctrl+Shift+T".to_string());
        mappings.insert(MenuAction::Authentication, "Ctrl+Shift+U".to_string());
        mappings.insert(MenuAction::NetworkProxy, "Ctrl+Shift+X".to_string());
        mappings.insert(MenuAction::HooksEvents, "Ctrl+Shift+E".to_string());
        mappings.insert(MenuAction::SessionResume, "Ctrl+Shift+L".to_string());
        mappings.insert(MenuAction::ApprovalPolicy, "Ctrl+Shift+O".to_string());
        mappings.insert(MenuAction::ShellEnvironment, "Ctrl+Shift+J".to_string());
        mappings.insert(MenuAction::ExecutionRules, "Ctrl+Shift+C".to_string());
        mappings.insert(MenuAction::ProjectTrust, "Ctrl+Shift+Y".to_string());
        mappings.insert(
            MenuAction::DeveloperInstructions,
            "Ctrl+Shift+D".to_string(),
        );
        mappings.insert(MenuAction::FeatureFlags, "Ctrl+Shift+F".to_string());
        mappings.insert(MenuAction::DxTools, "Ctrl+Shift+Z".to_string());

        Self {
            mappings: Arc::new(RwLock::new(mappings)),
        }
    }

    pub fn get(&self, action: MenuAction) -> String {
        self.read_mappings()
            .get(&action)
            .cloned()
            .unwrap_or_else(|| "Not Set".to_string())
    }

    pub fn set(&self, action: MenuAction, shortcut: String) {
        self.write_mappings().insert(action, shortcut);
    }

    #[allow(dead_code)]
    pub fn get_all(&self) -> Vec<(MenuAction, String)> {
        MenuAction::all_actions()
            .into_iter()
            .map(|action| {
                let shortcut = self.get(action);
                (action, shortcut)
            })
            .collect()
    }

    pub fn clone_inner(&self) -> Self {
        Self {
            mappings: Arc::clone(&self.mappings),
        }
    }

    fn read_mappings(&self) -> RwLockReadGuard<'_, HashMap<MenuAction, String>> {
        self.mappings
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn write_mappings(&self) -> RwLockWriteGuard<'_, HashMap<MenuAction, String>> {
        self.mappings
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl Default for KeyboardMappings {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for KeyboardMappings {
    fn clone(&self) -> Self {
        self.clone_inner()
    }
}
