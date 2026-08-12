# Problem: Extensions tabs show no Grok Build items

## Summary

In the main DX TUI extensions modal, these original Grok Build tabs appear empty:

- Hooks
- Plugins
- Marketplace
- Skills
- MCP Servers

Providers and Connect are separate DX/ZeroClaw integrations and are not the source of the original registry data. The required fix is to restore the original Grok Build extension registries and render their real items without replacing them with ZeroClaw data or fabricated placeholders.

## User-visible symptoms

When the extensions menu is opened and the user selects Hooks, Plugins, Marketplace, Skills, or MCP Servers, the list contains no useful original Grok Build entries. These tabs were believed to work previously, but after provider/channel integration they appear empty.

The TUI previously rendered a generic picker empty state ('No matches') when a loaded registry contained zero entries. That made a valid empty response indistinguishable from a broken request, dropped response, or failed registry connection.

## Scope boundary

Do not replace the existing Grok Build extension tabs with ZeroClaw data.

- Providers and Connect are separate tabs.
- Hooks, Plugins, Marketplace, Skills, and MCP Servers must use the existing Grok Build ACP extension endpoints and response types.
- Do not create fake built-in entries merely to make the UI look populated.
- Do not delete or overwrite existing user configuration.

## Expected data flow

    Extensions modal opens or refreshes
        -> pager creates ACP extension requests
        -> embedded Grok shell routes the requests
        -> shell discovers the real configured registries
        -> shell returns { result: ... }
        -> pager decodes the result
        -> TaskResult updates ExtensionsModalState
        -> renderer displays the actual items

## Pager request generation

The shared fetch function is:

- G:/Dx/tui/crates/codegen/xai-grok-pager/src/app/dispatch/transcript.rs
- Function: extensions_modal_tab_fetches

It requests:

- x.ai/hooks/list
- x.ai/plugins/list
- x.ai/mcp/list
- x.ai/skills/list
- x.ai/workflows/list
- x.ai/marketplace/list

The request effects are implemented in:

- G:/Dx/tui/crates/codegen/xai-grok-pager/src/app/effects/mod.rs

Relevant effects:

- Effect::FetchHooksList
- Effect::FetchPluginsList
- Effect::FetchMcpsList
- Effect::FetchSkillsList
- Effect::FetchWorkflowsList
- Effect::FetchMarketplaceList

## Shell routing

The embedded shell routes the methods in:

- G:/Dx/tui/crates/codegen/xai-grok-shell/src/agent/mvp_agent/acp_agent.rs

Current route groups:

- x.ai/hooks/* -> extensions::hooks::handle
- x.ai/plugins/* -> extensions::plugins::handle
- x.ai/marketplace/* -> extensions::marketplace::handle
- x.ai/mcp/* -> extensions::mcp::handle
- x.ai/skills/* and x.ai/workflows/list -> extensions::skills::handle

Relevant handlers:

- G:/Dx/tui/crates/codegen/xai-grok-shell/src/extensions/hooks.rs
- G:/Dx/tui/crates/codegen/xai-grok-shell/src/extensions/plugins.rs
- G:/Dx/tui/crates/codegen/xai-grok-shell/src/extensions/marketplace.rs
- G:/Dx/tui/crates/codegen/xai-grok-shell/src/extensions/mcp.rs
- G:/Dx/tui/crates/codegen/xai-grok-shell/src/extensions/skills.rs

Shell responses use this envelope:

    {
      "result": {},
      "error": null
    }

The pager unwraps result before deserializing.

## Pager result handling

Task results are handled in:

- G:/Dx/tui/crates/codegen/xai-grok-pager/src/app/dispatch/task_result.rs
- G:/Dx/tui/crates/codegen/xai-grok-pager/src/app/dispatch/transcript.rs

Expected mappings:

- HooksListLoaded -> modal.hooks_data
- PluginsListLoaded -> modal.plugins_data
- MarketplaceListLoaded -> modal.marketplace_data
- SkillsListLoaded -> modal.skills_data
- McpsListLoaded -> modal.mcps_data

The renderer is:

- G:/Dx/tui/crates/codegen/xai-grok-pager/src/views/extensions_modal.rs

## Changes already attempted

The current working tree contains these related changes:

1. Added Action::RefreshExtensions.
2. Added refresh dispatch when switching between the original extension tabs.
3. Kept Providers and Connect out of the Grok Build registry refresh path.
4. Fixed the Skills request so it sends the active sessionId.
5. Fixed the shell Skills handler so it resolves the active session working directory instead of always using the shell process directory.
6. Added explicit empty-state rows:
   - No hooks configured
   - No plugins installed
   - No marketplace sources configured
   - No skills or workflows discovered
   - No MCP servers configured

These changes improve diagnostics and fix the known Skills working-directory defect, but they do not yet prove why the real registries are empty.

## Current evidence from this machine

The current user configuration is:

    [marketplace]
    default_skills_installs_purged = true

    [cli]
    installer = "internal"

The current C:/Users/Computer/.grok directory contains general runtime directories such as agent, bin, docs, logs, and sessions, but no visible configured:

- skills directory
- plugins directory
- hooks directory
- marketplace source configuration
- MCP configuration file

Recent unified logs contain marketplace entries such as:

    marketplace handle_list: sources loaded
    source_count: 0
    sources: []

Session initialization logs also showed zero MCP servers.

This means the current environment may genuinely have empty registries. However, the original report says these tabs worked previously, so the next AI must verify whether configuration was lost, the runtime is reading the wrong home/workspace, or shell registry initialization is incomplete.

## Most likely investigation areas

### 1. Verify runtime home and working directory

Confirm that all components use the same values for:

- GROK_HOME
- USERPROFILE
- current process directory
- active session cwd
- workspace/project root

The pager, shell, plugin discovery, skills discovery, marketplace loader, and MCP config loader must not silently use different roots.

### 2. Verify actual ACP requests and responses

Add temporary structured debug logging, without credentials, recording for every registry request:

- method name
- redacted session ID
- resolved working directory
- response success/error
- decoded item count
- response shape when decoding fails

Distinguish:

- request never sent
- request routed to the wrong handler
- shell returned an error
- shell returned a valid empty list
- pager failed to decode a non-empty response
- result delivered to the wrong agent/modal

### 3. Verify registry initialization timing

Check whether plugin, hook, MCP, marketplace, and skill registries are initialized before list requests run.

Pay special attention to:

- plugin_registry_handle.snapshot() returning None
- session handles not being resident yet
- hooks gated by workspace trust
- plugin discovery running after the first list request
- MCP configuration loaded from a different working directory

### 4. Verify workspace trust

Hooks and project plugins may be hidden until the workspace is trusted. Confirm the UI displays the trust/blocked state rather than silently showing an empty list.

### 5. Verify marketplace feature gating

Official marketplace auto-registration is feature-gated. The default configuration resolves it to false unless enabled by environment or remote settings.

Do not enable the marketplace globally without confirming product policy. Make sure:

- configured sources are loaded
- an empty source list is reported clearly
- Add Source works
- source scan errors are rendered

### 6. Verify Skills discovery

Skills discovery is implemented in:

- G:/Dx/tui/crates/codegen/xai-grok-agent/src/prompt/skills.rs

Verify:

- active session cwd is used
- .grok/skills, .agents/skills, .claude/skills, and .cursor/skills rules are correct
- plugin-provided skills are included when the plugin registry is available
- default_skills_installs_purged is not hiding user skills
- a timeout is not silently converted into an empty vector

### 7. Verify response decoding

Confirm actual payloads match:

    { "hooks": [] }
    { "plugins": [] }
    { "sources": [] }
    { "skills": [] }
    { "servers": [] }

The pager unwraps the outer result object. Ensure the shell does not return a second unexpected envelope or a raw shape that becomes an empty fallback.

## Known validation status

This command passed:

    $env:PROTOC = "G:\\Temp\\UserTemp\\protoc\\bin\\protoc.exe"
    $env:RUSTUP_TOOLCHAIN = "1.96.0"
    cargo check -p xai-grok-pager --lib --offline -q

git diff --check passed.

The shell-only check and focused pager test command exceeded the five-minute command limit while compiling the large integrated dependency graph. They did not report a compiler or test failure before timing out.

## Acceptance criteria

The fix is complete only when:

1. Opening the Extensions modal requests all original Grok Build registries.
2. Switching tabs does not clear loaded data or create uncontrolled duplicate requests.
3. Hooks, Plugins, Marketplace, Skills, and MCP Servers display real configured entries when they exist.
4. Project/worktree Skills are discovered from the active session cwd.
5. Registry errors are displayed as errors, not empty lists.
6. Empty registries display a clear actionable empty state.
7. Workspace trust restrictions are visible.
8. Providers and Connect remain separate and continue working.
9. ZeroClaw entries do not replace Grok Build entries in the five original tabs.
10. No user configuration, credentials, or local changes are deleted.
11. Pager and shell checks pass without new warnings or errors.
12. An end-to-end test proves a non-empty shell response reaches the correct modal tab and renders an item.

## Suggested next step

Run the TUI with a known temporary test workspace containing one fixture for each registry type, enable structured request/response count logging, open each tab, and compare:

    request sent -> shell handler entered -> source count -> response payload -> pager task result -> rendered item count

This will identify whether the remaining problem is missing user configuration or a runtime wiring/initialization defect.

