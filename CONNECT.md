# DX Connect Requirements

DX Connects are the DX-owned workflow nodes shown in the Extensions → Connects
tab. The current local catalog contains **2,208 implementation-backed nodes**:

| Runtime | Nodes | What runs them |
| --- | ---: | --- |
| Flow-Like Rust/WASM | 1,644 | DX Flow-Like adapter, Wasmtime, and the node `.wasm` modules |
| n8n JavaScript | 564 | Node.js LTS, the DX n8n worker, n8n core/workflow packages, and their installed dependencies |

## Required user tools

### All platforms

- DX TUI
- Network access for nodes that call remote services
- Provider credentials for the services a workflow uses
- A graphical desktop session for the DX Video Player

### n8n Connects

Node.js alone is not sufficient. Install Node.js LTS and make the n8n package
tree available with its production dependencies (`node_modules`). DX launches
the isolated worker at:

`crates/common/dx-connect/adapters/n8n/worker.cjs`

Configure either:

```powershell
$env:DX_N8N_ROOT = 'C:\ProgramData\Dx\n8n'
```

or provide an explicit Node wrapper through `DX_N8N_ADAPTER`. Optional
settings include `DX_NODE`, `DX_N8N_PACKAGES`,
`DX_N8N_ADAPTER_TIMEOUT_MS`, and `DX_N8N_ADAPTER_MAX_OUTPUT_BYTES`.

The worker keeps third-party JavaScript outside the TUI process and communicates
through the `dx-connect/1` JSON-line protocol.

### Flow-Like Connects

Node.js is not required. Install a Rust toolchain only when building the
adapter locally. A release installation should ship the adapter executable and
WASM modules directly:

```powershell
Set-Location G:\Dx\flow-like
cargo build -p flow-like-connect-adapter --release
$env:DX_FLOW_LIKE_ADAPTER = (Resolve-Path '.\target\release\flow-like-connect-adapter.exe')
$env:DX_FLOW_LIKE_WASM_ROOT = 'C:\ProgramData\Dx\flow-like\wasm'
$env:DX_FLOW_LIKE_ADAPTER_CWD = $env:DX_FLOW_LIKE_WASM_ROOT
```

The adapter validates the WASM module, permissions, request, credentials, and
response before returning results to DX.

## DX Video Player

The `/video` command supports local paths and the built-in DX showcase:

```text
/video showcase
```

The command suggestions also list:

- Frieren Beyond Journey's End
- One Piece
- Spiderman Into The SpiderVerse

The playlist order is Spiderman, One Piece, then Frieren.

Selecting any showcase item launches all three entries as an M3U8 playlist
with infinite playlist looping. DX first checks the per-user Downloads folder
(then the Videos folder) for these files:

```text
spiderman-into-the-spiderverse.mp4
one-piece.mp4
frieren-beyond-journeys-end.mp4
```

Existing readable files are used immediately. Missing files remain Catbox
stream URLs for the current launch, and DX downloads them in a background
thread using a `.mp4.part` temporary file before an atomic rename. Downloads
are attempted only when the system volume has more than 10 GiB free; when the
space check or the local download directory is unavailable, streaming remains
the safe fallback. The generated playlist is at:

`%LOCALAPPDATA%\dx\video\dx-showcase.m3u8`

Streaming uses the persistent player cache at:

`%LOCALAPPDATA%\dx\video\cache`

The player uses a 10-minute cache window, 30 seconds of read-ahead, a 32 MiB
stream buffer, and cache-pausing when the network falls behind. The disk
cache budget is computed from free space: it keeps a 2 GiB safety reserve and
uses between 512 MiB and 4 GiB for streaming cache. It does not prefetch the
complete playlist, so opening the showcase does not wait on all three remote
videos. Missing downloads are handled by one sequential background worker so
they do not saturate the connection while the current video is playing.

The installed DX Video Player must include its runtime manifest and native
libraries. `DX_VIDEO_PLAYER` can override the installed executable path.
