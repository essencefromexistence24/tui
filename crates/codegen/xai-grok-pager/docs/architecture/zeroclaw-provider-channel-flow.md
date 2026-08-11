# DX provider catalog and ZeroClaw channel transport

## Provider milestone

The DX TUI now exposes **302 provider entries** in the unified provider menu.
The catalog is assembled at runtime from three sources:

| Source | Contribution |
| --- | ---: |
| DX community provider catalog | 136 source entries |
| ZeroClaw Agent provider registry and OAuth rows | merged into the DX catalog |
| DX Router API-key catalog | 125 previously missing provider rows |
| **Final deduplicated TUI menu** | **302 entries** |

The Router comparison found 255 API-key provider IDs. 129 were already
represented by the TUI/Agent catalog, and 125 new effective rows were added.
Two Router IDs normalize to one existing row, so the number of new menu rows is
125 rather than 126. Provider identity rows imported from Router deliberately
do not invent a direct endpoint. Saving a credential is rejected until a
verified provider-specific base URL exists; this prevents an unknown provider
from accidentally sending traffic to OpenAI's endpoint.

The merge and normalization logic lives in
`src/views/provider_connect/mod.rs`; the Router identity list lives in
`src/views/provider_connect/router_api_key_providers.rs`.

## Important separation: model providers versus message channels

ZeroClaw has two independent network boundaries:

```text
inbound platform event
        │
        ▼
Channel adapter ── ChannelMessage ──► orchestrator ──► agent/tool loop
                                                           │
                                                           ▼
                                                ModelProvider::chat/stream_chat
                                                           │
                                                           ▼
                                                final assistant text
                                                           │
                                                           ▼
                                             Channel ── SendMessage ──► platform
```

The model provider does not post directly to Telegram, Discord, Slack, or any
other messaging service. It only produces an LLM response. The channel
orchestrator owns delivery and calls the selected channel adapter's `send`,
`send_draft`, or `finalize_draft` method.

## Inbound path

1. Each enabled channel implements the shared `zeroclaw_api::channel::Channel`
   trait. Its `listen` method receives platform events and pushes normalized
   `ChannelMessage` values into a Tokio MPSC queue.

2. `ChannelMessage` is platform-neutral. It carries the message ID, sender,
   reply target, channel type, optional channel alias, thread ID, attachments,
   subject, conversation scope, and text content. Platform-specific parsing
   happens inside the adapter before this boundary.

3. The orchestrator's dispatch loop resolves the message to an agent-owned
   channel registry entry. It applies approval-gate handling, `/stop`
   cancellation, per-sender interruption grouping, and channel-specific
   debounce aggregation before starting the worker.

4. The worker runs receive hooks, self-message loop prevention, SOP ingress,
   conversation-history lookup, acknowledgement reactions, and media
   enrichment. Audio can be transcribed and media can be annotated before the
   model sees the resulting prompt.

5. Conversation history is keyed from the channel, alias, reply target,
   thread/sender scope, and sender identity. This prevents unrelated users or
   rooms from sharing context unless the configured scope intentionally makes
   them share it.

## Agent and model-provider path

The runtime resolves the configured agent profile to a provider family and
alias, for example `anthropic.default` or `openrouter.work`. It then:

- loads the provider credential from the typed provider configuration;
- resolves the configured model and provider URI;
- builds the provider through ZeroClaw's family factory/dispatch layer;
- assembles built-in tools, skills, peripherals, MCP tools, memory tools, and
  security-policy filters;
- selects the native-tool or XML/text dispatcher supported by the provider;
- sends a `ChatRequest` containing normalized `ChatMessage` history, model,
  tool specifications, sampling settings, and multimodal content;
- parses the provider response into assistant text and/or tool calls;
- executes approved tools, appends tool results to history, and repeats the
  model/tool loop until a final response, cancellation, limit, or error.

Provider adapters translate the common request into the upstream protocol:
OpenAI-compatible JSON, Anthropic Messages, Gemini, Bedrock, Ollama, CLI
subprocess protocols, or another provider-specific format. They also normalize
the upstream response back into ZeroClaw's common response types.

## Outbound channel path

After the agent loop completes, the orchestrator does not blindly forward raw
model output. It performs these stages:

1. Runs the `on_message_sending` hook. The hook may modify or cancel content,
   but attempts to rewrite the channel or recipient are not accepted as routing
   changes.

2. Applies channel-format-aware tool-output sanitization and credential-leak
   detection. Malformed tool-only output is replaced with a safe runtime
   message, and empty replies are converted to a non-empty fallback.

3. Adds fallback-provider information when the reliability router used a
   different provider family, while suppressing noisy same-family notices.

4. Resolves the delivery route. The normal route is the originating channel and
   `msg.reply_target`; the `send_via`/peer routing tools can select another
   configured channel and recipient for the current turn.

5. Delivers using one of three paths:

   - **Draft-capable channel:** create an initial draft, stream deltas by
     editing it, then finalize it. If finalization fails, send a new message.
   - **Redirected delivery:** cancel the originating draft and call `send` on
     the selected destination channel.
   - **Normal delivery:** build `SendMessage::reply_to(&msg, response)` and
     call the originating channel's `send` method.

6. `SendMessage` carries the final content, recipient, subject, thread ID,
   cancellation token, attachments, email reply ID, and voice modality flags.
   Each adapter maps these fields to its platform API. Unsupported fields are
   ignored or represented using that platform's fallback behavior.

7. A successful delivery fires the sent-message hook and records a structured
   outbound event. Delivery failures are logged with the channel and error
   context; they are not silently reported as successful.

## Channel-specific behavior

The shared trait gives every adapter the same contract, while adapters own
their platform details:

- Telegram, Discord, Slack, Matrix, WhatsApp, Signal, email, IRC, Lark,
  WeChat, LINE, Mattermost, Bluesky, Reddit, Twitch, and other adapters map
  `recipient` and `thread_ts` to their native destination/thread fields.
- Channels with draft support edit one platform message instead of creating a
  new message for every token delta.
- Channels with reactions can show receipt, thinking, success, failure, or
  no-reply state without putting internal tool data into the user-visible
  response.
- Voice-capable channels inspect `suppress_voice` and `force_voice`; TTS is a
  channel concern, not a model-provider concern.
- Attachments are carried through the common message type and ignored by
  adapters that cannot deliver them.

## Reliability and safety properties

- A bounded semaphore limits concurrent channel workers.
- Newer messages can cancel an older in-flight turn for the same interruption
  scope.
- Debouncing coalesces bursts from the same conversation before invoking the
  model.
- Self-message checks prevent adapter echo loops.
- Hooks can cancel inbound or outbound processing.
- Approval gates are handled before ordinary agent dispatch.
- Tool output is sanitized for the destination channel and checked for leaked
  credentials.
- History records the delivered response, not merely the model's raw response,
  so failed sends and hook cancellations are not falsely presented as sent.
- Structured attribution records agent, channel, sender, message ID, model
  provider, model, duration, and outcome without intentionally exposing
  secrets.

## Source map

- Shared message and channel contracts: `crates/common/agent/zeroclaw-api/src/channel.rs`
- Channel registry, ingress queue, routing, sanitization, and delivery:
  `crates/common/agent/zeroclaw-channels/src/orchestrator/mod.rs`
- Agent entry points and model/tool loop:
  `crates/common/agent/zeroclaw-runtime/src/agent/loop_.rs`
- Tool dispatch and tool-result conversion:
  `crates/common/agent/zeroclaw-runtime/src/agent/dispatcher.rs`
- Provider family construction and protocol dispatch:
  `crates/common/agent/zeroclaw-providers/src/factory.rs`

