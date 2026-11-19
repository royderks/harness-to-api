# Your coding harness, available through an API

Run the coding harnesses you already use behind a local, authenticated, OpenAI-compatible text API. Keep provider logins on the host and give each client its own bounded gateway key.

[Get started](getting-started.md) · [Quickstart for agents](agent-quickstart.md) · [View on GitHub](https://github.com/royderks/harness-to-api)

## Start with an outcome

### Use Codex from an OpenAI client

Connect an existing client to your local gateway and send a text completion with `codex/default`. Start with the [getting started guide](getting-started.md), then try the [OpenAI Agents SDK example](../examples/agents-sdk/README.md).

### Combine Codex and Gemini with LangGraph

Use one harness to draft and another to review in a two-step workflow. The [LangGraph example](../examples/langgraph/README.md) shows the setup and request count.

### Connect a local OpenShell sandbox

Expose h2a as an endpoint-bound inference provider while the harness runs on the host. Follow the [OpenShell example](../examples/openshell/README.md).

## A small gateway with clear boundaries

h2a supports text chat completions, legacy completions, model listing, and streaming. It rejects unsupported API fields. Gateway keys can limit models, expiry, accepted requests, rate, duration, and concurrency; they do not limit provider billing or isolate host-side harness execution.

Explore the [API](api.md), [key policies](policies.md), [remote access](remote-access.md), and [examples](../examples/README.md).


## Give your agent a function

The [client tool-calling recipe](../examples/tool-calling/README.md) shows a two-turn loop through the OpenAI SDK and Agents SDK. The gateway proposes a function call; the application executes its allowlisted function and returns the result. See the [API contract](api.md#client-function-tools) for the supported schema subset and validation boundary. Install v0.1.1 for tool support; v0.1.0 remains text-only.
