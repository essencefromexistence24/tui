# DX Memory

## Purpose

`dx-memory` is DX’s independent, always-on Markdown memory layer. It runs separately from `dx-tui`, stores memory inside the operating system’s local DX data directory, and allows DX TUI agents to search and use the generated Markdown directly.

This document is planning-only. It does not define an implementation commitment yet.

## Core principles

- Rust-first and cross-platform.
- Independent daemon lifecycle; DX TUI does not need to stay open.
- Markdown files are the shared, inspectable memory interface.
- No mandatory cloud service.
- Privacy filtering happens before durable storage.
- Explicit capture, pause, forget, retention, and permission controls.
- Atomic writes and recoverable indexing.
- The daemon owns memory creation; DX TUI owns browsing, search, and agent use.

## Local storage locations

Use the platform-local DX data directory:

```text
Windows: %LOCALAPPDATA%\\Dx\\memory\\
macOS:   ~/Library/Application Support/Dx/memory/
Linux:   $XDG_DATA_HOME/dx/memory/
         fallback: ~/.local/share/dx/memory/
```

The exact path should be resolved by a shared Rust path module so the daemon and TUI always use the same location.

## Markdown structure

```text
memory/
├── daily/          # Date-based activity and summaries
├── projects/       # Project and workspace memory
├── sessions/       # Conversation and work-session memory
├── workflows/      # Repeated, normalized workflows
├── preferences/    # User-approved preferences
├── skills/         # Derived skill candidates and instructions
├── inbox/          # Pending memories awaiting review
├── index/          # Optional derived search metadata
└── .state/         # Internal checkpoints and schema state
```

Markdown is the portable materialized view. Files must include stable IDs, timestamps, source/provenance, scope, and sensitivity metadata in front matter.

Example:

```markdown
---
id: mem_01...
created_at: 2026-08-13T12:00:00Z
updated_at: 2026-08-13T12:00:00Z
scope: project
source: dx-memory
sensitivity: normal
---

# Memory title

Memory content.
```

## Components

### `dx-memory-core`

Shared Rust domain logic for memory records, scopes, metadata, privacy decisions, retention, and search results.

### `dx-memory-daemon`

Long-running background process. It captures approved sources, filters and normalizes events, deduplicates records, and writes Markdown files under the local DX data directory.

### `dx-memory-cli`

Standalone commands for status, search, indexing, structure inspection, pause/resume, export, repair, and deletion.

### `dx-memory-markdown`

Shared Markdown parser, front-matter model, safe writer, atomic replacement, and directory-structure utilities used by both daemon and TUI.

### `dx-memory-search`

Direct filesystem search over DX Markdown. Start with indexed metadata plus full-text search; add semantic search only after profiling proves it is necessary.

### `dx-tui` integration

DX TUI reads the same local Markdown tree directly through the shared Rust memory crates. It does not require the daemon to be running for browsing or searching existing memories.

## DX TUI integration

Add a Memory area to the Extensions menu with:

- daemon status
- storage location
- memory folder structure
- recent memories
- search and filtering
- pause/resume state
- privacy and retention status
- repair/reindex controls

Add `/memory` commands:

```text
/memory search <query>
/memory structure
/memory open <id-or-path>
/memory save <content>
/memory forget <id-or-path>
/memory status
/memory pause
/memory resume
```

Memory suggestions should show the source file and reason for relevance. The user must be able to inspect, reject, forget, or disable suggested context.

## Agent usage

Agents should receive only relevant, permission-approved memory excerpts through DX’s existing context/tool pipeline. The TUI should provide:

- scope-aware search
- bounded result count and character/token budgets
- source paths and memory IDs
- sensitivity filtering
- explicit provenance
- no silent injection of unrelated memories

An optional MCP adapter may expose memory to external clients, but MCP is not required for DX TUI’s native memory functionality.

## File safety

- Write to a temporary file in the same directory.
- Flush and close it before replacement.
- Replace with an atomic rename.
- Ignore temporary and partially written files during search.
- Validate front matter before indexing.
- Keep stable IDs when files are moved or regenerated.
- Prevent path traversal outside the DX memory root.
- Use per-scope permissions and explicit deletion confirmation.

## Change detection

The TUI should detect daemon-written changes using filesystem notifications where available, with a bounded fallback rescan. Search must remain correct if notifications are missed; notifications are only an optimization.

## Privacy requirements

- Default to disabled or explicitly approved capture sources.
- Never reconstruct ordinary typed text from global key events.
- Exclude passwords, secure fields, private browsing, and configured applications.
- Redact secrets before writing Markdown or logs.
- Support pause, emergency forget, retention, and scope controls.
- Show capture gaps and capability limitations honestly.
- Keep sensitive content out of suggestions unless explicitly allowed.

## Initial delivery phases

### Phase 1: Markdown memory foundation

- Shared models and path resolution.
- Safe Markdown read/write.
- Direct TUI search and `/memory` commands.
- Memory tree in Extensions.
- Manual save, open, and forget.

### Phase 2: Independent daemon

- Long-running lifecycle.
- Approved filesystem, workspace, and session sources.
- Privacy gate and deduplication.
- Atomic Markdown materialization.
- CLI status and repair commands.

### Phase 3: Rich search and suggestions

- Incremental metadata index.
- Scope-aware ranking.
- Relevant-memory suggestions.
- Agent context integration with budgets and provenance.

### Phase 4: Optional advanced features

- SQLite/FTS5 acceleration.
- Encrypted journal and recovery source.
- Semantic embeddings.
- Workflow discovery and skill candidates.
- Optional MCP adapter for external clients.

## Target architecture

```text
Approved OS/app sources
        ↓
dx-memory-daemon
        ↓
Privacy gate → normalize → deduplicate
        ↓
LocalData/Dx/memory/*.md
        ↓
dx-memory-markdown + dx-memory-search
        ↓
dx-tui Extensions / /memory / suggestions / agent context
```

The daemon remains independent, while DX TUI and DX agents use the same local Markdown memory directly through shared Rust code.

