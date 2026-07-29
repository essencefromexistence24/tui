set shell := ["pwsh", "-c"]

protoc := env_var_or_default('PROTOC', env_var('TEMP') + '/protoc/bin/protoc.exe')

default:
    @just --list

build:
    $env:PROTOC = "{{protoc}}"; $env:CARGO_INCREMENTAL = "0"; cargo build -p xai-grok-pager-bin --release -j 12

watch:
    $env:PROTOC = "{{protoc}}"; $env:CARGO_INCREMENTAL = "0"; cargo watch -x "build -p xai-grok-pager-bin --release -j 12" -s ".\target\release\xai-grok-pager.exe"

check:
    $env:PROTOC = "{{protoc}}"; $env:CARGO_INCREMENTAL = "1"; cargo check -p xai-grok-pager-bin

run:
    $env:PROTOC = "{{protoc}}"; $env:CARGO_INCREMENTAL = "0"; cargo build -p xai-grok-pager-bin --release -j 12; if ($LASTEXITCODE -eq 0) { & ".\target\release\xai-grok-pager.exe" }

fmt:
    $env:CARGO_INCREMENTAL = "1"; cargo fmt --all

clippy:
    $env:CARGO_INCREMENTAL = "1"; cargo clippy -p xai-grok-pager-bin

clean:
    $env:CARGO_INCREMENTAL = "1"; cargo clean -p xai-grok-pager-bin
