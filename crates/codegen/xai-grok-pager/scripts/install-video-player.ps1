<#
.SYNOPSIS
Installs or updates the native DX video player for the current Windows user.

.DESCRIPTION
Copies a trusted, already-built DX release package into:
  %LOCALAPPDATA%\Programs\DX\Video

No files are downloaded and no administrator rights are required. The source
directory must contain the architecture-matching executable and its complete
DLL runtime. Every DLL beside the executable is copied into the isolated
per-user installation.

.EXAMPLE
.\install-video-player.ps1 -SourceDirectory G:\Dx\hexxed\terminal\dx-video-player
#>

[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$SourceDirectory
)

$ErrorActionPreference = 'Stop'

if (-not $IsWindows -and $PSVersionTable.PSEdition -ne 'Desktop') {
    throw 'The DX video-player installer currently supports Windows only.'
}
if (-not $env:LOCALAPPDATA) {
    throw 'LOCALAPPDATA is unavailable; cannot resolve the per-user installation directory.'
}

$source = [IO.Path]::GetFullPath($SourceDirectory)
if (-not (Test-Path -LiteralPath $source -PathType Container)) {
    throw "Source directory does not exist: $source"
}

$arch = switch ($env:PROCESSOR_ARCHITECTURE) {
    'AMD64' { 'x86_64' }
    'ARM64' { 'aarch64' }
    default { throw "Unsupported Windows architecture: $env:PROCESSOR_ARCHITECTURE" }
}
$sourceExeName = "dx-video-player-$arch-pc-windows-msvc.exe"
$sourceExe = Join-Path $source $sourceExeName
if (-not (Test-Path -LiteralPath $sourceExe -PathType Leaf)) {
    if ($arch -eq 'x86_64') {
        $sourceExe = Join-Path $source 'dx-video-player.exe'
    }
}

$runtimeManifest = Join-Path $PSScriptRoot "video-runtime-windows-$arch.txt"
if (-not (Test-Path -LiteralPath $runtimeManifest -PathType Leaf)) {
    throw "No verified DX video-player runtime manifest is available for $arch."
}
$runtimeFiles = @(Get-Content -LiteralPath $runtimeManifest | ForEach-Object { $_.Trim() } | Where-Object { $_ })
$required = @($sourceExe) + @($runtimeFiles | ForEach-Object { Join-Path $source $_ })
$missing = @($required | Where-Object { -not (Test-Path -LiteralPath $_ -PathType Leaf) })
if ($missing.Count -gt 0) {
    throw "DX video-player package is incomplete. Missing:`n  $($missing -join "`n  ")"
}

$programsRoot = [IO.Path]::GetFullPath((Join-Path $env:LOCALAPPDATA 'Programs'))
$destination = [IO.Path]::GetFullPath((Join-Path $programsRoot 'DX\Video'))
if (-not $destination.StartsWith($programsRoot, [StringComparison]::OrdinalIgnoreCase)) {
    throw "Refusing to install outside the per-user Programs directory: $destination"
}

$parent = Split-Path $destination -Parent
New-Item -ItemType Directory -Path $parent -Force | Out-Null
$staging = Join-Path $parent ("Video.staging-" + $PID)
$backup = Join-Path $parent ("Video.backup-" + $PID)
foreach ($path in @($staging, $backup)) {
    if (Test-Path -LiteralPath $path) {
        Remove-Item -LiteralPath $path -Recurse -Force
    }
}

try {
    New-Item -ItemType Directory -Path $staging | Out-Null
    Copy-Item -LiteralPath $sourceExe -Destination (Join-Path $staging 'dx-video-player.exe')
    foreach ($name in $runtimeFiles) {
        Copy-Item -LiteralPath (Join-Path $source $name) -Destination (Join-Path $staging $name)
    }
    $runtimeFiles | Sort-Object | Set-Content -LiteralPath (Join-Path $staging 'runtime-manifest.txt') -Encoding ASCII

    $stagedMissing = @(
        @('dx-video-player.exe', 'runtime-manifest.txt') + $runtimeFiles |
            Where-Object { -not (Test-Path -LiteralPath (Join-Path $staging $_) -PathType Leaf) }
    )
    if ($stagedMissing.Count -gt 0) {
        throw "Staged package verification failed: $($stagedMissing -join ', ')"
    }

    if (Test-Path -LiteralPath $destination) {
        Move-Item -LiteralPath $destination -Destination $backup
    }
    try {
        Move-Item -LiteralPath $staging -Destination $destination
    } catch {
        if (Test-Path -LiteralPath $backup) {
            Move-Item -LiteralPath $backup -Destination $destination
        }
        throw
    }
    if (Test-Path -LiteralPath $backup) {
        Remove-Item -LiteralPath $backup -Recurse -Force
    }
} finally {
    if (Test-Path -LiteralPath $staging) {
        Remove-Item -LiteralPath $staging -Recurse -Force
    }
}

$installedExe = Join-Path $destination 'dx-video-player.exe'
$hash = (Get-FileHash -LiteralPath $installedExe -Algorithm SHA256).Hash
Write-Host "DX video player installed to $destination" -ForegroundColor Green
Write-Host "Architecture: $arch" -ForegroundColor DarkGray
Write-Host "Runtime DLLs: $($runtimeFiles.Count)" -ForegroundColor DarkGray
Write-Host "SHA256: $hash" -ForegroundColor DarkGray
