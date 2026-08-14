$ErrorActionPreference = 'Stop'

$destination = Join-Path $env:LOCALAPPDATA 'dx\plugins\codex'
$asciiDestination = Join-Path $env:LOCALAPPDATA 'dx\plugins\ascii'
New-Item -ItemType Directory -Force -Path $destination, $asciiDestination | Out-Null

$roots = @(
    'C:\Users\Computer\.codex\.tmp\plugins\plugins',
    'C:\Users\Computer\.codex\.tmp\bundled-marketplaces\openai-bundled\plugins',
    'C:\Users\Computer\.codex\plugins\cache'
)
$converter = Join-Path (go env GOPATH) 'bin\ascii-image-converter.exe'
if (-not (Test-Path -LiteralPath $converter)) { throw "Missing $converter" }

$seen = @{}
$copied = 0
$logos = 0
foreach ($manifestPath in Get-ChildItem -LiteralPath $roots -Recurse -Filter plugin.json -File -ErrorAction SilentlyContinue) {
    try { $manifest = Get-Content -LiteralPath $manifestPath.FullName -Raw | ConvertFrom-Json } catch { continue }
    $name = [string]$manifest.name
    if ([string]::IsNullOrWhiteSpace($name)) { continue }
    $safeName = $name -replace '[^A-Za-z0-9._-]', '_'
    $sourceRoot = $manifestPath.Directory.Parent.FullName
    $targetRoot = Join-Path $destination $safeName
    if (-not $seen.ContainsKey($safeName)) {
        if (Test-Path -LiteralPath $targetRoot) { Remove-Item -LiteralPath $targetRoot -Recurse -Force }
        Copy-Item -LiteralPath $sourceRoot -Destination $targetRoot -Recurse -Force
        $seen[$safeName] = $true
        $copied++
    }

    $sourceLogo = $null
    if ($manifest.interface -and $manifest.interface.logo) {
        $candidate = Join-Path $sourceRoot ([string]$manifest.interface.logo)
        if (Test-Path -LiteralPath $candidate) { $sourceLogo = $candidate }
    }
    if (-not $sourceLogo) {
        $sourceLogo = Get-ChildItem -LiteralPath $sourceRoot -Recurse -File -ErrorAction SilentlyContinue |
            Where-Object { $_.Name -match '(?i)(logo|icon|avatar|app-icon)' -and $_.Extension -match '(?i)^\.(svg|png|jpg|jpeg|webp)$' } |
            Select-Object -First 1 -ExpandProperty FullName
    }
    if (-not $sourceLogo) { continue }

    $temporaryPng = Join-Path $env:TEMP ("dx-plugin-logo-$safeName.png")
    try {
        if ([IO.Path]::GetExtension($sourceLogo).ToLowerInvariant() -eq '.svg') {
            $oldErrorAction = $ErrorActionPreference
            $ErrorActionPreference = 'Continue'
            & resvg-js --background white --fit-width 256 --fit-height 256 $sourceLogo $temporaryPng *> $null
            $ErrorActionPreference = $oldErrorAction
        } else {
            Copy-Item -LiteralPath $sourceLogo -Destination $temporaryPng -Force
        }
        if (-not (Test-Path -LiteralPath $temporaryPng)) { continue }
        $ascii = & $converter $temporaryPng --dimensions '48,16' --grayscale --map '@%#*+=-:. ' | Out-String
        $ascii = [regex]::Replace($ascii, "\x1b\[[0-9;]*m", '').TrimEnd()
        if (($ascii -replace '\s', '').Length -ge 4) {
            [IO.File]::WriteAllText((Join-Path $asciiDestination "$safeName.txt"), $ascii, [Text.UTF8Encoding]::new($false))
            $logos++
        }
    } finally {
        Remove-Item -LiteralPath $temporaryPng -Force -ErrorAction SilentlyContinue
    }
}

"copied=$copied unique plugins; logos=$logos; destination=$destination"
