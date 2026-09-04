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

# Optional heavyweight messaging channels (off by default to keep dx-tui.exe
# lean). Each recipe rebuilds + reinstalls with that channel compiled in.
# Measured release sizes (Windows x86_64, thin LTO + strip):
#   base (default) ............ ~292 MB  (telegram, discord, slack, email, + full light set)
#   + matrix .................. ~322 MB  (+~30 MB, matrix-sdk + e2e-encryption + sqlite)
#   + wechat .................. ~297 MB  (+~5 MB, wechat crypto + qrcode)
#   + whatsapp-web ............ ~309 MB  (+~17 MB, wa-rs stack + prost)
#   + voice (wake-word) ....... ~297 MB  (+~5 MB, cpal audio)
#   + heavy (all four) ........ ~340 MB  (previous default; shared deps deduped)
# After installing, open Extensions → Connect: the new channel appears in the
# list. It still needs its own credentials (bot token / QR pairing) in
# channel.toml before the supervisor can connect it — nothing connects
# out of the box.
build-channels-matrix:
    $env:PROTOC = "{{protoc}}"; $env:GROK_HOME = "{{grok_home}}"; $env:CARGO_INCREMENTAL = "0"; cargo build -p xai-grok-pager-bin --release -j 6 --features channels-matrix; if ($LASTEXITCODE -eq 0) { Copy-Item -LiteralPath ".\target\release\dx-tui.exe" -Destination "{{bin_dir_1}}\dx-tui.exe" -Force; Copy-Item -LiteralPath ".\target\release\dx-tui.exe" -Destination "{{bin_dir_2}}\dx-tui.exe" -Force; Write-Output ("installed matrix build -> {{bin_dir_1}}\dx-tui.exe, {{bin_dir_2}}\dx-tui.exe") }

build-channels-wechat:
    $env:PROTOC = "{{protoc}}"; $env:GROK_HOME = "{{grok_home}}"; $env:CARGO_INCREMENTAL = "0"; cargo build -p xai-grok-pager-bin --release -j 6 --features channels-wechat; if ($LASTEXITCODE -eq 0) { Copy-Item -LiteralPath ".\target\release\dx-tui.exe" -Destination "{{bin_dir_1}}\dx-tui.exe" -Force; Copy-Item -LiteralPath ".\target\release\dx-tui.exe" -Destination "{{bin_dir_2}}\dx-tui.exe" -Force; Write-Output ("installed wechat build -> {{bin_dir_1}}\dx-tui.exe, {{bin_dir_2}}\dx-tui.exe") }

build-channels-whatsapp-web:
    $env:PROTOC = "{{protoc}}"; $env:GROK_HOME = "{{grok_home}}"; $env:CARGO_INCREMENTAL = "0"; cargo build -p xai-grok-pager-bin --release -j 6 --features channels-whatsapp-web; if ($LASTEXITCODE -eq 0) { Copy-Item -LiteralPath ".\target\release\dx-tui.exe" -Destination "{{bin_dir_1}}\dx-tui.exe" -Force; Copy-Item -LiteralPath ".\target\release\dx-tui.exe" -Destination "{{bin_dir_2}}\dx-tui.exe" -Force; Write-Output ("installed whatsapp-web build -> {{bin_dir_1}}\dx-tui.exe, {{bin_dir_2}}\dx-tui.exe") }

build-channels-voice:
    $env:PROTOC = "{{protoc}}"; $env:GROK_HOME = "{{grok_home}}"; $env:CARGO_INCREMENTAL = "0"; cargo build -p xai-grok-pager-bin --release -j 6 --features channels-voice; if ($LASTEXITCODE -eq 0) { Copy-Item -LiteralPath ".\target\release\dx-tui.exe" -Destination "{{bin_dir_1}}\dx-tui.exe" -Force; Copy-Item -LiteralPath ".\target\release\dx-tui.exe" -Destination "{{bin_dir_2}}\dx-tui.exe" -Force; Write-Output ("installed voice build -> {{bin_dir_1}}\dx-tui.exe, {{bin_dir_2}}\dx-tui.exe") }

build-channels-heavy:
    $env:PROTOC = "{{protoc}}"; $env:GROK_HOME = "{{grok_home}}"; $env:CARGO_INCREMENTAL = "0"; cargo build -p xai-grok-pager-bin --release -j 6 --features channels-heavy; if ($LASTEXITCODE -eq 0) { Copy-Item -LiteralPath ".\target\release\dx-tui.exe" -Destination "{{bin_dir_1}}\dx-tui.exe" -Force; Copy-Item -LiteralPath ".\target\release\dx-tui.exe" -Destination "{{bin_dir_2}}\dx-tui.exe" -Force; Write-Output ("installed heavy-channels build -> {{bin_dir_1}}\dx-tui.exe, {{bin_dir_2}}\dx-tui.exe") }

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

# Cross-compile the Linux x86_64 binary from Windows (no WSL needed).
# Proven 2026-09-03; produces target/x86_64-unknown-linux-gnu/size-opt/dx-tui.
# Why each knob exists (all learned the hard way):
# - cargo-zigbuild: provides the linux-gnu C toolchain (zig cc) for the
#   vendored C deps (libgit2, lua, sqlite, ring, ...). Install once:
#   `cargo install cargo-zigbuild --locked`.
# - scripts/linux-sysroot (Ubuntu ALSA .debs + extract.py): cpal/rodio need
#   libasound at link time; pkg-config finds it via PKG_CONFIG_* below.
# - RUST_MIN_STACK=16MB: rustc codegen recursion trips Windows' 1MB main
#   thread stack on the huge crates; 64MB starves thread spawning when RAM
#   is tight, 16MB is the sweet spot. Never use the default here.
# - -j1: parallel opt-z LLVM partitions + thin-LTO link exceed a 24GB box
#   with a small pagefile (see _release-prime note). Serial is slow (~1h)
#   but finishes; higher -j dies with 0xc0000409 / OOM kills.
# - --no-default-features --features sandbox-enforce: drops tikv-jemalloc,
#   whose autotools configure cannot cross-build from Windows. System
#   allocator instead; everything else identical. (Native CI keeps jemalloc.)
# - .2.34 target suffix: zig links glibc 2.34 (RHEL9/Ubuntu22.04+ compat).
#   The Jammy (1.2.6.1) libasound in the sysroot matches it; Noble's needs
#   2.38+ symbols and fails the link.
# - asound staging: pkg-config strips PKG_CONFIG_SYSROOT_DIR from -L, so the
#   linker never sees the sysroot. Recipe copies libasound.so* into the ring
#   build-out dir (already on the link search path) and re-runs to link.
# Close RAM hogs first; needs ~8GB free on G:.
# NOTE: PKG_CONFIG_* below must be absolute (justfile_directory()); relative
# paths make alsa-sys rebuild and fail to find alsa.pc.
build-linux:
    if (-not (Test-Path "scripts/linux-sysroot/sysroot/usr/lib/x86_64-linux-gnu/libasound.so")) { python scripts/linux-sysroot/extract.py }; $env:PROTOC = "{{protoc}}"; $env:CARGO_INCREMENTAL = "0"; $env:RUST_MIN_STACK = "16777216"; $env:PKG_CONFIG_ALLOW_CROSS = "1"; $env:PKG_CONFIG_PATH = "{{justfile_directory()}}/scripts/linux-sysroot/sysroot/usr/lib/x86_64-linux-gnu/pkgconfig"; $env:PKG_CONFIG_SYSROOT_DIR = "{{justfile_directory()}}/scripts/linux-sysroot/sysroot"; cargo zigbuild -p xai-grok-pager-bin --profile size-opt --target x86_64-unknown-linux-gnu.2.34 -j 1 --no-default-features --features sandbox-enforce; Copy-Item scripts/linux-sysroot/sysroot/usr/lib/x86_64-linux-gnu/libasound.so* target/x86_64-unknown-linux-gnu/size-opt/build/ring-*/out/ -Force; cargo zigbuild -p xai-grok-pager-bin --profile size-opt --target x86_64-unknown-linux-gnu.2.34 -j 1 --no-default-features --features sandbox-enforce; if ($LASTEXITCODE -eq 0) { $f = Get-Item target/x86_64-unknown-linux-gnu/size-opt/dx-tui; Write-Output ("dx-tui (linux): {0:N0} bytes ({1:N1} MB)" -f $f.Length, ($f.Length / 1MB)) }

# Size-optimized build (opt-level=z + strip). Same default channel set as
# `build`, much smaller: ~170 MB vs ~292 MB (measured 2026-09-03).
# Installs over the same G:\dx\bin + G:\bin targets.
build-tiny:
    $env:PROTOC = "{{protoc}}"; $env:GROK_HOME = "{{grok_home}}"; $env:CARGO_INCREMENTAL = "0"; cargo build -p xai-grok-pager-bin --profile size-opt -j 6; if ($LASTEXITCODE -eq 0) { $exe = ".\target\size-opt\dx-tui.exe"; Copy-Item -LiteralPath $exe -Destination "{{bin_dir_1}}\dx-tui.exe" -Force; Copy-Item -LiteralPath $exe -Destination "{{bin_dir_2}}\dx-tui.exe" -Force; $f = Get-Item $exe; Write-Output ("dx-tui.exe: {0:N0} bytes ({1:N1} MB)" -f $f.Length, ($f.Length / 1MB)); Write-Output ("copied -> {{bin_dir_1}}\dx-tui.exe"); Write-Output ("copied -> {{bin_dir_2}}\dx-tui.exe") }

# Sub-100MB distribution: UPX-pack a copy of the size-opt binary.
# Measured 2026-09-03 (upx 5.2.1 --best --lzma): 178,669,056 -> 53,992,448
# bytes (~51.5 MB), and the packed binary still runs (`--version` verified).
# Trade-offs: ~1s slower cold start (in-memory decompress), and some
# antivirus heuristics flag UPX-packed binaries — keep the unpacked
# size-opt binary as the default install and ship the packed one only where
# the 100 MB limit matters. Requires `upx` on PATH (or set UPX_BIN).
upx_bin := env_var_or_default('UPX_BIN', 'upx')
pack-tiny:
    if (-not (Test-Path ".\target\size-opt\dx-tui.exe")) { Write-Output "run 'just build-tiny' first"; exit 1 }; Copy-Item -LiteralPath ".\target\size-opt\dx-tui.exe" -Destination ".\target\size-opt\dx-tui-tiny.exe" -Force; & "{{upx_bin}}" --best --lzma ".\target\size-opt\dx-tui-tiny.exe"; if ($LASTEXITCODE -eq 0) { $f = Get-Item ".\target\size-opt\dx-tui-tiny.exe"; Write-Output ("dx-tui-tiny.exe: {0:N0} bytes ({1:N1} MB)" -f $f.Length, ($f.Length / 1MB)) }
