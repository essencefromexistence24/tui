# ZeroClaw — Technical Overview

ZeroClaw is a local-first, self-hostable AI agent runtime written in Rust (edition 2024, MSRV 1.96), with a separate TypeScript web dashboard and a Rust TUI. It can run a single agent over CLI, drive a fleet of agents over a REST/WebSocket gateway, and bridge into ~40 messaging platforms.

Line counts in this document are physical lines (`rg -c ''`, i.e. matching editor line numbers) counted on `.rs` files under each crate's `src/`, unless stated otherwise.

## At a glance

| Metric | Value |
|---|---|
| Workspace members | 26 (24 crates + 2 apps) |
| Total Rust | ~775k lines across ~1,055 `.rs` files |
| Total TypeScript | ~51k lines across ~126 files (`web/` dashboard) |
| Largest crate | `zeroclaw-runtime` — 213,650 lines / 274 files |
| 2nd largest | `zeroclaw-channels` — 147,376 lines / 89 files |
| HTTP/WS gateway | `zeroclaw-gateway` (default port 42617) |
| Memory | `zeroclaw-memory` (SQLite + keyword/hybrid search) |
| Channels | 37 canonical channel types (see `listing.rs` registry) |
| Model providers | 75 canonical provider slots across 18+ families |
| Runtime | Tokio (multi-thread), async/await throughout |
| Windows build | MSVC `+crt-static`, `/STACK:8388608` stack override |

## Workspace layout

```
.
├── src/                    # main binary (zeroclaw CLI): 161 files / 23,837 lines
├── crates/
│   ├── zeroclaw-runtime/        # agent loop, turn engine, tools, SOPs, cron — 274 f / 213,650 L
│   ├── zeroclaw-channels/       # messaging platform integrations — 89 f / 147,376 L
│   ├── zeroclaw-tools/          # MCP client + built-in tool implementations — 91 f / 69,355 L
│   ├── zeroclaw-config/         # TOML config schema, policy, secrets, autonomy — 34 f / 68,162 L
│   ├── zeroclaw-providers/      # model providers, routing, auth/OAuth — 37 f / 55,559 L
│   ├── zeroclaw-gateway/        # HTTP + WebSocket API server — 35 f / 34,781 L
│   ├── zeroclaw-memory/         # SQLite store, search, consolidation — 36 f / 26,303 L
│   ├── zeroclaw-hardware/       # hardware/peripheral access — 34 f / 10,404 L
│   ├── zeroclaw-api/            # shared API types (channels↔runtime contract) — 22 f / 8,667 L
│   ├── zeroclaw-log/            # structured logging / log writing — 13 f / 6,461 L
│   ├── zeroclaw-plugins/        # WASM plugin host + WIT interface — 13 f / 5,588 L
│   ├── zeroclaw-infra/          # session storage (SQLite), core infra — 9 f / 4,935 L
│   ├── zeroclaw-tool-call-parser/ # tool-call / function-call parser — 1 f / 4,281 L
│   ├── robot-kit/               # robotics support — 11 f / 3,627 L
│   ├── zeroclaw-macros/         # proc macros (model-provider slots, etc.) — 1 f / 2,884 L
│   ├── zeroclaw-eval/           # evaluation harness — 9 f / 1,390 L
│   ├── zeroclaw-sop-graph/      # SOP (standard operating procedure) graph model — 1 f / 479 L
│   ├── aardvark-sys/            # native bindings — 1 f / 455 L
│   ├── zeroclaw-commands/       # shell-command wrapper crate — 1 f / 350 L
│   ├── zeroclaw-spawn/          # subprocess spawn — 1 f / 167 L
│   └── channel-fixture/         # test channel fixture
├── apps/
│   ├── zerocode/                # TUI client — 39 f / 51,889 L
│   └── tauri/                   # desktop shell (1,397 L)
├── web/                         # React/TS dashboard — ~51k L
├── tests/                       # integration tests — 58 f / 12,479 L
├── xtask/                       # mdbook preprocessors, schema dump, misc — 40 f / 13,245 L
└── wit/                         # WIT files for plugin interface
```

## 1. Messaging channels

All channel code lives in `crates/zeroclaw-channels/src/`. Each channel is a compile-time feature (`channel-<name>`); the binary only pulls the platforms it was built with. Default build enables `channel-acp-server`, `channel-discord`, `channel-email`, `channel-filesystem`, `channel-telegram`, `channel-webhook`; `channels-full` enables everything listed below plus `channel-matrix`, `channel-nostr`, `channel-wechat`, `whatsapp-web`, `voice-wake`.

The heart of the subsystem is the ingress orchestrator: `crates/zeroclaw-channels/src/orchestrator/mod.rs` (31,448 lines) — it owns login/relink flows, message normalization, pacing, dedup, allowlists, and dispatch to the runtime.

The canonical inventory is `CHANNEL_COMPILE_SPECS` in `crates/zeroclaw-channels/src/listing.rs` — **37 channel types** (36 schema-named + ACP server). The full registry, numbered:

| # | Channel | Feature | Module | Lines |
|---|---|---|---|---|
| 1 | Telegram | `channel-telegram` (default) | `telegram.rs` | 8,371 |
| 2 | Discord | `channel-discord` (default) | `discord/mod.rs` + 13 submodules | 14,153 |
| 3 | Slack | `channel-slack` | `slack.rs` | 10,024 |
| 4 | Mattermost | `channel-mattermost` | `mattermost.rs` | 3,438 |
| 5 | iMessage | `channel-imessage` | `imessage.rs` | 1,364 |
| 6 | Matrix | `channel-matrix` | `matrix.rs` | 6,826 |
| 7 | Signal | `channel-signal` | `signal.rs` | 2,135 |
| 8 | WhatsApp (Cloud API) | `channel-whatsapp-cloud` | `whatsapp.rs` | 2,873 |
| 9 | WhatsApp Web (QR) | `whatsapp-web` | `whatsapp_web.rs` (+ storage 1,820) | 4,294 |
| 10 | Linq | `channel-linq` | `channels/linq.rs` | — |
| 11 | WATI | `channel-wati` | `channels/wati.rs` | — |
| 12 | NextCloud Talk | `channel-nextcloud` | `nextcloud_talk.rs` | 1,659 |
| 13 | Email (IMAP/SMTP) | `channel-email` (default) | `email_channel.rs` | 1,889 |
| 14 | Gmail Push | `channel-email` | `gmail_push.rs` | 1,259 |
| 15 | IRC | `channel-irc` | `irc.rs` | 1,209 |
| 16 | Twitch | `channel-twitch` | `irc.rs` (twitch module) | 202 |
| 17 | Lark/Feishu | `channel-lark` | `lark.rs` | 6,842 |
| 18 | DingTalk | `channel-dingtalk` | `dingtalk.rs` | 553 |
| 19 | WeCom | `channel-wecom` | `wecom.rs` | 237 |
| 20 | WeCom WebSocket | `channel-wecom-ws` | `wecom_ws.rs` | 3,867 |
| 21 | WeChat (iLink) | `channel-wechat` | `wechat.rs` | 3,273 |
| 22 | QQ Official | `channel-qq` | `qq.rs` | 2,837 |
| 23 | Nostr | `channel-nostr` | `nostr.rs` | 555 |
| 24 | ClawdTalk | `channel-clawdtalk` | `clawdtalk.rs` | 446 |
| 25 | Reddit | `channel-reddit` | `reddit.rs` | 571 |
| 26 | Bluesky | `channel-bluesky` | `bluesky.rs` | 647 |
| 27 | Git forges (GitHub/Gitea) | `channel-git` | `git/` | 6,432 |
| 28 | X/Twitter | `channel-twitter` | `twitter.rs` | 566 |
| 29 | Mochat | `channel-mochat` | `mochat.rs` | 463 |
| 30 | LINE | `channel-line` | `line.rs` | 2,762 |
| 31 | Voice Call | `channel-voice-call` | `voice_call.rs` + `transcription.rs` 2,422 + `tts.rs` 1,329 | 4,600 |
| 32 | VoiceWake | `voice-wake` | `voice_wake.rs` + `voice.rs` | 963 |
| 33 | MQTT | `channel-mqtt` | `mqtt.rs` | 306 |
| 34 | AMQP (RabbitMQ) | `channel-amqp` | `amqp.rs` | 1,193 |
| 35 | Filesystem | `channel-filesystem` (default) | `filesystem.rs` | 942 |
| 36 | Webhook | `channel-webhook` (default) | `webhook.rs` | 1,104 |
| 37 | ACP Server | `channel-acp-server` (default) | `orchestrator/acp_server.rs` (+ `acp_channel.rs` 1,586) | 5,546 |

Default build ships 6 of them (Telegram, Discord, Email, Webhook, ACP Server, Filesystem); `channels-full` enables all 37 except the opt-in heavy ones (`channel-matrix`, `channel-nostr`, `channel-wechat`, `whatsapp-web`, `voice-wake`).

### Chat / messaging platforms

| Channel | Module | Lines | Feature |
|---|---|---|---|
| Slack | `slack.rs` | 10,024 | `channel-slack` |
| Telegram | `telegram.rs` | 8,371 | `channel-telegram` (default) |
| Discord | `discord/mod.rs` + 13 submodules | 14,153 total | `channel-discord` (default) |
| Matrix | `matrix.rs` | 6,826 | `channel-matrix` |
| Lark | `lark.rs` | 6,842 | `channel-lark` |
| Mattermost | `mattermost.rs` | 3,438 | `channel-mattermost` |
| WhatsApp Cloud API | `whatsapp.rs` | 2,873 | `channel-whatsapp-cloud` |
| WhatsApp Web (QR) | `whatsapp_web.rs` (+ `whatsapp_storage.rs` 1,820) | 4,294 | `whatsapp-web` |
| WeChat (iLink) | `wechat.rs` | 3,273 | `channel-wechat` |
| WeCom (Work) | `wecom.rs` | 237 | `channel-wecom` |
| WeCom WebSocket | `wecom_ws.rs` | 3,867 | `channel-wecom-ws` |
| QQ | `qq.rs` | 2,837 | `channel-qq` |
| Line | `line.rs` | 2,762 | `channel-line` |
| Signal | `signal.rs` | 2,135 | `channel-signal` |
| DingTalk | `dingtalk.rs` | 553 | `channel-dingtalk` |
| Mochat | `mochat.rs` | 463 | `channel-mochat` |
| iMessage | `imessage.rs` | 1,364 | `channel-imessage` |
| IRC | `irc.rs` | 1,209 | `channel-irc` (Twitch adds `channel-twitch`, 202 L) |
| Nextcloud Talk | `nextcloud_talk.rs` | 1,659 | `channel-nextcloud` |
| Linq | (in `channels/linq.rs`) | — | `channel-linq` |
| Wati | (in `channels/wati.rs`) | — | `channel-wati` |
| Clawdtalk | `clawdtalk.rs` | 446 | `channel-clawdtalk` |

### Social / web platforms

| Channel | Module | Lines | Feature |
|---|---|---|---|
| Bluesky | `bluesky.rs` | 647 | `channel-bluesky` |
| Twitter/X | `twitter.rs` | 566 | `channel-twitter` |
| Reddit | `reddit.rs` | 571 | `channel-reddit` |
| Nostr | `nostr.rs` | 555 | `channel-nostr` |
| Notion | `notion.rs` | 757 | `channel-notion` |

### Email

| Channel | Module | Lines | Feature |
|---|---|---|---|
| Email (IMAP/SMTP) | `email_channel.rs` | 1,889 | `channel-email` (default) |
| Gmail push | `gmail_push.rs` | 1,259 | under `channel-email` |

### Voice

| Channel | Module | Lines | Feature |
|---|---|---|---|
| Transcription | `transcription.rs` | 2,422 | `channel-voice-call` |
| TTS | `tts.rs` | 1,329 | `channel-voice-call` |
| Voice call | `voice_call.rs` | 849 | `channel-voice-call` |
| Voice wake word | `voice_wake.rs` | 651 | `voice-wake` |
| Audio | `voice.rs` | 312 | `voice-wake` |

### Protocol / webhook / filesystem

| Channel | Module | Lines | Feature |
|---|---|---|---|
| HTTP webhook | `webhook.rs` | 1,104 | `channel-webhook` (default) |
| ACP server (API over HTTP) | `orchestrator/acp_server.rs` | 5,546 | `channel-acp-server` (default) |
| ACP channel client | `acp_channel.rs` | 1,586 | `channel-acp-server` |
| Filesystem folder watch | `filesystem.rs` | 942 | `channel-filesystem` (default) |
| MQTT | `mqtt.rs` | 306 | `channel-mqtt` |
| AMQP (RabbitMQ) | `amqp.rs` | 1,193 | `channel-amqp` |
| Git forges (GitHub/Gitea) | `git/` module | 6,432 total | `channel-git` |

### Channel infrastructure (not a platform)

| File | Lines | Purpose |
|---|---|---|
| `orchestrator/mod.rs` | 31,448 | ingress orchestration, login/relink, normalization, dedup, pacing |
| `orchestrator/acp_server.rs` | 5,546 | ACP (Agent Client Protocol) HTTP server |
| `paced_channel.rs` | 940 | rate-limiting wrapper used by channels |
| `listing.rs` | 441 | canonical channel registry |
| `allowlist.rs` | 123 | inbound sender allowlist |
| `util.rs` | 880 | shared helpers (media download, formatting) |
| `link_enricher.rs` | 462 | link metadata enrichment |
| `identity_persist.rs` | 507 | per-channel login identity persistence |
| `login_events.rs` / `login_probe.rs` / `login_relink.rs` | 342 / 183 / 206 | QR/bot-token login flow state machine |
| `lib.rs` | 106 | crate root / module wiring |

## 2. AI model providers & authentication

Provider code lives in `crates/zeroclaw-providers/src/`. The canonical provider list is generated by the `for_each_model_provider_slot!` macro in `crates/zeroclaw-config/src/providers.rs` (config structs in `crates/zeroclaw-config/src/schema.rs`) — 75 provider slots across the families below. The display registry `list_model_providers()` in `crates/zeroclaw-providers/src/lib.rs:2216` emits 75 `ModelProviderInfo` entries, with a debug assertion (`lib.rs:2034-2037`) enforcing 1:1 parity with the macro list. `docs/book/src/providers/catalog.md` ("All slots", line 82) is generated from the same macro via an mdbook preprocessor (`xtask/src/cmd/mdbook/peer_groups.rs:823`).

### Provider families (source file / lines)

| Family | File | Lines | Local / Hosted |
|---|---|---|---|
| OpenAI | `openai.rs` | 2,583 | Hosted |
| OpenAI Codex (CLI + API) | `openai_codex.rs` | 2,720 | Hosted |
| Azure OpenAI | `azure_openai.rs` | 1,070 | Hosted (your tenant) |
| Anthropic | `anthropic.rs` | 3,522 | Hosted |
| AWS Bedrock | `bedrock.rs` | 2,903 | Hosted (your account) |
| Gemini (incl. Gemini CLI) | `gemini.rs` + `gemini_cli.rs` | 2,667 + 404 | Hosted |
| OpenRouter | `openrouter.rs` (+ `openrouter_catalog.rs` 275) | 2,243 | Hosted (aggregator) |
| Ollama | `ollama.rs` | 1,992 | Local |
| Copilot (subprocess) | `copilot.rs` | 945 | Hosted |
| Telnyx AI | `telnyx.rs` | 415 | Hosted |
| Kilo CLI (subprocess) | `kilocli.rs` | 370 | Local/CLI |
| Zhipu GLM | `glm.rs` | 380 | Hosted |
| Any OpenAI-compatible endpoint | `compatible.rs` | 7,988 | Any |
| Generic / misc hosts | `models_dev.rs` | 334 | — |

### Core provider machinery

| File | Lines | Purpose |
|---|---|---|
| `compatible.rs` | 7,988 | OpenAI-compatible adapter (the default catch-all for arbitrary base URLs) |
| `reliable.rs` | 5,524 | reliability layer: retries, fallbacks, health checks |
| `factory.rs` | 2,507 | builds a provider instance from config |
| `router.rs` | 1,470 | request routing / provider selection |
| `multimodal.rs` | 2,488 | vision + audio input handling |
| `dispatch.rs` | 645 | streaming dispatch |
| `pricing.rs` | 782 | token cost tracking |
| `catalog.rs` | 470 | catalog of models per family |
| `model_pin.rs` | 234 | model pinning |
| `vision_override.rs` | 274 | per-request vision override |
| `stream_guard.rs` | 93 | stream safety guard |
| `lib.rs` | 4,782 | registry, `list_model_providers()`, auth plumbing |

### Authentication & OAuth

Auth lives in `crates/zeroclaw-providers/src/auth/`.

| File | Lines | Purpose |
|---|---|---|
| `mod.rs` | 2,065 | auth dispatch, per-provider auth resolution |
| `profiles.rs` | 916 | auth profiles: store/logout/login state, persisted credentials |
| `oauth_common.rs` | 361 | shared OAuth2/PKCE plumbing |
| `openai_oauth.rs` | 601 | OpenAI OAuth (device flow) |
| `gemini_oauth.rs` | 624 | Google OAuth (loopback flow) |
| `xai_oauth.rs` | 615 | xAI OAuth |
| `email_oauth2.rs` | 203 | IMAP/SMTP OAuth2 (XOAUTH2) |
| `anthropic_token.rs` | 86 | Anthropic token handling |

Auth modes per family:

| Mode | Used by | Mechanics |
|---|---|---|
| API key | OpenAI, Anthropic, Gemini, OpenRouter, Azure, Ollama (none), etc. | `Authorization: Bearer <key>` |
| OAuth | OpenAI, Gemini, xAI (plus email IMAP/SMTP via XOAUTH2) | `zeroclaw auth login` → device/PKCE flow → token cached in auth profile |
| CLI subprocess | Codex CLI, Copilot, Gemini CLI, Kilo | provider shells out to the vendor CLI which owns the login |
| PAT | Gitea / GitHub (git channel) | personal access token |
| SigV4 | AWS Bedrock | AWS credential chain |
| JWT (short-lived signed) | Zhipu GLM | `id.secret` → signed JWT (`sign_type: "SIGN"`) → `/v4/chat/completions` |
| None / local | Ollama, Telnyx local | no credential |

### OAuth capability matrix (per provider)

The `AuthProvider` enum (`crates/zeroclaw-providers/src/auth/mod.rs:597`) has 4 variants; real OAuth is implemented for 3 of them.

| Provider | OAuth login | Device code | PKCE browser | Paste-redirect resume | Token refresh (in-process) | Import token file | Auth file / lines |
|---|---|---|---|---|---|---|---|
| OpenAI Codex | Yes | Yes | Yes | Yes | Yes (retry+backoff, JWT account id) | Yes (`~/.codex/auth.json`) | `openai_oauth.rs` 601 |
| Gemini | Yes | Yes | Yes | Yes | Yes (id-token email) | No | `gemini_oauth.rs` 624 |
| xAI | Yes | Yes (RFC 8628, discovery endpoints) | Yes | Yes | Yes (JWT account id) | Yes (grok profile) | `xai_oauth.rs` 559 |
| Anthropic | No — bearer token only | — | — | — | — | — | `anthropic_token.rs` 86 |
| Email (channels) | No (operator provides creds) | — | — | — | Yes — generic OAuth2 refresh for IMAP/SMTP XOAUTH2 | — | `email_oauth2.rs` 178 |

Shared OAuth plumbing: `oauth_common.rs` (361 L) PKCE/refresh utilities; `profiles.rs` (916 L) encrypted on-disk auth profiles; `mod.rs` (2,065 L) dispatch + 3-attempt refresh with failure backoff and a permanent-vs-transient error classifier.

Caveat: `XaiFlow` overrides login/paste/refresh, so the default-impl error strings in `auth/mod.rs:1065` and `:1079` ("only OpenAI Codex and Gemini...") are stale — xAI also has both flows.

CLI surface: `zeroclaw auth login` / `auth logout` / `auth status` manage per-profile OAuth state (see `src/` auth commands).

## 3. Best features (with code paths)

### Top 10 at a glance

| # | Feature | What it does | Where | Scale |
|---|---|---|---|---|
| 1 | **Streaming agent loop** | Multi-turn agent runtime: model call → tool execution → results, with approval gates, context recovery, vision routing, max-iteration caps | `crates/zeroclaw-runtime/src/agent/loop_.rs` + `agent/turn/` (10-stage pipeline) | 16,962 L + 24 files |
| 2 | **37 messaging channels** | Ingress from ~30 chat/social/email/voice/platforms (Slack, Discord, Telegram, WhatsApp, Matrix, WeChat…) with QR pairing, dedup, pacing, login/relink | `crates/zeroclaw-channels/src/` + `orchestrator/mod.rs` | 147,376 L |
| 3 | **Security & policy engine** | Declarative policy (allow/deny per tool/provider), risk profiles (supervised↔auto-approve), human approval gate, OTP, remote e-stop, secret-leak detection | `crates/zeroclaw-config/src/policy.rs`, `runtime/approval/mod.rs`, `runtime/security/*` | policy 6,083 L, approval 1,043 L |
| 4 | **SOP graph engine** | Executes Standard Operating Procedures as graph workflows (scripted multi-step ops) | `crates/zeroclaw-runtime/src/sop/engine.rs` + `zeroclaw-sop-graph` | 14,955 L + 479 L |
| 5 | **MCP client** | Consumes external Model Context Protocol servers (stdio/SSE) as tools: resources, prompts, tools | `crates/zeroclaw-tools/src/mcp_client.rs` + 10 `mcp_*` modules | 3,115 L |
| 6 | **75 model providers + routing** | 18+ families (OpenAI, Anthropic, Gemini, Bedrock, Ollama, OpenRouter…) with retry/fallback, cost tracking, OAuth login for OpenAI/Gemini/xAI | `crates/zeroclaw-providers/src/` | 55,559 L |
| 7 | **Memory subsystem** | Persistent SQLite memory: search (keyword/hybrid), consolidation/compaction, forgetting, memory tools (store/recall/forget/purge/export) | `crates/zeroclaw-memory/src/` | 26,303 L |
| 8 | **Multi-agent & gateway API** | Spawn/sub-agent coordination + HTTP/WebSocket REST API for remote control of many agents | `runtime/tools/spawn_subagent.rs`, `zeroclaw-gateway` + `runtime/rpc/dispatch.rs` | gateway 34,781 L |
| 9 | **Tool-call parser** | Model-agnostic extraction of function calls from messy LLM output (XML-ish + JSON) | `crates/zeroclaw-tool-call-parser/src/lib.rs` | 4,281 L (1 file) |
| 10 | **Cron + WASM plugins** | Persistent scheduled jobs + capability extensions via WASM plugins (WIT interface) | `runtime/cron/scheduler.rs`, `zeroclaw-plugins/src/host.rs` | cron 2,730 L, plugins 5,588 L |

Honorable mentions: observability (OpenTelemetry + Prometheus, `runtime/observability/`), hardware access (`zeroclaw-hardware` 10,404 L), eval harness (`zeroclaw-eval`), TUI (`apps/zerocode` 51,889 L), web dashboard (`web/` ~51k TS).

### 3.1 The agent loop — 16,962 lines
`crates/zeroclaw-runtime/src/agent/loop_.rs` is the single most important file in the repo: the streaming agent run loop (input → model → tool calls → results → repeat, with iteration caps and context recovery). Agent state/negotiation in `agent/agent.rs` (9,876 L).

The turn pipeline is decomposed into stages under `crates/zeroclaw-runtime/src/agent/turn/` (24 files):

| Stage | File | Lines |
|---|---|---|
| Provider call | `provider_call.rs` | 487 |
| Parse response | `parse_response.rs` | 569 |
| Approval gate | `approval_gate.rs` | 220 |
| Execution | `execution.rs` | 285 |
| Results collection | `results_collect.rs` | 346 |
| Max iterations | `max_iter.rs` | 461 |
| Context recovery | `context_recovery.rs` | 470 |
| Stream consumption | `stream_consume.rs` | 374 |
| Vision routing | `vision_route.rs` | 510 |
| Output redaction | `redact.rs` | 167 |

### 3.2 Tool-call parsing — 4,281 lines
`crates/zeroclaw-tool-call-parser/src/lib.rs` is a hand-rolled, model-agnostic parser that extracts tool/function calls from LLM output (including malformed XML-ish tool blocks and JSON fragments) — a single 4,281-line file.

### 3.3 Security & policy
- `crates/zeroclaw-config/src/policy.rs` (6,083 L): declarative policy engine — allow/deny lists, per-tool and per-provider rules.
- `crates/zeroclaw-config/src/secrets.rs` (1,247 L): secret redaction/scanning on logs and messages.
- `crates/zeroclaw-runtime/src/approval/mod.rs` (1,043 L): human-in-the-loop approval gate (risk profiles: supervised / auto-approve).
- `crates/zeroclaw-runtime/src/security/estop.rs` (419 L): remote emergency-stop command.
- `crates/zeroclaw-runtime/src/security/otp.rs` (325 L): one-time-password confirmation flow.
- `crates/zeroclaw-runtime/src/security/leak_detector.rs` (1,397 L): secret-leak detection over channel traffic.
- `crates/zeroclaw-config/src/autonomy.rs` (221 L): autonomy-level knob.

### 3.4 SOP engine — 14,955 lines
`crates/zeroclaw-runtime/src/sop/engine.rs` executes Standard Operating Procedures defined as graphs (the graph model itself is `crates/zeroclaw-sop-graph/src/lib.rs`, 479 L). Enables scripted multi-step operational workflows.

### 3.5 MCP client — 3,115 lines
`crates/zeroclaw-tools/src/mcp_client.rs` is the Model Context Protocol client (stdio + SSE), plus `mcp_*` modules in the same crate (transport, resources, prompts, tools, deferred loading). Lets the agent consume external MCP servers as tools.

### 3.6 Runtime tool surface
- `crates/zeroclaw-runtime/src/tools/delegate.rs` (8,323 L): delegates tool calls to the tool crates.
- `crates/zeroclaw-runtime/src/tools/shell.rs` (1,733 L): shell execution tool.
- `crates/zeroclaw-runtime/src/tools/spawn_subagent.rs`: multi-agent spawn.
- 91 more tool modules in `crates/zeroclaw-tools/src/` (69,355 L): files, browser, web search, email, git, cloud ops, calendar/schedule, pushover, memory tools, claude_code/codex runners, MCP, etc.

### 3.7 Memory — 26,303 lines
`crates/zeroclaw-memory/src/sqlite.rs` (5,457 L) is the SQLite store; `retrieval.rs` (1,521 L) search; `consolidation.rs` (1,678 L) background compaction/summarization. Supports keyword search, optional hybrid (needs an embedding provider), and auto-forgetting.

### 3.8 Gateway API — 34,781 lines
`crates/zeroclaw-gateway/src/lib.rs` (8,685 L) exposes the HTTP + WebSocket API (default port 42617): multi-agent sessions, message streaming, admin endpoints. Session lifecycle in `crates/zeroclaw-infra/src/session_sqlite.rs` (1,335 L) and `crates/zeroclaw-runtime/src/rpc/session.rs` (1,014 L); RPC dispatch in `crates/zeroclaw-runtime/src/rpc/dispatch.rs` (10,311 L).

### 3.9 Cron / scheduling
`crates/zeroclaw-runtime/src/cron/scheduler.rs` (2,730 L): persistent cron jobs (CRUD tools + scheduler); tools in `crates/zeroclaw-runtime/src/tools/cron_*.rs`.

### 3.10 Observability
- `crates/zeroclaw-log/src/writer.rs` (1,942 L): structured log writing.
- `crates/zeroclaw-runtime/src/observability/otel.rs` (2,119 L): OpenTelemetry export.
- `crates/zeroclaw-runtime/src/observability/prometheus.rs` (917 L): Prometheus metrics.

### 3.11 WASM plugins — 5,588 lines
`crates/zeroclaw-plugins/src/host.rs` (1,218 L) runs WASM plugins against the WIT interface in `wit/` (host in `zeroclaw-plugins/src/`). Plugins extend the agent with custom capabilities.

### 3.12 Hardware access — 10,404 lines
`crates/zeroclaw-hardware/src/` exposes board info, memory maps, memory read, GPIO-style access (`hardware_board_info.rs`, `hardware_memory_map.rs`, `hardware_memory_read.rs` tools in `crates/zeroclaw-tools/src/`). Robotics via `robot-kit/`.

### 3.13 Multi-agent / spawning
`crates/zeroclaw-runtime/src/tools/spawn_subagent.rs` lets an agent spawn and coordinate sub-agents; gateway + RPC session layer manages multiple concurrent agent instances.

### 3.14 TUI client
`apps/zerocode/src/chat.rs` (10,955 L): the `zerocode` interactive terminal chat client (39 files, 51,889 L).

### 3.15 Web dashboard
`web/` (~51k lines TypeScript) — browser dashboard; i18n contract in `web/src/lib/i18n.ts`.

### 3.16 ACP bridge & remote agents
`src/bin/zeroclaw-acp-bridge.rs` bridges remote agents via ACP; `channel-acp-server` exposes the same protocol as a channel.

## Feature flags (top-level, root `Cargo.toml`)

| Flag | Enables |
|---|---|
| `default` | standard agent CLI + default-channels |
| `channels-full` | all `channel-*` features (everything except WhatsApp Web / voice-wake / matrix / nostr / wechat extras) |
| `whatsapp-web` | WhatsApp Web QR stack (vendored `whatsapp-rust`) |
| `voice-wake` | cpal-based wake word |
| `channel-matrix`, `channel-nostr`, `channel-wechat` | opt-in heavy SDKs |

`channel-*` features are declared in `crates/zeroclaw-channels/Cargo.toml`.

## Notes / caveats

- `openai_codex` is display-only in the registry; Codex credentials resolve through the `openai` slot (`requires_openai_auth = true`, `wire_api = "responses"`).
- `glm.rs` (380 L) appears orphaned in `zeroclaw-providers` (Zhipu slot) — verify before relying on it.
- Line counts use physical lines; a `Measure-Object -Line` count will be ~10% lower (it skips blank lines).
