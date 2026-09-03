# BRUTAL TRUTHS & ROADMAP TO PRODUCTION READINESS

## What dx-tui Actually Is

dx-tui is a ~90K-line Rust TUI shell with an embedded file browser and AI chat. It is **not an autonomous AI coding agent** in the same league as opencode or Claude Code. It's closer to a "terminal IDE frontend" with a chat panel bolted on — a GUI for talking to LLMs while browsing files.

---

## The Ranking

| Rank | Tool | Why |
|------|------|------|
| 1 | **Claude Code** | 91% satisfaction, production-grade, hooks, worktrees, 200K context, multi-agent code review |
| 2 | **OpenCode** | 160K stars, 7.5M users, 75+ providers, MIT-licensed, LSP built-in, local models |
| 3 | **Aider** | Apache 2.0, git-native, 75+ providers, well-tested, mature |
| 4 | **Codex CLI** | MIT, sandboxed execution, OpenAI ecosystem |
| 5 | **dx-tui** | Interesting file browser, but not production-ready |

---

## The Flaws

### 1. 123 dead code items globally suppressed
`#![allow(dead_code)]` on entire crate. Hiding decay, not managing it. Plus 43 additional `#[allow(dead_code)]` scattered across files.

### 2. 0 integration tests for 90K LoC
File browser (52% of codebase, 47K lines) has **51 tests total**. 320 unit tests total, none testing any real workflow end-to-end.

### 3. Not an agent — it's a chat UI
No QueryEngine, no subagent system, no hooks, no permission model at the level of CC/OC. "Goal mode" is rudimentary.

### 4. Token counting = `text.len() / 4`
Without `llm` feature, token counting is a placeholder. Appears in `components.rs` AND `file_browser/plugin/src/utils/token.rs`.

### 5. NyanCat and Confetti are defined but disabled
In enum, no-op match arms, commented out of carousel. Incomplete work in a "released" product.

### 6. Monolithic files
- `state.rs` — 3,495 lines
- `dispatcher.rs` — 2,749 lines  
- `components.rs` — 2,927 lines

### 7. 6 global allow lints
`dead_code`, `unused_imports`, `unused_variables`, `unused_mut`, `unused_comparisons`, `non_upper_case_globals`. Compiler screaming, you've muted it.

### 8. File browser untested
908 files, 26 crates, 47K lines, 51 tests. ~0.05 tests per file.

### 9. .snap.new files unreviewed
`bottom_controls_wide.snap.new` and `bottom_controls_narrow.snap.new` exist — snapshot tests awaiting review.

### 10. SFTP/SSH commented out
`russh` dependency (447 transitive crates) commented out. SFTP is half-implemented.

### 11. No CI pipeline visible
No GitHub Actions visible, no coverage gates, no benchmark regression tracking.

---

## THE PLAN: How to Reach Production Readiness (100/100)

### Phase 0: Stop the Bleeding (Week 1) — Priority: CRITICAL

| # | Task | Files Affected | Effort |
|---|------|---------------|--------|
| 0.1 | Delete all `.snap.new` files (review then approve, or delete if stale) | 2 files | 15 min |
| 0.2 | Fix the 2 panic sites: `unimplemented!()` in `twox.rs` and `todo!()` in `url/buf.rs` | 2 files | 1 hr |
| 0.3 | Remove or complete NyanCat animation — either implement `render_nyancat_animation_in_area` or remove the variant entirely | `animations.rs`, `state.rs`, `chat_render.rs`, `sound.rs` | 2 hr |
| 0.4 | Remove or complete Confetti animation | `animations.rs`, `state.rs`, `chat_render.rs` | 1 hr |
| 0.5 | Audit and fix every `#[allow(dead_code)]` — either remove the code or add justification | ~43 locations across codebase | 4 hr |

### Phase 1: Testing Infrastructure (Week 2-3) — Priority: CRITICAL

| # | Task | Target | Effort |
|---|------|--------|--------|
| 1.1 | Add integration tests for the agent loop (`agent_loop.rs:run()`) — mock the Zen API, verify tool call extraction, verify loop termination | 10+ tests | 8 hr |
| 1.2 | Add integration tests for the dispatcher — key events → state transitions | 15+ tests | 8 hr |
| 1.3 | Add tests for every tool in `tools/mod.rs` — test parse, execute, format for each ToolKind | 20+ tests | 6 hr |
| 1.4 | Add tests for `goal_runner.rs` — start, pause, resume, budget enforcement | 10+ tests | 3 hr |
| 1.5 | Add tests for `chat_render.rs` — verify rendering of messages, accordions, tool calls, thinking blocks | 10+ tests | 6 hr |
| 1.6 | Add file browser tests — minimum 200 tests across the 26 crates (focus on `core`, `fs`, `vfs`, `config`, `widgets`) | 200+ tests | 40 hr |
| 1.7 | Set up CI with `cargo-tarpaulin` or `cargo-llvm-cov` for coverage — fail PRs below 60% coverage | 1 config file | 2 hr |
| 1.8 | Add snapshot review process — snapshots must be approved in PRs | 1 CI job | 1 hr |

### Phase 2: Code Quality & Hygiene (Week 3-4) — Priority: HIGH

| # | Task | Details | Effort |
|---|-------|---------|--------|
| 2.1 | Remove ALL 6 global allow lints from `lib.rs` | Fix every warning individually | 8 hr |
| 2.2 | Replace naive `len/4` token counting everywhere | Both in `components.rs` and `file_browser/plugin/src/utils/token.rs` | 4 hr |
| 2.3 | Set `unsafe_code = "deny"` in workspace lints (currently `"warn"`) | Audit and justify each `unsafe` block | 4 hr |
| 2.4 | Add `#![deny(warnings)]` to the main crate | Forces fixing all warnings before merge | 15 min |
| 2.5 | Make `rust-version = "1.94.0"` in CI with MSRV check | 1 CI job | 1 hr |
| 2.6 | Enable clippy `pedantic` and fix all violations | ~200+ warnings expected | 12 hr |

### Phase 3: Architecture Refactoring (Week 4-6) — Priority: HIGH

| # | Task | Details | Effort |
|---|-------|---------|--------|
| 3.1 | Split `state.rs` (3,495 lines) into modules | Extract: `session_state.rs`, `ui_state.rs`, `animation_state.rs`, `agent_state.rs`, `settings_state.rs` | 12 hr |
| 3.2 | Split `dispatcher.rs` (2,749 lines) into handlers | Extract: `keyboard_handler.rs`, `mouse_handler.rs`, `agent_event_handler.rs`, `system_event_handler.rs` | 10 hr |
| 3.3 | Split `components.rs` (2,927 lines) into focused modules | Extract: `message_list.rs`, `markdown_renderer.rs`, `tool_call_renderer.rs`, `thinking_accordion.rs` | 10 hr |
| 3.4 | Remove global Lua state (`fb_plugin::LUA`) — make plugin system properly isolated | `plugin_system.rs`, `fb_plugin` crate | 6 hr |
| 3.5 | Extract the agent loop from `agent_loop.rs` into cleaner abstractions | Separate: `agent_loop.rs` → `agent_loop/mod.rs`, `agent_loop/orchestrator.rs`, `agent_loop/tool_executor.rs` | 8 hr |

### Phase 4: Feature Completion (Week 5-7) — Priority: MEDIUM

| # | Task | Details | Effort |
|---|-------|---------|--------|
| 4.1 | Implement proper token counting (use `tiktoken-rs` by default, not just under `llm` feature) | `components.rs`, `file_browser/plugin/src/utils/token.rs` | 4 hr |
| 4.2 | Complete SFTP/SSH implementation or remove the dead code | Either uncomment `russh` and finish, or strip all SFTP code from `fb-vfs` and `fb-sftp` | 8-40 hr |
| 4.3 | Add proper permission model (currently looks basic) — implement tool-level permissions with allow/deny/ask per tool type | `permission_hub.rs`, `tools/mod.rs` | 8 hr |
| 4.4 | Add hooks system (pre/post tool hooks for linting, testing, validation) | New `hooks.rs` module | 12 hr |
| 4.5 | Implement proper subagent spawning with isolated context (depth limits, summary-only returns) | `subagent_registry.rs`, `agent_loop.rs` | 8 hr |

### Phase 5: Testing Deep Dive (Week 6-8) — Priority: MEDIUM

| # | Task | Details | Effort |
|---|-------|---------|--------|
| 5.1 | Add end-to-end tests for the full chat flow | Mock LLM → send message → receive response → render output | 8 hr |
| 5.2 | Add fuzz testing for input parsing (keyboard events, slash commands, tool call extraction) | Use `cargo-fuzz` or `proptest` | 6 hr |
| 5.3 | Add property-based tests for serialization roundtrips (session save/load, config, etc.) | Use `proptest` crate | 6 hr |
| 5.4 | Add performance regression tests for render loop, file browser startup, animation frame times | New `benches/` directory | 6 hr |
| 5.5 | Target: 70%+ line coverage across the entire codebase | Continuous improvement via CI gates | Ongoing |

### Phase 6: CI/CD & Tooling (Week 7-8) — Priority: MEDIUM

| # | Task | Details | Effort |
|---|-------|---------|--------|
| 6.1 | Set up GitHub Actions CI: `cargo check`, `cargo test`, `cargo clippy`, `cargo fmt --check`, coverage | `.github/workflows/ci.yml` | 3 hr |
| 6.2 | Set up nightly benchmark tracking with `cargo-criterion` | `.github/workflows/bench.yml` | 3 hr |
| 6.3 | Set up dependabot or Renovate for dependency updates | `.github/dependabot.yml` | 1 hr |
| 6.4 | Add `cargo-deny` for license and security audit of dependencies | 1 config file | 2 hr |
| 6.5 | Set up automated release workflow (build, strip, upload binaries) | `.github/workflows/release.yml` | 3 hr |

### Phase 7: Documentation & Developer Experience (Week 8-9) — Priority: MEDIUM

| # | Task | Details | Effort |
|---|-------|---------|--------|
| 7.1 | Add module-level doc comments to all 90+ modules in `lib.rs` | Every module must have a doc comment explaining purpose, usage, and entry points | 8 hr |
| 7.2 | Document the public API surface | `cargo doc` must build without warnings and provide meaningful docs | 4 hr |
| 7.3 | Write CONTRIBUTING.md with setup, build, test, and PR guidelines | 1 file | 2 hr |
| 7.4 | Write ADR (Architecture Decision Records) for key decisions | File browser embedding, Lua plugin system, Zen API client, MCP integration | 4 hr |
| 7.5 | Add code examples to key modules (tools, agent_loop, dispatcher) | 5-10 code examples in doc comments | 4 hr |

### Phase 8: Performance & Security (Week 9-10) — Priority: LOW

| # | Task | Details | Effort |
|---|-------|---------|--------|
| 8.1 | Profile the main render loop — identify and fix hot spots | Use `tracing` spans or `perf` / `DTrace` | 6 hr |
| 8.2 | Profile the file browser startup — currently inits 26 crates sequentially | Measure and optimize the init chain | 6 hr |
| 8.3 | Profile animation frame times — ensure 60fps on mid-range terminals | `animations.rs` render functions | 4 hr |
| 8.4 | Audit all `unsafe` blocks for correctness | ~50+ `unsafe` blocks expected across codebase | 8 hr |
| 8.5 | Audit tool permission model — ensure no shell injection, path traversal, or data leaks | `permission_hub.rs`, `tools/mod.rs` | 6 hr |
| 8.6 | Review dependency tree — remove unused deps, flag high-risk transitive deps | `cargo-udeps`, `cargo-deny` | 4 hr |
| 8.7 | Add `SECURITY.md` with vulnerability reporting process | 1 file (exists? verify) | 1 hr |

---

## Summary Dashboard

| Metric | Current | Target | Gap |
|--------|---------|--------|-----|
| Global `#[allow(...)]` lints | 6 | 0 | 100% reduction |
| `#[allow(dead_code)]` sites | 43 | 0 | 100% elimination |
| Integration tests | 0 | 50+ | Add from scratch |
| File browser tests | 51 | 250+ | 5x increase |
| Total test count | ~320 | 1,000+ | 3x increase |
| Line coverage | ~15% (estimate) | 70%+ | 55% gap |
| CI pipelines | 0 | 5+ | Add from scratch |
| Module documentation | ~30% | 100% | 70% gap |
| Architecture documentation | 0 | 5 ADRs | Add |
| `len/4` token counts | 2 sites | 0 | Fix |
| `unimplemented!`/`todo!` panics | 2 | 0 | Fix |
| NyanCat/Confetti dead entries | 2 | 0 | Fix or remove |
| Monolithic files >2K lines | 3 | 0 | Split |
| `.snap.new` unreviewed | 2 | 0 | Review |

---

## Estimated Total Effort: ~250-300 hours (6-8 weeks for 1 FTE)

## Estimated Timeline: 10 weeks if focused, 4-5 months if part-time

## One Thing dx-tui Does Better

The **file browser is genuinely impressive** — multi-tab, multi-format image preview, SFTP/SSH, VFS, Lua plugin system, 26 crates of async file engine. If dx-tui were positioned as a "terminal file manager with AI chat" rather than an "AI code editor", it would be more differentiated.

---

## Bottom Line

dx-tui suffers from **architecture ambition exceeding engineering discipline**. The file browser engine is genuinely impressive (the delta from a fresh `cargo new`), but the top-level application code is held together with global allow-lints, dead code, and missing tests. 

To compete with Claude Code or OpenCode, dx-tui needs:
1. A proper CI pipeline with coverage gates (Week 1-2)
2. 3x more tests with integration coverage (Week 2-4)  
3. The 3 monolithic files split into focused modules (Week 4-5)
4. Every `#[allow(dead_code)]` and global lint removed (Week 3)
5. Actual agent architecture, not just a chat loop (Week 5-7)

Without these, dx-tui remains a hobby project, not a production AI coding agent.

Here's the complete audit of hardcoded colors vs theme colors in the TUI rendering code:
Fixed — changed to use theme colors
File	What changed
src/components.rs	Diff separator 0x333344 → theme.muted_fg
src/components.rs	Tool gutter 0x666677 → theme.muted_fg
src/components.rs	Error output 0xff8888 → theme.danger()
src/components.rs	Shell command 0x7ddaa5 → theme.success()
src/components.rs	Generic output 0xcccccc → theme.fg
src/components.rs	Todo in-progress 0x448aff → theme.primary
src/components.rs	Footer line SOFT_MUTED_FG → self.theme.muted_fg
src/chat_render.rs	Tool suggestion colors (4 hardcoded RGBs) → theme.success(), theme.danger(), theme.primary(), theme.warning(), theme.accent
src/chat_render.rs	Sidebar done/active/cancelled (3 hardcoded RGBs) → theme.success(), theme.primary(), theme.muted_fg
src/chat_render.rs	Session saved muted SOFT_MUTED_FG → self.theme.muted_fg
src/chat_render.rs	Command style 0x4ade80 → theme.success()
src/chat_render.rs	Perf overlay bg Color::Black → self.theme.bg
src/msg_ui/render.rs	Thinking fg blend → theme.muted_fg
src/msg_ui/render.rs	Subagent color blend → theme.accent
src/msg_ui/render.rs	Diff hunk blend → theme.primary
src/msg_ui/render.rs	String literal blend → blend(success, fg, 0.3) (uses theme)
Remaining — acceptable as-is
File	Why
src/components.rs:900-930	SIDEBAR_BG, SCROLLBAR_*, DIFF_* constants — used in functions without theme access (would need API changes)
src/components.rs:1259	Code block gutter 0x555555 — no theme context
src/components.rs:1306	Diff context line 0xbbbbbb — render_diff_line has no theme parameter
src/components.rs:1762-1914	render_inline_markdown/render_web_line syntax highlight colors — code syntax highlighting, not UI chrome
src/chat_render.rs:730-820	Voice meter gradient colors — algorithmic/feature-specific
src/plan_wizard.rs	Entire file — no theme integration at all (18 hardcoded colors), needs dedicated pass
src/modes.rs:61-135	Mode/confidence colors — semantic brand colors
src/animations.rs	All animated colors — computed dynamically for visual effects
src/diff_view.rs	Diff colors duplicate DIFF_* constants — same as components.rs
