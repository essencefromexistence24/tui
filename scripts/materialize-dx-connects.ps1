$ErrorActionPreference = 'Stop'

$connects = Join-Path $env:LOCALAPPDATA 'dx\connects'
$catalogPath = Join-Path $connects 'catalog.json'
if (-not (Test-Path -LiteralPath $catalogPath -PathType Leaf)) {
    throw "DX Connect catalog is missing: $catalogPath"
}

$catalog = Get-Content -LiteralPath $catalogPath -Raw | ConvertFrom-Json
$nodes = @($catalog.nodes)

# This exact directory is the DX-created Connects cache. Do not broaden this
# target: source workspaces remain untouched.
foreach ($child in @(Get-ChildItem -LiteralPath $connects -Force)) {
    Remove-Item -LiteralPath $child.FullName -Recurse -Force
}

$index = 0
foreach ($node in $nodes) {
    $index++
    $slug = ([string]$node.display_name).ToLowerInvariant() -replace '[^a-z0-9]+', '-'
    $slug = $slug.Trim('-')
    if ([string]::IsNullOrWhiteSpace($slug)) { $slug = 'node' }
    $folder = Join-Path $connects ("{0:D4}-{1}" -f $index, $slug)
    New-Item -ItemType Directory -Force -Path $folder | Out-Null
    $node | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath (Join-Path $folder 'node.json') -Encoding utf8
}

"materialized=$index direct DX Connect node folders at $connects"
