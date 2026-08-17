set shell := ["pwsh", "-c"]

protoc := env_var_or_default('PROTOC', env_var('TEMP') + '/protoc/bin/protoc.exe')
# DX runtime/config/cache root on this machine. An explicitly exported
# GROK_HOME still wins because the recipes preserve it when present.
grok_home := env_var_or_default('GROK_HOME', 'C:/Users/Computer/Dx/tui')

default:
    @just --list

# A cold opt-level=3 build can run several 1-2 GB LLVM processes at once.
# Windows machines with a small page file then fail with misleading metadata
# and linker errors after the first allocation failure. Prime the exact release
# graph with bounded concurrency; the public build/run recipes still finish
# with the requested 1-job build once the heavy artifacts are available.
_release-prime:
    $env:PROTOC = "{{protoc}}"; $env:GROK_HOME = "{{grok_home}}"; $env:CARGO_INCREMENTAL = "1"; cargo build -p xai-grok-pager-bin --profile release-dist -j 12

build: _release-prime
    $env:PROTOC = "{{protoc}}"; $env:GROK_HOME = "{{grok_home}}"; $env:CARGO_INCREMENTAL = "1"; cargo build -p xai-grok-pager-bin --profile release-dist -j 12

watch:
    $env:PROTOC = "{{protoc}}"; $env:GROK_HOME = "{{grok_home}}"; $env:CARGO_INCREMENTAL = "1"; cargo watch -x "build -p xai-grok-pager-bin --release -j 12" -s ".\target\release\dx-tui.exe"

check:
    $env:PROTOC = "{{protoc}}"; $env:GROK_HOME = "{{grok_home}}"; $env:CARGO_INCREMENTAL = "1"; cargo check -p xai-grok-pager-bin

run:
    $env:PROTOC = "{{protoc}}"; $env:GROK_HOME = "{{grok_home}}"; $env:CARGO_INCREMENTAL = "1"; cargo build -p xai-grok-pager-bin --release -j 12; if ($LASTEXITCODE -eq 0) { & ".\target\release\dx-tui.exe" }

fmt:
    $env:CARGO_INCREMENTAL = "1"; cargo fmt --all

clippy:
    $env:CARGO_INCREMENTAL = "1"; cargo clippy -p xai-grok-pager-binf

clean:
    $env:CARGO_INCREMENTAL = "1"; cargo clean -p xai-grok-pager-bin
