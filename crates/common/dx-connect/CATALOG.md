# DX Connect catalog

This crate is the DX-owned boundary for external node ecosystems.

## Sources

- Flow-Like: `G:\Dx\flow-like`, MIT licensed. Its catalog is Rust/WASM and
  uses Flow-Like's own execution context. DX records source provenance and
  runs WASM nodes through the isolated `flow-like-connect-adapter` worker;
  it does not copy Flow-Like's whole workspace into the TUI.
- n8n: `G:\Dx\hexxed\n8n`, fair-code licensed under the repository's
  `LICENSE.md`. Its nodes are TypeScript classes and require n8n's
  `IExecuteFunctions`, credential service, expression evaluator, and task
  runtime. DX therefore does not vendored-copy or execute arbitrary n8n node
  classes in-process.

## Execution contract

`dx-connect` has three backends:

1. `Native`: deterministic Rust nodes that run in the TUI process.
2. `FlowLikeAdapter`: an external JSONL process boundary for the Flow-Like
   Rust/WASM executor. The adapter is built in Flow-Like's workspace so it
   uses the exact Flow-Like Wasmtime host and permission model.
3. `N8nAdapter`: the bundled Node.js worker boundary for n8n's
   `WorkflowExecute` engine and credential isolation.

An external node is never reported as executable just because its metadata is
present. Calling one without its adapter returns an explicit error. This keeps
the Connect UI and AI tool registry truthful.

At runtime the catalog reads n8n's generated `dist/known/nodes.json` files
for `n8n-nodes-base` and `@n8n/n8n-nodes-langchain` (442 + 122 entries in the
current checkout). It also scans Flow-Like catalog source for every
`impl NodeLogic for ...` definition (1,667 in the current checkout). If those
source trees are not installed, the smaller checked-in fallback inventory is
used. The native slice includes `dx.set`, `dx.if`, `dx.merge`, `dx.noop`, and
compatible execution aliases for n8n `Set`, `If`, `Merge`, and `NoOp`, plus
Flow-Like `control.branch`. Other external nodes remain adapter-backed and
return an actionable configuration error only when that runtime is not
installed or configured.
