# API compatibility

Implemented:

- `POST /v1/chat/completions`: text messages and client function-tool turns, including correlated assistant calls and tool results.
- `POST /v1/completions`: one string prompt.
- SSE streaming with `[DONE]`, OpenAI-shaped errors, and optional `stream_options.include_usage`.
- `GET /v1/models`: configured models permitted by the key when `serve --allowed-models` is set; otherwise enabled provider aliases and exact key scopes. This is not a live account/model catalog.
- `GET /healthz`: unauthenticated liveness only.

Conversation messages are serialized into one fresh harness prompt. This preserves supplied conversation text, but does not implement provider-native role precedence or saved chat sessions. Codex streams completed assistant message items; Antigravity, Pi/Bob, Gemini, and Claude stream their response deltas; OpenCode emits completed text parts. No adapter guarantees token-by-token latency. Reasoning and harness-internal tool output are filtered out. Client function calls follow the separate bridge contract below. Usage is the harness's aggregate run usage, which may cover multiple model calls; unavailable usage is omitted.

This is a text-completion compatibility layer, not the entire OpenAI API. Unsupported fields—including `temperature`, `max_tokens`, `stop`, and `response_format`—return HTTP 400. There is no Responses API, embeddings, images, logprobs, token-array prompt, batch prompting, or `n > 1` support. A client that always sends unsupported sampling fields needs to omit them.

## Admission limits

Keys may expire and restrict request duration, lifetime accepted requests, rolling-minute request rate, and concurrent runs. Both completion routes and both response modes use the same admission operation. Policy checks happen before starting a CLI or sending SSE headers. Invalid requests and denied admissions consume no allowance; all accepted requests count, including failed or canceled runs. Counts survive restart.

Capacity, per-key concurrency, rate, and lifetime-quota exhaustion return HTTP 429 with distinct error codes. `Retry-After` is supplied when a retry interval is meaningful; a lifetime quota does not reset automatically. Invalid or expired keys return HTTP 401. Unreadable accounting fails closed. During an existing SSE stream, cancellation or expiry is a stream error after HTTP 200, not a successful `[DONE]` completion.

See [bounded API keys](policies.md) for flags, inspection, validation, and live probes. Local model validation checks configuration and authorization; it does not establish provider entitlement or guarantee that a provider alias resolves to an immutable model version.

Duplicate JSON field names, including case-insensitive duplicates, are rejected
at every nesting level before admission. Use one value for each field.


## Client function tools

Use the existing `POST /v1/chat/completions` route. Send OpenAI-shaped function definitions in `tools`; your application executes the selected function and sends its result in the next request. The [executable recipe](../examples/tool-calling/README.md) covers streaming and nonstreaming with the OpenAI SDK, plus a nonstreaming Agents SDK loop. Client function tools are included in v0.1.1 and are not present in v0.1.0.

```json
{
  "model": "codex/default",
  "messages": [{"role": "user", "content": "Check the api service."}],
  "tools": [{"type": "function", "function": {
    "name": "lookup_status",
    "description": "Look up a local service status",
    "parameters": {
      "type": "object",
      "properties": {"service": {"type": "string"}},
      "required": ["service"],
      "additionalProperties": false
    },
    "strict": true
  }}],
  "parallel_tool_calls": false
}
```

A tool-call response has `finish_reason: "tool_calls"` and a standard assistant message:

```json
{
  "role": "assistant",
  "content": null,
  "tool_calls": [{
    "id": "call_EXAMPLE",
    "type": "function",
    "function": {"name": "lookup_status", "arguments": "{\"service\":\"api\"}"}
  }]
}
```

Preserve that assistant message in `messages`, then append `{"role":"tool","tool_call_id":"call_EXAMPLE","content":"operational"}` using the actual returned ID. Supply every result for that assistant turn before requesting the next turn. h2a rejects orphaned, repeated, or unresolved result IDs. History is carried by the client; there is no saved gateway chat session. A final text answer uses `finish_reason: "stop"`.

| Field | Supported behavior |
| --- | --- |
| `tools` | Function definitions with unique names, descriptions and supported object parameter schemas. Other tool types are rejected. |
| `tool_choice` | `auto` (default with tools), `none`, `required`, or `{"type":"function","function":{"name":"lookup_status"}}`. Required/named choices need matching definitions. |
| `parallel_tool_calls` | Boolean, default `true`; `false` allows at most one returned call. Execution scheduling belongs to your client. |
| `strict` | Returned arguments are validated against the supported schema. This is validate-or-error, not native constrained decoding. Non-strict definitions are also validated. |
| Assistant `tool_calls` | IDs, type `function`, name and JSON-string arguments; assistant content can be null. |
| Tool result | Role `tool`, text `content`, and the matching `tool_call_id`. |

### Supported parameter schemas

Function parameters must have a root `type: "object"`. Supported keywords are `type`, `properties`, `required`, boolean `additionalProperties`, `items`, `enum`, `anyOf`, and string `title`/`description` annotations. Types are `object`, `array`, `string`, `number`, `integer`, `boolean`, and `null`; arrays require `items`. Use `anyOf` for nullable values. Required names must refer to declared properties. Extra properties default to allowed unless disabled.

For `strict: true`, every object must set `additionalProperties: false` and list every property in `required`. Returned values are validated even when strict is false. Unknown keywords are errors, including `$ref`, `$defs`, `format`, `default`, `oneOf`, `allOf`, and numeric/string/array constraint keywords. Schema references are not resolved. This is a supported subset, not full JSON Schema.

Limits are 64 tools or returned calls, schema depth 16, 128 properties per object, 128 enum values, and 16 `anyOf` branches. Argument validation has a shared 100,000-step budget per response and fails closed when exceeded. The existing 1 MiB request and 8 MiB assistant-output limits apply. The recursive duplicate-key rule also applies inside schemas and arguments; keys differing only by ASCII case are rejected.

### Generation and execution boundaries

All configured harnesses use an API-owned structured-output bridge for these turns. The gateway asks the harness for a terminal answer or proposed calls, then validates the full output before returning it. It does not expose the harness's internal tools, run caller functions, or add retries to repair invalid output. Model compliance depends on the installed harness and model; fixture tests establish protocol behavior, not live model reliability or factual correctness.

Tool turns buffer output before emitting content or calls. Streaming still uses standard `chat.completion.chunk` objects: an assistant role, indexed `delta.tool_calls` with IDs, function names and JSON-string arguments, then `finish_reason: "tool_calls"`, optional usage, and `[DONE]`. Arguments may arrive in one complete chunk. Heartbeats can arrive while the model is running. Final-answer turns emit content with `stop`. This does not promise token-by-token tool latency.

Invalid request definitions, histories, or unsupported schema features return HTTP 400 before admission and consume no request allowance. Malformed model output returns sanitized HTTP 502 `invalid_harness_output`; after SSE headers it produces a stream error without a successful finish or `[DONE]`. Accepted failed turns still count. Each tool-loop round is a separate completion request, so the two-round examples consume two accepted requests. Cancellation, expiry and revocation use the same admission and subprocess cleanup as text requests.

Function names and arguments are untrusted model output. Clients must allowlist functions, validate arguments and authorize side effects before execution. Neither a valid schema nor an API key grants authority to run a shell command. The gateway's existing host execution boundary remains unchanged.
