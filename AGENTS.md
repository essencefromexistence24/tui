# Build notes for this repo

## protoc on Windows

`bin/protoc` is a [dotslash](https://dotslash-cli.com) wrapper (JSON), not a
real binary. It only has entries for `macos-aarch64`, `linux-x86_64`, and
`linux-aarch64` — **no Windows entry**. Dotslash will not work on Windows.

To build on Windows, download a Windows protoc release and point to it:

```powershell
# One-time setup
Invoke-WebRequest -Uri "https://github.com/protocolbuffers/protobuf/releases/download/v29.3/protoc-29.3-win64.zip" -OutFile "$env:TEMP\protoc.zip"
Expand-Archive -Path "$env:TEMP\protoc.zip" -DestinationPath "$env:TEMP\protoc" -Force

# Before each build
$env:PROTOC = "$env:TEMP\protoc\bin\protoc.exe"
```

## Build only the main binary (not the whole workspace)

Full workspace builds are very slow. Target only the pager-bin package:

```powershell
cargo build -p xai-grok-pager-bin --release -j 8
```

## `/dev/stdout` / `/dev/null` on Windows

The `emit_rerun_if_changed` function in
`crates/build/xai-proto-build/src/lib.rs` originally used
`--dependency_out=/dev/stdout` and `--descriptor_set_out=/dev/null`, which do
not exist on Windows. The fix (already applied):

- Use a `NamedTempFile` for `--dependency_out` instead of `/dev/stdout`
- Use `NUL` (Windows) or `/dev/null` (non-Windows) for `--descriptor_set_out`
- Read dependency output from the temp file instead of stdout

## Binary name

The binary artifact is named `xai-grok-pager` (not `grok`). It lives at
`target/release/xai-grok-pager.exe` on Windows.
