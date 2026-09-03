<div align="center">

<h1>
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://media.x.ai/v1/website/spacexai-symbol-white-transparent-0c31957f.png">
    <source media="(prefers-color-scheme: light)" srcset="https://media.x.ai/v1/website/spacexai-symbol-black-transparent-6435cf42.png">
    <img alt="SpaceXAI logo" src="https://media.x.ai/v1/website/spacexai-symbol-black-transparent-6435cf42.png" width="96">
  </picture>
  <br>
  Grok Build (<code>grok</code>)
</h1>

**Grok Build** is SpaceXAI's terminal-based AI coding agent. It runs as a
full-screen TUI that understands your codebase, edits files, executes shell
commands, searches the web, and manages long-running tasks — interactively,
headlessly for scripting/CI, or embedded in editors via the Agent Client
Protocol (ACP).

[Installing the released binary](#installing-the-released-binary) ·
[Building from source](#building-from-source) ·
[Documentation](#documentation) ·
[Repository layout](#repository-layout) ·
[Development](#development) ·
[Contributing](#contributing) ·
[License](#license)

![Grok Build TUI](https://media.x.ai/v1/website/universe-tui-screenshot-6f7a0837.png)

**Learn more about Grok Build at [x.ai/cli](https://x.ai/cli)**

This repository contains the Rust source for the `grok` CLI/TUI and its agent
runtime. It is synced periodically from the SpaceXAI monorepo.

A small `SOURCE_REV` file at the root records the full monorepo commit SHA
for the version of the code present in this tree.

</div>

---

## Installing the released binary

Prebuilt binaries are published for macOS, Linux, and Windows:

```sh
curl -fsSL https://x.ai/cli/install.sh | bash   # macOS / Linux / Git Bash
irm https://x.ai/cli/install.ps1 | iex          # Windows PowerShell
grok --version
```

See the [changelog](https://x.ai/build/changelog) for the latest fixes,
features, and improvements in each release.

## Building from source

Requirements:

- **Rust** — the toolchain is pinned by [`rust-toolchain.toml`](rust-toolchain.toml);
  `rustup` installs it automatically on first build.
- **[DotSlash](https://dotslash-cli.com)** — required so hermetic tools under
  [`bin/`](bin/) (notably [`bin/protoc`](bin/protoc)) can download and run.
  Install it and ensure `dotslash` is on your `PATH` **before** building:

  ```sh
  cargo install dotslash
  # or: prebuilt packages — https://dotslash-cli.com/docs/installation/
  /usr/bin/env dotslash --help   # sanity check
  ```

- **protoc** — proto codegen resolves [`bin/protoc`](bin/protoc) via DotSlash,
  or falls back to a `protoc` on `PATH` / `$PROTOC`.
- macOS and Linux are supported build hosts; Windows builds are best-effort
  and not currently tested from this tree.

```sh
cargo run -p xai-grok-pager-bin              # build + launch the TUI
cargo build -p xai-grok-pager-bin --release  # release binary: target/release/dx-tui
cargo check -p xai-grok-pager-bin            # fast validation
```

The shipped binary is `dx-tui`. On first launch it opens your browser to
authenticate — see the
[authentication guide](crates/codegen/xai-grok-pager/docs/user-guide/02-authentication.md).

### DX video playback

The full TUI supports `/video "<path>"`, which opens the media in the separate
native DX video-player window and leaves Dx's terminal responsive. Relative
paths are resolved from the active session workspace, so generated video output
can be played directly. Native-window embedding in a terminal is intentionally
not supported.

On Windows, install or update a trusted local player package without
administrator rights:

```powershell
.\crates\codegen\xai-grok-pager\scripts\install-video-player.ps1 -SourceDirectory G:\Dx\hexxed\terminal\dx-video-player
```

The installer copies the verified executable and its complete DLL runtime to
`%LOCALAPPDATA%\Programs\DX\Video`. For development, `DX_VIDEO_PLAYER` may
point directly to another player executable.

On macOS or Linux, install a native release package with:

```sh
sh ./crates/codegen/xai-grok-pager/scripts/install-video-player.sh /path/to/dx-video-player-package
```

The package directory must contain an executable named `dx-video-player` and
may include `runtime-manifest.txt` listing private shared libraries beside it.
Before installation, the staged executable is run with its staged library path
and rejected if any private runtime dependency is unavailable. For Linux x64,
use the graphical `build-x86_64-unknown-linux-gnu-gui` package; it includes
Wayland, X11, EGL/OpenGL, PulseAudio, and ALSA support. The older musl package
is headless and must not be used for `/video` window playback. Standard Linux
desktop libraries are still supplied by the distribution.
The per-user destination is `~/Library/Application Support/DX/Video` on macOS
and `${XDG_DATA_HOME:-~/.local/share}/dx/video` on Linux. Linux playback
requires an active Wayland or X11 graphical session; remote/headless sessions
receive an actionable error instead of a false playback notification.

## DX binary variants (size)

All variants below are the **same binary with the same default features** —
only compiler/optimizer settings (and optional channel sets) differ. Verified
2026-09-03: `doctor`, `models`, `--version`, and `--help` produce identical
output on every variant.

| Variant | Size | How | Notes |
|---|---|---|---|
| `release` (default) | ~292 MB | `just build` | Fastest startup; installed to `G:\dx\bin`, `G:\bin` |
| `size-opt` | ~170 MB | `just build-tiny` | `opt-level="z"` + strip; same features, marginally slower hot loops |
| `size-opt` + UPX | **~51.5 MB** | `just pack-tiny` (needs `upx` on `PATH` / `UPX_BIN`) | Self-extracting pack; ~1s slower cold start; some AV heuristics flag UPX-packed binaries, so it is a distribution option, not the default install |

`--profile size-opt` is defined in the root `Cargo.toml` (`opt-level="z"`,
thin LTO, `strip = true`). UPX is compression, not feature removal: the
packed binary decompresses to the exact same code in memory at launch.

### Optional heavyweight messaging channels

The default build ships the full light channel set (Telegram, Discord, Slack,
Email, WhatsApp Cloud, Matrix-ready basics, and the rest of `channels-full`).
Four heavy channels are **compiled out by default** to save ~48 MB and are
opt-in via cargo features (also wired as `just` recipes that rebuild and
reinstall):

| Channel set | Extra size | Install |
|---|---|---|
| `channels-matrix` | ~+30 MB (matrix-sdk, e2e-encryption, sqlite) | `just build-channels-matrix` |
| `channels-wechat` | ~+5 MB (wechat crypto, qrcode) | `just build-channels-wechat` |
| `channels-whatsapp-web` | ~+17 MB (wa-rs stack, prost) | `just build-channels-whatsapp-web` |
| `channels-voice` (wake-word) | ~+5 MB (cpal audio) | `just build-channels-voice` |
| `channels-heavy` (all four) | ~+48 MB total | `just build-channels-heavy` |

Or raw cargo: `cargo build -p xai-grok-pager-bin --release
--features channels-heavy` (individual flags work too). The Extensions →
Connect tab lists only compiled-in channels. **A listed channel is not a
connected channel**: each one needs its own credentials (bot token, QR
pairing, …) in `channel.toml` before the supervisor can connect it — nothing
connects out of the box.

## Connects catalog (2,400+ nodes)

`crates/common/connect` (`dx-connect`) is the source-aware node catalog and
deterministic executor behind Extensions → Connects and `/connect`:

- **4 native nodes** (`dx.set`, `dx.if`, `dx.merge`, `dx.noop`) run in-process.
- **564 n8n nodes** (442 `n8n-nodes-base` + 122 `@n8n/nodes-langchain`),
  generated from n8n's real `dist/known/nodes.json` inventories.
- **1,845 flow-like nodes** across 11 catalog families, extracted with the
  same `impl NodeLogic for X` scan the runtime discovery uses, from
  Rheosoph/flow-like @ `1545749d`.
- External nodes run through the `dx-connect/1` adapter subprocess boundary
  (Node.js worker for n8n, isolated executor for flow-like). Calling one
  without its runtime returns an explicit `AdapterUnavailable` error — a
  catalog entry is never reported as executable unless its backend is present.
- Live sources win at runtime: materialized `node.json` folders,
  `DX_N8N_ROOT` / `DX_FLOW_LIKE_ROOT` checkouts. The checked-in
  `src/static_nodes.rs` is the deterministic fallback; regenerate it with
  `crates/common/connect/scripts/regen_static_catalog.py` (see
  [`crates/common/connect/CATALOG.md`](crates/common/connect/CATALOG.md)).

## Documentation

Full online documentation is available at
[docs.x.ai/build/overview](https://docs.x.ai/build/overview).

The user guide ships with the pager crate:
[`crates/codegen/xai-grok-pager/docs/user-guide/`](crates/codegen/xai-grok-pager/docs/user-guide/)
— getting started, keyboard shortcuts, slash commands, configuration, theming,
MCP servers, skills, plugins, hooks, headless mode, sandboxing, and more.

## Repository layout

| Path | Contents |
|------|----------|
| `crates/codegen/xai-grok-pager-bin` | Composition-root package; builds the `dx-tui` binary |
| `crates/codegen/xai-grok-pager` | The TUI: scrollback, prompt, modals, rendering |
| `crates/codegen/xai-grok-shell` | Agent runtime + leader/stdio/headless entry points |
| `crates/codegen/xai-grok-tools` | Tool implementations (terminal, file edit, search, ...) |
| `crates/codegen/xai-grok-workspace` | Host filesystem, VCS, execution, checkpoints |
| `crates/codegen/...` | The rest of the CLI crate closure (config, MCP, markdown, sandbox, ...) |
| `crates/common/`, `crates/build/`, `prod/mc/` | Small shared leaf crates pulled in by the closure |
| `crates/common/agent/` | Vendored ZeroClaw agent stack (channels, providers, runtime, config) wired into the workspace |
| `crates/common/connect` | `dx-connect`: the 2,400+ node Connects catalog + executor (see above) |
| `crates/common/dx` | Standalone DX side-project workspace (separate `[workspace]`; not part of the pager build) |
| `third_party/` | Vendored upstream source (Mermaid diagram stack) — see below |

> [!IMPORTANT]
> The root `Cargo.toml` (workspace members, dependency versions, lints,
> profiles) is **generated** — treat it as read-only. Prefer editing per-crate
> `Cargo.toml` files.

## Development

```sh
cargo check -p <crate>        # always target specific crates; full-workspace builds are slow
cargo test -p xai-grok-config # per-crate tests
cargo clippy -p <crate>       # lint config: clippy.toml at the repo root
cargo fmt --all               # rustfmt.toml at the repo root
```

## Contributing

> [!NOTE]
> External contributions are not accepted. See [`CONTRIBUTING.md`](CONTRIBUTING.md).

## License

First-party code in this repository is licensed under the **Apache License,
Version 2.0** — see [`LICENSE`](LICENSE).

Third-party and vendored code remains under its original licenses. See:

- [`THIRD-PARTY-NOTICES`](THIRD-PARTY-NOTICES) — crates.io / git dependencies,
  bundled UI themes, and **in-tree source ports** (including openai/codex and
  sst/opencode tool implementations)
- [`crates/codegen/xai-grok-tools/THIRD_PARTY_NOTICES.md`](crates/codegen/xai-grok-tools/THIRD_PARTY_NOTICES.md)
  — crate-local notice for the codex and opencode ports (license texts +
  Apache §4(b) change notice)
- [`third_party/NOTICE`](third_party/NOTICE) — vendored Mermaid-stack index
