# API compatibility

Implemented:

- `POST /v1/chat/completions`: text messages with system, developer, user, and assistant roles.
- `POST /v1/completions`: one string prompt.
- SSE streaming with `[DONE]`, OpenAI-shaped errors, and optional `stream_options.include_usage`.
- `GET /v1/models`: enabled provider aliases and exact models permitted by the key. This is not a live account/model catalog.
- `GET /healthz`: unauthenticated liveness only.

Conversation messages are serialized into one fresh harness prompt. This preserves supplied conversation text, but does not implement provider-native role precedence or saved chat sessions. Codex streams completed assistant message items; Antigravity streams its response deltas. Neither guarantees token-by-token latency. Reasoning and tool output are filtered out. Usage is the harness's aggregate run usage, which may cover multiple model calls; unavailable usage is omitted.

This is a text-completion compatibility layer, not the entire OpenAI API. Unsupported fields—including `tools`, `temperature`, `max_tokens`, `stop`, and `response_format`—return HTTP 400. There is no Responses API, embeddings, images, logprobs, token-array prompt, batch prompting, or `n > 1` support. A client that always sends unsupported sampling fields needs to omit them.
