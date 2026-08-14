$ErrorActionPreference = 'Stop'

$destination = Join-Path $env:LOCALAPPDATA 'dx\connects\.runtime'
New-Item -ItemType Directory -Force -Path $destination | Out-Null

function Sync-ConnectSource([string]$source, [string]$name) {
    if (-not (Test-Path -LiteralPath $source -PathType Container)) {
        throw "Connect source does not exist: $source"
    }
    $target = Join-Path $destination $name
    New-Item -ItemType Directory -Force -Path $target | Out-Null
    # No /MIR: stale local files are never deleted by a refresh.
    robocopy $source $target /E /COPY:DAT /DCOPY:DAT /R:2 /W:2 /NFL /NDL /NP | Out-Null
    if ($LASTEXITCODE -gt 7) { throw "robocopy failed for $name (exit $LASTEXITCODE)" }
}

Sync-ConnectSource 'G:\Dx\flow-like' 'core'
Sync-ConnectSource 'G:\Dx\hexxed\n8n' 'integrations'
Write-Output "DX Connect sources synced to $destination"
