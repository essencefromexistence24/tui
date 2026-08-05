# Codex CLI Setup for Contributors

> Instructions for contributors using Codex CLI with dx-tui

## ✅ Production-Ready Codebase

This project maintains production-quality standards:
- Zero compiler warnings
- Zero clippy warnings
- Fully formatted with rustfmt
- Comprehensive documentation
- Optimized for performance

## For New Contributors

If you're using Codex CLI to contribute to dx-tui, follow these steps:

### 1. Fork and Clone

```bash
git clone https://github.com/millercarla211-ctrl/dx-cli
cd dx-tui
```

### 2. Install Codex CLI

```bash
npm install -g @openai/codex
```

### 3. Authenticate

```bash
codex  # Opens browser for OAuth
```

### 4. Verify AGENTS.md

The project includes a comprehensive `AGENTS.md` file that Codex will automatically read. It contains:

- Project architecture and conventions
- Rust-specific guidelines
- Build and test commands
- Boundaries (what not to modify)

**No additional configuration needed!**

### 5. Start Contributing

```bash
# Launch Codex in the project directory
codex

# Or use specific commands
codex "Help me understand the animation system"
codex "Review my changes to src/chat.rs"
```

## Recommended Codex Settings

Create `~/.codex/config.toml` with these settings:

```toml
model = "gpt-5.3-codex"
approval_policy = "on-request"
sandbox_mode = "workspace-write"
web_search = "cached"

[features]
multi_agent = true
shell_tool = true
```

## Using Codex for Common Tasks

### Code Review

```bash
# Review your changes before committing
codex exec "Review my uncommitted changes for issues"
```

### Refactoring

```bash
# Interactive refactoring
codex "Refactor src/animations.rs to use a trait-based approach"
```

### Bug Fixing

```bash
# Debug with Codex
codex "Why is the animation not playing on startup?"
```

### Testing

```bash
# Generate tests
codex "Add unit tests for the chat input handler"
```

## Project-Specific Guidelines

### What Codex Knows About This Project

Codex reads `AGENTS.md` and understands:

- ✅ This is a Rust project (Edition 2024)
- ✅ Uses Tokio async runtime
- ✅ Ratatui for TUI
- ✅ 26-member workspace structure
- ✅ Animation system is a core feature
- ✅ File browser based on yazi architecture

### What to Tell Codex

When asking Codex for help, be specific:

**Good:**
- "Add error handling to the file browser actor in src/file_browser/actor/src/actor.rs"
- "Refactor the Matrix animation to use less CPU"
- "Fix the clippy warnings in src/chat_components.rs"

**Less Good:**
- "Make it better" (too vague)
- "Add tests" (which module?)
- "Fix the bug" (which bug?)

## Approval Modes

### Development (Recommended)

```toml
approval_policy = "on-request"
sandbox_mode = "workspace-write"
```

Codex will ask before:
- Deleting files
- Running potentially dangerous commands
- Installing new dependencies

### Code Review Only

```toml
approval_policy = "never"
sandbox_mode = "read-only"
```

Codex can only read files, not modify them.

### Full Auto (Use with Caution)

```bash
codex --full-auto "Fix all clippy warnings"
```

Codex will make changes without asking. Only use for safe, well-defined tasks.

## Troubleshooting

### Codex Doesn't Follow Project Conventions

**Check:**
1. Is `AGENTS.md` in the project root?
2. Is the file size < 64 KiB?
3. Are you running Codex from the project directory?

```bash
# Verify Codex can read AGENTS.md
codex exec "What does AGENTS.md say about this project?"
```

### Codex Suggests Wrong Patterns

**Tell Codex explicitly:**
```bash
codex "Use Tokio async, not sync code. Check AGENTS.md for conventions."
```

### Build Errors After Codex Changes

**Always verify (all checks pass cleanly):**
```bash
cargo check --workspace
cargo clippy --workspace --all-targets --all-features
cargo test --workspace
```

The codebase is production-ready with zero warnings.

## Best Practices

### 1. Read AGENTS.md First

Before asking Codex to make changes, read `AGENTS.md` to understand project conventions.

### 2. Use Specific File Mentions

```bash
codex --mention src/chat.rs "Add a new message type"
```

### 3. Review Changes Before Committing

```bash
git diff  # Review what Codex changed
cargo check --workspace  # Verify it builds
cargo clippy --workspace --all-targets --all-features  # Check for issues
```

### 4. Use Named Profiles

```bash
# For code review
codex --profile review

# For refactoring
codex --profile refactor

# For quick fixes
codex --profile quick-fix
```

### 5. Keep Context Focused

Don't mention too many files at once. Codex works best with focused context.

## Resources

- **Full Integration Guide**: [CODEX_INTEGRATION.md](../CODEX_INTEGRATION.md)
- **Project Conventions**: [AGENTS.md](../AGENTS.md)
- **Quick Start**: [QUICKSTART.md](../QUICKSTART.md)
- **Codex CLI Docs**: https://platform.openai.com/docs/codex-cli

## Questions?

Open an issue or ask in discussions:
- **Issues**: https://github.com/millercarla211-ctrl/dx-cli/issues
- **Discussions**: https://github.com/millercarla211-ctrl/dx-cli/discussions
