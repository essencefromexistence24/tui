$server = "$env:TEMP\llama\llama-server.exe"
$model = "$env:USERPROFILE\.dx\flow\models\llm\qwen2.5-coder-1.5b-instruct-q4_k_m.gguf"

if (!(Test-Path $server)) {
    Write-Error "llama-server not found at $server"
    Write-Host "Run setup-llama.ps1 first"
    exit 1
}

if (!(Test-Path $model)) {
    Write-Error "Model not found at $model"
    exit 1
}

Write-Host "Starting llama-server with Qwen2.5 Coder 1.5B..."
Write-Host "Model: $model"
Write-Host "Port: 8080 | Threads: 6 | Context: 32768"
Write-Host ""

& $server -m "$model" -c 8192 -t 6 --load-mode mlock --reasoning off --repeat-penalty 1.1 --repeat-last-n 128 --port 8080
