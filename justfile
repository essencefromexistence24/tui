set shell := ["pwsh", "-c"]

protoc := env_var_or_default('PROTOC', env_var('TEMP') + '/protoc/bin/protoc.exe')

default:
    @just --list

build:
    $env:PROTOC = "{{protoc}}"; cargo build -p xai-grok-pager-bin --release -j 12

watch:
    $env:PROTOC = "{{protoc}}"; cargo watch -x "build -p xai-grok-pager-bin --release -j 12" -s ".\target\release\xai-grok-pager.exe"

check:
    $env:PROTOC = "{{protoc}}"; cargo check -p xai-grok-pager-bin -j 12

run:
    $env:PROTOC = "{{protoc}}"; cargo build -p xai-grok-pager-bin --release -j 12; if ($LASTEXITCODE -eq 0) { & ".\target\release\xai-grok-pager.exe" }

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all --check

lint:
    $env:PROTOC = "{{protoc}}"; cargo clippy -p xai-grok-pager-bin --release -j 12

lint-all:
    $env:PROTOC = "{{protoc}}"; cargo clippy --workspace --release -j 12 2>&1; exit $LASTEXITCODE

clippy:
    @just lint

clean:
    cargo clean -p xai-grok-pager-bin
