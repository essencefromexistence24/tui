$ErrorActionPreference = 'Stop'

$connects = 'C:\Users\Computer\AppData\Local\dx\connects'
$flowRoot = 'G:\Dx\flow-like\packages\catalog'
$n8nRoot = 'G:\Dx\hexxed\n8n'
$folders = @(Get-ChildItem -LiteralPath $connects -Directory | Where-Object { $_.Name -notlike '.rename-*' })
$n8nKnown = @{}
foreach ($package in @(
    @{ Name = 'n8n-nodes-base'; File = Join-Path $n8nRoot 'packages\nodes-base\dist\known\nodes.json'; Root = Join-Path $n8nRoot 'packages\nodes-base' },
    @{ Name = '@n8n/n8n-nodes-langchain'; File = Join-Path $n8nRoot 'packages\@n8n\nodes-langchain\dist\known\nodes.json'; Root = Join-Path $n8nRoot 'packages\@n8n\nodes-langchain' }
)) {
    $known = Get-Content -LiteralPath $package.File -Raw | ConvertFrom-Json
    foreach ($entry in $known.PSObject.Properties) {
        $n8nKnown["$($package.Name).$($entry.Value.className)"] = @{ Root = $package.Root; Source = [string]$entry.Value.sourcePath }
    }
}

$flowFiles = @{}
foreach ($file in Get-ChildItem -LiteralPath $flowRoot -Recurse -Filter '*.rs' -File) {
    if ($file.FullName -match '\\tests\\') { continue }
    $text = [IO.File]::ReadAllText($file.FullName)
    $relative = $file.FullName.Substring($flowRoot.Length).TrimStart('\\')
    $family = $relative.Split('\\')[0]
    foreach ($match in [regex]::Matches($text, 'impl NodeLogic for ([A-Za-z0-9_]+)')) {
        $flowFiles["flow-like.$family.$($match.Groups[1].Value.ToLowerInvariant())"] = $file.FullName
    }
}

$copied = 0
$missing = 0
foreach ($folder in $folders) {
    $metadataPath = Join-Path $folder.FullName 'node.json'
    if (-not (Test-Path -LiteralPath $metadataPath)) { continue }
    $metadata = Get-Content -LiteralPath $metadataPath -Raw | ConvertFrom-Json
    $implementation = Join-Path $folder.FullName 'implementation'
    if (Test-Path -LiteralPath $implementation) { continue }
    $sourceDirectory = $null
    $sourceFile = $null
    if ($n8nKnown.ContainsKey([string]$metadata.id)) {
        $info = $n8nKnown[[string]$metadata.id]
        $sourceFile = Join-Path $info.Root $info.Source
        if (Test-Path -LiteralPath $sourceFile) { $sourceDirectory = Split-Path $sourceFile -Parent }
    } elseif ($flowFiles.ContainsKey([string]$metadata.id)) {
        $sourceFile = $flowFiles[[string]$metadata.id]
    }
    if ($sourceDirectory) {
        New-Item -ItemType Directory -Force -Path $implementation | Out-Null
        Copy-Item -LiteralPath $sourceDirectory -Destination (Join-Path $implementation 'node') -Recurse -Force
        $copied++
    } elseif ($sourceFile) {
        New-Item -ItemType Directory -Force -Path $implementation | Out-Null
        Copy-Item -LiteralPath $sourceFile -Destination (Join-Path $implementation 'node.rs') -Force
        $copied++
    } else {
        $missing++
    }
}

"implementations_copied=$copied missing=$missing"
