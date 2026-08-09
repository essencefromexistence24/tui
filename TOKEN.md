# DX TUI Token Optimization Plan

## Purpose

This document defines where token optimization will be integrated into the DX
Grok-Build TUI and gives planning estimates for the expected savings. The
percentages below are estimates, not benchmark results. We must measure the
actual DX request and response payloads before claiming production savings.

## Expected savings

The realistic planning target for a normal mixed coding session is **25%–45%
fewer billable input tokens**, with a reasonable long-term target of **35%–55%**
after routing, result compression, history compaction, and caching are all
working together.

For a workload dominated by large terminal output, repeated tool schemas, or
long conversations, savings may reach **50%–70%**. For short conversations with
few tools, savings may be only **5%–20%** because the fixed system prompt and
user request dominate the payload.

These ranges are not additive. For example, a 30% schema reduction followed by
a 40% history reduction does not automatically equal 70%; both operate on
overlapping portions of the request. The combined result must be measured at
the final model boundary.

| Optimization area | Planning range | Where it helps most |
| --- | ---: | --- |
| Tool routing: send only relevant tools | 10%–35% | First request and tool-heavy turns |
| Safe tool-schema minification | 5%–20% | Large tool catalogs |
| Prefix reuse/caching | 5%–25% of repeated input | Repeated turns with stable system/tool prefixes |
| Terminal, git, test, and shell-output compression | 20%–70% of tool-result tokens | Logs and command output |
| Duplicate-result removal | 5%–30% of repeated result tokens | Repeated searches, status checks, and retries |
| Output limits and selective truncation | 10%–60% of oversized results | Large logs and generated files |
| Inter-turn pruning and summarization | 20%–50% of old-history tokens | Long sessions |
| RLM for large files and repository context | 30%–80% of raw-file context | Large source files and broad repository queries |
| Response/semantic caching | 5%–25% fewer repeated calls | Deterministic read-only operations |

### Practical scenarios

| Session type | Before optimization | Expected after optimization |
| --- | ---: | ---: |
| Short question, few tools | 12,000 tokens | 9,600–11,400 |
| Typical coding turn | 12,000 tokens | 6,600–9,000 |
| Tool-heavy debugging turn | 20,000 tokens | 8,000–14,000 |
| Long repository session | 50,000 tokens | 20,000–35,000 |

The 12,000-token example is illustrative. It must be replaced with the real
DX request count from the model boundary and the tokenizer used by the target
model.

## Integration points

### 1. Tool definitions: highest-priority first-request optimization

Integrate a DX adapter around `tool-router` and safe `schema-minifier` before
tool definitions are passed to the model:

- [`xai-grok-tools/src/registry/types.rs`](G:/Dx/tui/crates/codegen/xai-grok-tools/src/registry/types.rs)
  - `ToolRegistryBuilder::tool_definitions()`
  - `tool_definitions_builtins_only()`
- [`xai-grok-agent/src/agent.rs`](G:/Dx/tui/crates/codegen/xai-grok-agent/src/agent.rs)
  - `Agent::tool_definitions()`
  - `Agent::tool_definitions_builtins_only()`
- [`xai-grok-shell/src/session/acp_session.rs`](G:/Dx/tui/crates/codegen/xai-grok-shell/src/session/acp_session.rs)
  - the projection into sampling tool definitions

The model may receive a reduced presentation of a tool schema, but dispatch
must continue using the canonical DX tool name, namespace, argument schema,
and validator. Tool routing must be adapted to DX’s real tool names; generic
names from the token workspace cannot be used blindly.

Start with safe minification. Do not remove required properties, enum values,
argument descriptions needed for correct selection, or protocol fields.

The first routing adapter is now attached to the shell sampler preparation
boundary, but remains opt-in while task-success parity is measured. Configure
it inside the agent definition so the setting is isolated to that session:

```yaml
tokenOptimization:
  enabled: true
  routeTools: true
  toolRouting:
    enabled: true
    maxTools: 12
```

Routing is skipped for local-model turns, empty queries, invalid limits,
ambiguous/weak matches, and any case where no confident match exists. The
router tokenizes tool names and descriptions, requires a minimum score and
margin, and preserves mandatory control/edit tools. The complete canonical
tool bridge still handles dispatch and validation.

### 2. Prompt assembly and stable prefixes

Integrate prefix reuse and safe whitespace normalization while the system
prompt, project instructions, and tool catalog are assembled:

- [`xai-grok-agent/src/builder.rs`](G:/Dx/tui/crates/codegen/xai-grok-agent/src/builder.rs)
- the prompt/context construction used by the shell session

The stable prefix should include only content that is genuinely unchanged.
User messages, current repository state, dynamic tool availability, and
security instructions must not be incorrectly cached.

### 3. Pre-call token budget and routing policy

Add a model-aware budget gate before sampling requests are sent:

- [`xai-grok-shell/src/session/compaction.rs`](G:/Dx/tui/crates/codegen/xai-grok-shell/src/session/compaction.rs)
  - pre-sampling compaction checks
  - context-overflow preflight
  - effective tool-definition token estimation
- [`xai-grok-shell/src/session/compaction_config.rs`](G:/Dx/tui/crates/codegen/xai-grok-shell/src/session/compaction_config.rs)

The gate should select, in order:

1. no compression when comfortably below budget;
2. tool routing and safe schema minification;
3. tool-result truncation/deduplication;
4. history pruning or summarization;
5. RLM for large file or repository context;
6. a hard failure only when the request still cannot fit safely.

The route workspace currently estimates with `cl100k_base`; this is useful for
planning but is not automatically the exact tokenizer for every target model.
Production accounting must record both the estimate and the provider/model
token count where available.

### 4. Tool-result compression: largest recurring saving

Compress results immediately after a tool finishes and before the result is
inserted into conversation history:

- [`xai-grok-tools/src/util/truncate.rs`](G:/Dx/tui/crates/codegen/xai-grok-tools/src/util/truncate.rs)
- [`xai-grok-shell/src/session/acp_conversion.rs`](G:/Dx/tui/crates/codegen/xai-grok-shell/src/session/acp_conversion.rs)

Use RTK-style compression for terminal, git, test, build, and search output;
deduplicate repeated lines; and apply bounded output truncation. Preserve:

- tool-call ID;
- exit status and error text;
- file paths and line numbers;
- patch hunks and diagnostics;
- the first and last useful portions of logs;
- a clear indication when content was omitted.

Use RLM for large files and repository context instead of placing the entire
raw file into the prompt. RLM should return focused evidence and locations,
not silently discard information required for the task.

### 5. Inter-turn context compaction

Integrate pruning, summarization, and compacted state in the existing session
compaction pipeline:

- [`xai-grok-shell/src/session/compaction.rs`](G:/Dx/tui/crates/codegen/xai-grok-shell/src/session/compaction.rs)
- [`xai-grok-shell/src/session/compaction_config.rs`](G:/Dx/tui/crates/codegen/xai-grok-shell/src/session/compaction_config.rs)

Always preserve the system prompt, the current user request, the latest task
state, recent tool-call/result pairs, active edits, and subagent coordination
messages. Older repetitive tool output and already-resolved exploration are
the first candidates for removal or summary.

### 6. Subagents and specialized tool sets

Subagents should receive a task-specific prompt and a small tool set rather
than the full DX catalog. Use the existing built-in-only/tool-definition paths
as a starting point. Apply the same canonical-schema rule: compact the model
presentation, never change the actual dispatch contract.

### 7. Caching and duplicate-call prevention

Add prefix, response, or semantic caching at the model-request/session
boundary, with strict safety rules:

- cache only deterministic read-only results by default;
- include repository revision, working directory, tool arguments, and relevant
  environment state in the cache key;
- never cache mutations, credentials, unstable command output, or stale error
  results without an explicit policy;
- use a governor to prevent repeated or runaway tool calls.

## Formats and components that need caution

- `serializer` is suitable for an internal compact context representation only
  when the receiving protocol permits its abbreviated envelope fields.
- `Caveman` and aggressive prose transforms are lossy and must not process
  canonical JSON schemas or tool-call messages.
- `Headroom`/TOON-style output must pass a round-trip test before it is used for
  structured tool data. Compact output is not automatically reversible.
- RLM is a context-selection mechanism, not a replacement for the canonical
  tool protocol.

## Measurement plan

Every request should record:

- raw input tokens;
- optimized input tokens;
- tool-definition tokens before and after routing/minification;
- tool-result tokens before and after compression;
- history tokens before and after compaction;
- output tokens;
- compression mode and whether information was truncated or summarized;
- latency and tool-selection/argument-validation failures.

The primary metric is:

```text
input_token_savings =
    (raw_input_tokens - optimized_input_tokens) / raw_input_tokens * 100
```

Run the same representative DX task corpus with optimization disabled and
enabled. Report median, p75, and p95 savings—not only the best case—and track
whether tool-call correctness, latency, and task success regress.

## Recommended target

For the first production milestone, target:

- **15%–30%** savings on the first request through tool routing and safe schema
  minification;
- **30%–60%** savings on large tool results;
- **25%–45%** average input-token savings across normal multi-turn coding
  sessions;
- no loss of required tool arguments, errors, paths, patches, or task state.

Only after these measurements pass should more aggressive serializer, TOON, or
prose-compression modes be enabled behind an explicit experimental setting.
