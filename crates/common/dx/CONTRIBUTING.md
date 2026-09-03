# Contributing

## Setup

```powershell
git clone <repo>
cd dx-tui
cargo build -p dx-tui -j12
```

## Before Submitting

```powershell
cargo fmt --check
cargo check -p dx-tui -j12
cargo clippy -p dx-tui -j12 -- -D warnings
cargo test -p dx-tui -j12
```

## Guidelines

- All new code must compile without warnings (`-D warnings`)
- Add tests for new features (unit tests in-module, integration tests in `tests/`)
- Use Rust 2024 idioms
- Keep files under 2,000 lines; split large modules
- Document public API surface with doc comments
- No `#![allow(dead_code)]` — remove unused code instead
