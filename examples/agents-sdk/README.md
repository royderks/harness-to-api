# Text-only OpenAI Agents SDK

Run an incident editor using your harness's existing login through h2a. The SDK
uses `OpenAIChatCompletionsModel` explicitly; its default Responses API is not
supported by h2a.

Follow [gateway setup](../README.md#start-a-bounded-gateway), then:

```sh
python -m pip install -r examples/requirements.txt
export H2A_MODEL=codex/default
```

<!-- example-command: agents-sdk -->
```sh
python examples/agents-sdk/main.py
```
<!-- /example-command -->

One accepted request, `max_turns=1`, no tools, handoffs, structured outputs, or
sampling parameters. Tracing is disabled, so this needs no separate OpenAI API
account or tracing key. `OPENAI_API_KEY` is the **h2a gateway key**.

Cleanup: use the shared gateway cleanup. No agent session or output file is saved.
See the SDK's [custom models](https://openai.github.io/openai-agents-python/models/)
and [tracing configuration](https://openai.github.io/openai-agents-python/tracing/).
