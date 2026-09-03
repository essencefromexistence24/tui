# DX Connect runtime adapters

`dx-connect` never loads third-party node code into the TUI process. It sends
one request as one JSON line on stdin and expects one JSON response line on
stdout. Adapter diagnostics belong on stderr.

Protocol version: `dx-connect/1`

```json
{
  "protocol": "dx-connect/1",
  "request_id": "dx-123-456",
  "node_id": "n8n-nodes-base.httpRequest",
  "context": {
    "items": [{"json": {"url": "https://example.test"}}],
    "parameters": {"url": "={{$json.url}}"},
    "credentials": {},
    "metadata": {}
  }
}
```

The response is:

```json
{
  "protocol": "dx-connect/1",
  "request_id": "dx-123-456",
  "ok": true,
  "outputs": [[{"json": {"statusCode": 200}}], [ ]],
  "runtime_version": "2.34.0"
}
```

## n8n

The bundled `n8n/worker.cjs` loads compiled n8n packages and runs the node
through `WorkflowExecute`, so n8n expressions, node versions, and node-owned
execution logic remain in n8n. Set `DX_N8N_ROOT` to the n8n checkout or set
`DX_N8N_ADAPTER` to an explicit `node` executable wrapper. The worker uses the
`DX_N8N_ROOT` package tree and keeps credentials in the request body only.

Optional variables:

- `DX_NODE`: alternate Node.js executable.
- `DX_N8N_PACKAGES`: JSON object of package name to package-root paths.
- `DX_N8N_ADAPTER_TIMEOUT_MS`: bounded request timeout, default 30 seconds.
- `DX_N8N_ADAPTER_MAX_OUTPUT_BYTES`: output cap, default 16 MiB.

## Flow-Like

Flow-Like nodes require Flow-Like's own Rust/WASM executor because their
`NodeLogic::run` method receives Flow-Like's `ExecutionContext` and its WASM
host enforces declared permissions. The checked-out Flow-Like workspace now
contains the production adapter at
`apps/backend/local/connect-adapter`. Build it from that workspace, then point
DX at the resulting executable:

```powershell
Set-Location G:\Dx\flow-like
cargo build -p flow-like-connect-adapter --release
$env:DX_FLOW_LIKE_ADAPTER = (Resolve-Path '.\target\release\flow-like-connect-adapter.exe')
$env:DX_FLOW_LIKE_WASM_ROOT = 'C:\ProgramData\Dx\flow-like\wasm'
$env:DX_FLOW_LIKE_ADAPTER_CWD = $env:DX_FLOW_LIKE_WASM_ROOT
```

`DX_FLOW_LIKE_ADAPTER_ARGS` is a JSON string array when fixed, non-secret
arguments are needed. DX does not invoke a shell and never places credentials
in adapter arguments. Each Flow-Like request must include
`context.metadata.wasm_path`; relative paths resolve under
`DX_FLOW_LIKE_WASM_ROOT`, absolute paths must remain inside that root, and only
`.wasm` modules are accepted. The adapter derives Wasmtime capabilities from
the module's declared node permissions and returns the node output as one
JSON item, preserving the `dx-connect/1` request/response contract.
