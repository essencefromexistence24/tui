$ErrorActionPreference = 'Stop'
$root = 'C:\Users\Computer\AppData\Local\dx\connects'
$removed = 0
foreach ($folder in @(Get-ChildItem -LiteralPath $root -Directory)) {
    if (-not (Test-Path -LiteralPath (Join-Path $folder.FullName 'implementation'))) {
        Remove-Item -LiteralPath $folder.FullName -Recurse -Force
        $removed++
    }
}
"removed=$removed"
