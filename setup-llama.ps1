$server = "$env:TEMP\llama\llama-server.exe"
$modelDir = "$env:USERPROFILE\.dx\flow\models\llm"
$model = "$modelDir\qwen2.5-coder-1.5b-instruct-q4_k_m.gguf"

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
    New-Item -ItemType Directory -Path $modelDir -Force | Out-Null
    Invoke-WebRequest -Uri "https://huggingface.co/Qwen/Qwen2.5-Coder-1.5B-Instruct-GGUF/resolve/main/qwen2.5-coder-1.5b-instruct-q4_k_m.gguf?download=true" -OutFile $model -UseBasicParsing
    Write-Host "Model downloaded."
} else {
    Write-Host "Model found."
}

Write-Host ""
Write-Host "Setup complete. Run: .\start-llama-server.ps1"
