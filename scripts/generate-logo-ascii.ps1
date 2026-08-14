param(
    [Parameter(Mandatory = $true)]
    [string]$SourceRoot,
    [Parameter(Mandatory = $true)]
    [string]$OutputRoot,
    [int]$Width = 48,
    [int]$Height = 12,
    [int]$Workers = 12
)

$ErrorActionPreference = "Stop"
$sourceRoot = (Resolve-Path -LiteralPath $SourceRoot).Path
New-Item -ItemType Directory -Force -Path $OutputRoot | Out-Null
$outputRoot = (Resolve-Path -LiteralPath $OutputRoot).Path
$converter = Join-Path (go env GOPATH) "bin\ascii-image-converter.exe"
$asciiMap = "@%#*+=-:. "

if (-not (Test-Path -LiteralPath $converter)) {
    throw "ascii-image-converter was not found at $converter. Install it with: go install github.com/TheZoraiz/ascii-image-converter@latest"
}

$inputs = @(Get-ChildItem -LiteralPath $sourceRoot -Filter *.png -File | Sort-Object Name)
if ($inputs.Count -eq 0) {
    throw "No PNG files found in $sourceRoot"
}

$results = $inputs | ForEach-Object -Parallel {
    $item = $_
    try {
        $asciiPath = Join-Path $using:outputRoot "$($item.BaseName).txt"
        $ascii = & $using:converter $item.FullName --dimensions "$($using:Width),$($using:Height)" --grayscale --map $using:asciiMap |
            Out-String
        if ($LASTEXITCODE -ne 0) {
            throw "ascii-image-converter failed"
        }
        $ascii = [regex]::Replace($ascii, "\x1b\[[0-9;]*m", "").TrimEnd()
        $visible = ($ascii -replace "\s", "").Length
        if ($visible -lt 12) {
            throw "ASCII output was empty or too small ($visible visible characters)"
        }
        [IO.File]::WriteAllText($asciiPath, $ascii, [Text.UTF8Encoding]::new($false))
        [pscustomobject]@{ ok = $true; name = $item.BaseName; visible = $visible }
    } catch {
        [pscustomobject]@{ ok = $false; name = $item.BaseName; error = $_.Exception.Message }
    }
} -ThrottleLimit $Workers

$failures = @($results | Where-Object { -not $_.ok })
if ($failures.Count -gt 0) {
    $failures | ConvertTo-Json -Depth 4 |
        Set-Content -LiteralPath (Join-Path $outputRoot "failures.json") -Encoding utf8
    throw "Failed to generate $($failures.Count) of $($inputs.Count) ASCII assets; see $outputRoot\failures.json"
}

Remove-Item -LiteralPath (Join-Path $outputRoot "failures.json") -Force -ErrorAction SilentlyContinue
$results | Sort-Object name | ConvertTo-Json -Depth 4 |
    Set-Content -LiteralPath (Join-Path $outputRoot "manifest.json") -Encoding utf8
Write-Host "Generated $($results.Count) ASCII logos in $outputRoot with $Workers workers"
