// Submenu definitions for all command palette items
mod approval_policy;
mod authentication;
mod developer_instructions;
pub(crate) mod dx_tools;
mod execution_rules;
mod feature_flags;
mod hooks_events;
mod image_vision;
pub(super) mod keyboard_shortcuts;
mod mcp_servers;
mod memory_history;
mod multi_agent;
mod network_proxy;
mod notifications;
mod plugins_apps;
mod profiles;
mod project_trust;
mod providers;
mod sandbox;
mod session_resume;
mod shell_environment;
mod skills;
mod theme;
mod voice_realtime;
mod web_search;
mod worktree;

pub fn get_all_submenus() -> Vec<Vec<(&'static str, &'static str)>> {
    vec![
        theme::get_submenu(),
        keyboard_shortcuts::get_submenu(),
        providers::get_submenu(),
        plugins_apps::get_submenu(),
        skills::get_submenu(),
        sandbox::get_submenu(),
        web_search::get_submenu(),
        mcp_servers::get_submenu(),
        memory_history::get_submenu(),
        multi_agent::get_submenu(),
        notifications::get_submenu(),
        voice_realtime::get_submenu(),
        image_vision::get_submenu(),
        profiles::get_submenu(),
        worktree::get_submenu(),
        authentication::get_submenu(),
        network_proxy::get_submenu(),
        hooks_events::get_submenu(),
        session_resume::get_submenu(),
        approval_policy::get_submenu(),
        shell_environment::get_submenu(),
        execution_rules::get_submenu(),
        project_trust::get_submenu(),
        developer_instructions::get_submenu(),
        feature_flags::get_submenu(),
        dx_tools::get_submenu(),
    ]
}

#[cfg(test)]
mod tests {
    use super::{dx_tools, get_all_submenus};

    #[test]
    fn dx_tools_submenu_index_matches_registered_submenu_order() {
        let submenus = get_all_submenus();

        assert_eq!(
            submenus
                .get(dx_tools::DX_TOOLS_SUBMENU_INDEX)
                .expect("DX Tools submenu"),
            &dx_tools::get_submenu()
        );
    }
}
