# DX Development Notes

## Build Performance

Use `-j12` for all cargo commands. Release builds with LTO take 10-20 minutes.

## Common Issues

- **Editor not rendering**: The CaptureBackend requires a valid terminal size. If the TUI starts before the terminal is ready, the editor may fail to initialize. The adapter defers initialization on error.
- **block_in_place panics**: If you see "can call blocking only when running on the multi-threaded runtime", ensure the editor's tokio runtime is entered with `rt.enter()` before calling editor methods outside the TUI runtime.
- **Two runtimes**: The editor and TUI each have their own tokio runtime. The adapter manages this with `block_in_place` + `rt.enter()` patterns.

## Testing

The editor crate has extensive tests (215K lines). The TUI shell needs more. When adding tests, prefer inline `#[cfg(test)]` modules.
