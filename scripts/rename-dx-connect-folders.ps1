$ErrorActionPreference = 'Stop'
$root = 'C:\Users\Computer\AppData\Local\dx\connects'
$folders = @(Get-ChildItem -LiteralPath $root -Directory | Where-Object { $_.Name -notlike '.rename-*' })
$stage = Join-Path $root '.rename-staging'
New-Item -ItemType Directory -Force -Path $stage | Out-Null
$records = @()

foreach ($folder in $folders) {
    $metadata = Get-Content -LiteralPath (Join-Path $folder.FullName 'node.json') -Raw | ConvertFrom-Json
    $slug = ([string]$metadata.display_name).ToLowerInvariant() -replace '[^a-z0-9]+', '-'
    $slug = $slug.Trim('-')
    if ([string]::IsNullOrWhiteSpace($slug)) { $slug = 'node' }
    $temporary = Join-Path $stage ([guid]::NewGuid().ToString('N'))
    Move-Item -LiteralPath $folder.FullName -Destination $temporary
    $records += [pscustomobject]@{ Temporary = $temporary; Slug = $slug }
}

$used = @{}
foreach ($record in $records) {
    $name = $record.Slug
    while ($used.ContainsKey($name) -or (Test-Path -LiteralPath (Join-Path $root $name))) {
        $name += '-alt'
    }
    Move-Item -LiteralPath $record.Temporary -Destination (Join-Path $root $name)
    $used[$name] = $true
}

Remove-Item -LiteralPath $stage -Recurse -Force
"renamed_direct_node_folders=$($used.Count)"
