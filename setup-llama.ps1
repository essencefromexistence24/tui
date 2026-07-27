$server = "$env:TEMP\llama\llama-server.exe"
$modelDir = "$env:USERPROFILE\.dx\flow\models\llm"
$model = "$modelDir\MiniCPM5-1B-Agentic-Tooluse-Nemotron-DPO.Q4_K_M.gguf"

# Download llama-server if missing
if (!(Test-Path $server)) {
    Write-Host "Downloading llama-server..."
    $tag = "b10152"
    $url = "https://github.com/ggml-org/llama.cpp/releases/download/$tag/llama-$tag-bin-win-cpu-x64.zip"
    $zip = "$env:TEMP\llama.zip"
    Invoke-WebRequest -Uri $url -OutFile $zip -UseBasicParsing
    Expand-Archive -Path $zip -DestinationPath "$env:TEMP\llama" -Force
    Write-Host "Downloaded."
} else {
    Write-Host "llama-server found."
}

# Copy model if missing
if (!(Test-Path $model)) {
    $src = "G:\Dx\flow\models\MiniCPM5-1B-Agentic-Tooluse-Nemotron-DPO.Q4_K_M.gguf"
    if (Test-Path $src) {
        New-Item -ItemType Directory -Path $modelDir -Force | Out-Null
        Copy-Item $src $model -Verbose
        Write-Host "Model copied."
    } else {
        Write-Error "Model not found at $src"
        exit 1
    }
} else {
    Write-Host "Model found."
}

Write-Host ""
Write-Host "Setup complete. Run: .\start-llama-server.ps1"
