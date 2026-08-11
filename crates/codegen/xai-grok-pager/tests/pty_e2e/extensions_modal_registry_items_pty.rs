// Per-test-case module for the `pty_e2e` integration test crate.
//
// End-to-end proof (PROBLEM.md acceptance #12) that real Grok Build registry
// items — hooks, plugins, marketplace sources, skills, and MCP servers —
// travel from the embedded shell response into the right extensions modal tab
// and render as items. Each registry is seeded with one fixture; the test
// opens each original tab via its slash command and waits for the item.
#[allow(unused_imports)]
use super::common::*;

const FIXTURE_HOOK_NAME: &str = "user:PreToolUse[0].hooks[0]";
const FIXTURE_PLUGIN: &str = "ptytestplugin";
const FIXTURE_SKILL: &str = "ptytestskill";
const FIXTURE_MARKETPLACE_SOURCE: &str = "Local Fixture";
const FIXTURE_MARKETPLACE_PLUGIN: &str = "ptytestmp";
const FIXTURE_MCP: &str = "ptytestmcp";

fn dump_screen(label: &str, harness: &PtyHarness) {
    let screen = harness.screen_contents();
    eprintln!(
        "\n========== PTY CAPTURE: {label} ==========\n{screen}\n========== END: {label} ==========\n"
    );
}

/// Seed one fixture per Grok Build registry under the harness's fake
/// `GROK_HOME`, plus a local marketplace source directory carrying one plugin.
fn seed_registry_fixtures(content: &ContentController) {
    let grok_home = content.home().join(".grok");
    std::fs::create_dir_all(&grok_home).expect("create fake GROK_HOME");

    // User plugin (~/.grok/plugins/<name>/plugin.json), proven fixture shape.
    let plugin_dir = grok_home.join("plugins").join(FIXTURE_PLUGIN);
    std::fs::create_dir_all(&plugin_dir).expect("create plugin dir");
    std::fs::write(
        plugin_dir.join("plugin.json"),
        format!(
            r#"{{"name":"{FIXTURE_PLUGIN}","version":"0.0.1","description":"extensions modal registry fixture"}}"#
        ),
    )
    .expect("write plugin.json");

    // User skill (~/.grok/skills/<name>/SKILL.md with frontmatter).
    let skill_dir = grok_home.join("skills").join(FIXTURE_SKILL);
    std::fs::create_dir_all(&skill_dir).expect("create skill dir");
    std::fs::write(
        skill_dir.join("SKILL.md"),
        format!(
            "---\nname: {FIXTURE_SKILL}\ndescription: extensions modal registry fixture skill\n---\n\n# {FIXTURE_SKILL}\n\nRun the registry fixture steps.\n"
        ),
    )
    .expect("write SKILL.md");

    // Local marketplace source: a temp dir carrying one plugin. The child
    // scans it directly (no git needed). Forward slashes keep the TOML
    // literal valid on Windows.
    let mp_src = tempfile::tempdir().expect("marketplace source tempdir");
    let mp_root = mp_src.path().to_string_lossy().replace('\\', "/");
    let mp_plugin = mp_src.path().join(FIXTURE_MARKETPLACE_PLUGIN);
    std::fs::create_dir_all(&mp_plugin).expect("create marketplace plugin dir");
    std::fs::write(
        mp_plugin.join("plugin.json"),
        format!(
            r#"{{"name":"{FIXTURE_MARKETPLACE_PLUGIN}","version":"0.0.1","description":"marketplace registry fixture"}}"#
        ),
    )
    .expect("write marketplace plugin.json");

    #[cfg(not(windows))]
    let mcp_command = "/bin/cat";
    #[cfg(windows)]
    let mcp_command = "cmd.exe";

    let config = format!(
        r#"
[plugins]
enabled = ["{FIXTURE_PLUGIN}"]

[[hooks.PreToolUse]]
matcher = "Bash"

[[hooks.PreToolUse.hooks]]
type = "command"
command = "echo ptyhook"

[[marketplace.sources]]
name = "{FIXTURE_MARKETPLACE_SOURCE}"
path = "{mp_root}"

[mcp_servers.{FIXTURE_MCP}]
command = "{mcp_command}"
args = []
startup_timeout_sec = 2
"#
    );
    std::fs::write(grok_home.join("config.toml"), config).expect("write config.toml");
}

fn open_tab(harness: &mut PtyHarness, slash: &[u8], tab_label: &str) {
    harness.inject_keys(slash).expect("submit slash command");
    harness
        .wait_for_text(tab_label, Duration::from_secs(15))
        .expect("extensions modal tab chrome");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "PTY e2e; run the owning pty_e2e_* Cargo test with --ignored (see Cargo.toml)"]
async fn extensions_modal_registry_items_pty() {
    let content = ContentController::start().await.expect("start content");
    seed_registry_fixtures(&content);

    let binary = pager_binary().expect("resolve pager binary");
    let mut harness =
        PtyHarness::spawn_with_content(&binary, DEFAULT_ROWS, DEFAULT_COLS, &content, &[])
            .expect("spawn pager");

    harness
        .wait_for_text(WELCOME_SCREEN_SENTINEL, WELCOME_TIMEOUT)
        .expect("welcome text");

    // ── Hooks tab: config-layer hook renders as a row. ───────────────────
    open_tab(&mut harness, b"/hooks\r", "Hooks");
    harness
        .wait_for_text(FIXTURE_HOOK_NAME, Duration::from_secs(30))
        .expect("config-layer hook row loaded in modal");

    // ── Plugins tab: user plugin renders (groups seed collapsed). ────────
    open_tab(&mut harness, b"/plugins\r", "Plugins");
    harness
        .wait_for_text("(1 plugin)", Duration::from_secs(20))
        .expect("plugin source group header");
    harness.inject_keys(b"l").expect("expand plugin group");
    harness
        .wait_for_text(FIXTURE_PLUGIN, Duration::from_secs(20))
        .expect("plugin row loaded in modal");

    // ── Marketplace tab: local source scans and renders its plugin. ──────
    open_tab(&mut harness, b"/marketplace\r", "Marketplace");
    harness
        .wait_for_text(FIXTURE_MARKETPLACE_SOURCE, Duration::from_secs(30))
        .expect("marketplace source header");
    harness
        .wait_for_text(FIXTURE_MARKETPLACE_PLUGIN, Duration::from_secs(30))
        .expect("marketplace plugin row loaded in modal");

    // ── Skills tab: user skill renders (groups seed collapsed). ──────────
    open_tab(&mut harness, b"/skills\r", "Skills");
    harness
        .wait_for_text("User (1 skill)", Duration::from_secs(30))
        .expect("skill source group header");
    harness.inject_keys(b"l").expect("expand skill group");
    harness
        .wait_for_text(FIXTURE_SKILL, Duration::from_secs(20))
        .expect("skill row loaded in modal");

    // ── MCP Servers tab: configured server renders. ──────────────────────
    open_tab(&mut harness, b"/mcps\r", "MCP Servers");
    harness
        .wait_for_text(FIXTURE_MCP, Duration::from_secs(30))
        .expect("MCP server loaded in modal");

    assert!(
        !harness.contains_text("panicked"),
        "pager panicked\nscreen:\n{}",
        harness.screen_contents()
    );

    dump_screen("extensions modal all five registry tabs", &harness);
    harness.quit().expect("clean quit");
}