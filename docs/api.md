# API compatibility

Implemented:

- `POST /v1/chat/completions`: text messages with system, developer, user, and assistant roles.
- `POST /v1/completions`: one string prompt.
- SSE streaming with `[DONE]`, OpenAI-shaped errors, and optional `stream_options.include_usage`.
- `GET /v1/models`: configured models permitted by the key when `serve --allowed-models` is set; otherwise enabled provider aliases and exact key scopes. This is not a live account/model catalog.
- `GET /healthz`: unauthenticated liveness only.

Conversation messages are serialized into one fresh harness prompt. This preserves supplied conversation text, but does not implement provider-native role precedence or saved chat sessions. Codex streams completed assistant message items; Antigravity, Pi/Bob, Gemini, and Claude stream their response deltas; OpenCode emits completed text parts. No adapter guarantees token-by-token latency. Reasoning and tool output are filtered out. Usage is the harness's aggregate run usage, which may cover multiple model calls; unavailable usage is omitted.

This is a text-completion compatibility layer, not the entire OpenAI API. Unsupported fields—including `tools`, `temperature`, `max_tokens`, `stop`, and `response_format`—return HTTP 400. There is no Responses API, embeddings, images, logprobs, token-array prompt, batch prompting, or `n > 1` support. A client that always sends unsupported sampling fields needs to omit them.

## Admission limits

Keys may expire and restrict request duration, lifetime accepted requests, rolling-minute request rate, and concurrent runs. Both completion routes and both response modes use the same admission operation. Policy checks happen before starting a CLI or sending SSE headers. Invalid requests and denied admissions consume no allowance; all accepted requests count, including failed or canceled runs. Counts survive restart.

Capacity, per-key concurrency, rate, and lifetime-quota exhaustion return HTTP 429 with distinct error codes. `Retry-After` is supplied when a retry interval is meaningful; a lifetime quota does not reset automatically. Invalid or expired keys return HTTP 401. Unreadable accounting fails closed. During an existing SSE stream, cancellation or expiry is a stream error after HTTP 200, not a successful `[DONE]` completion.

See [bounded API keys](policies.md) for flags, inspection, validation, and live probes. Local model validation checks configuration and authorization; it does not establish provider entitlement or guarantee that a provider alias resolves to an immutable model version.

Duplicate JSON field names, including case-insensitive duplicates, are rejected
at every nesting level before admission. Use one value for each field.
