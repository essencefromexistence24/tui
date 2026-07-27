$server = "$env:TEMP\llama\llama-server.exe"
$model = "$env:USERPROFILE\.dx\flow\models\llm\MiniCPM5-1B-Agentic-Tooluse-Nemotron-DPO.Q4_K_M.gguf"

if (!(Test-Path $server)) {
    Write-Error "llama-server not found at $server"
    Write-Host "Run setup-llama.ps1 first"
    exit 1
}

if (!(Test-Path $model)) {
    Write-Error "Model not found at $model"
    exit 1
}

Write-Host "Starting llama-server with MiniCPM5 1B Tool Use..."
Write-Host "Model: $model"
Write-Host "Port: 8080 | Threads: 6 | Context: 131072"
Write-Host ""

& $server -m "$model" -c 131072 -t 6 --load-mode mlock --reasoning off --port 8080
