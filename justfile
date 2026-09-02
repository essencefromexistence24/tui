set shell := ["pwsh", "-c"]

protoc := env_var_or_default('PROTOC', env_var('TEMP') + '/protoc/bin/protoc.exe')
# DX runtime/config/cache root on this machine. An explicitly exported
# GROK_HOME still wins because the recipes preserve it when present.
grok_home := env_var_or_default('GROK_HOME', 'C:/Users/Computer/Dx/tui')
# Install targets for the release binary.
bin_dir_1 := 'G:/dx/bin'
bin_dir_2 := 'G:/bin'

default:
    @just --list

# A cold opt-level=3 build can run several 1-2 GB LLVM processes at once.
# Windows machines with a small page file then fail with misleading metadata
# and linker errors after the first allocation failure. Prime the exact release
# graph with bounded concurrency; the public build/run recipes still finish
# with the requested 1-job build once the heavy artifacts are available.
_release-prime:
    $env:PROTOC = "{{protoc}}"; $env:GROK_HOME = "{{grok_home}}"; $env:CARGO_INCREMENTAL = "1"; cargo build -p xai-grok-pager-bin --release -j 12

# Build the optimized release binary (thin LTO + strip), then copy it to
# G:\dx\bin and G:\bin and report the final size.
build: _release-prime
    $env:PROTOC = "{{protoc}}"; $env:GROK_HOME = "{{grok_home}}"; cargo build -p xai-grok-pager-bin --release -j 12; if ($LASTEXITCODE -eq 0) { $exe = ".\target\release\dx-tui.exe"; Copy-Item -LiteralPath $exe -Destination "{{bin_dir_1}}\dx-tui.exe" -Force; Copy-Item -LiteralPath $exe -Destination "{{bin_dir_2}}\dx-tui.exe" -Force; $f = Get-Item $exe; Write-Output ("dx-tui.exe: {0:N0} bytes ({1:N1} MB)" -f $f.Length, ($f.Length / 1MB)); Write-Output ("copied -> {{bin_dir_1}}\dx-tui.exe"); Write-Output ("copied -> {{bin_dir_2}}\dx-tui.exe") }

watch:
    $env:PROTOC = "{{protoc}}"; $env:GROK_HOME = "{{grok_home}}"; $env:CARGO_INCREMENTAL = "1"; cargo watch -x "build -p xai-grok-pager-bin --release -j 12" -s ".\target\release\dx-tui.exe"

check:
    $env:PROTOC = "{{protoc}}"; $env:GROK_HOME = "{{grok_home}}"; $env:CARGO_INCREMENTAL = "1"; cargo check -p xai-grok-pager-bin

run:
    $env:PROTOC = "{{protoc}}"; $env:GROK_HOME = "{{grok_home}}"; $env:CARGO_INCREMENTAL = "0"; cargo build -p xai-grok-pager-bin --release -j 12; if ($LASTEXITCODE -eq 0) { & ".\target\release\dx-tui.exe" }

fmt:
    $env:CARGO_INCREMENTAL = "1"; cargo fmt --all

clippy:
    $env:CARGO_INCREMENTAL = "1"; cargo clippy -p xai-grok-pager-bin

clean:
    $env:CARGO_INCREMENTAL = "1"; cargo clean -p xai-grok-pager-bin
