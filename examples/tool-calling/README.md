# Client-side tool calling

These examples use the pinned `openai` and `openai-agents` SDKs. Follow the
[gateway setup](../README.md#start-a-bounded-gateway), install the pinned
requirements, and set `H2A_MODEL` to a model allowed by your h2a key.

<!-- example-command: tool-calling-1 -->
```sh
python examples/tool-calling/main.py
```
<!-- /example-command -->

<!-- example-command: tool-calling-2 -->
```sh
python examples/tool-calling/main.py --stream
```
<!-- /example-command -->

<!-- example-command: tool-calling-3 -->
```sh
python examples/tool-calling/agent.py
```
<!-- /example-command -->



Each flow makes two accepted completion requests and executes `lookup_status`
once locally against a fixed registry. h2a proposes calls; it does not execute
client functions. The Agents SDK uses Chat Completions and disables tracing.
The direct SDK streaming example buffers and validates the gateway result before exposing it; it does
not imply incremental model output. Schema validation is validate-or-error,
not a guarantee of constrained generation or correct answers. These examples
need a gateway with client tool calling enabled and do not establish live
provider reliability. Use the shared gateway cleanup after the demo.

The Agents SDK recipe uses nonstreaming requests and validates terminal finish reasons at its HTTP boundary before the SDK can dispatch a function. Use the direct SDK recipe for streaming; the pinned Agents SDK can treat an incomplete third-party stream as a completed turn.
