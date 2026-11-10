# Draft and review with LangGraph

A two-node graph drafts an incident update, then asks another harness to check it
for invented facts. The graph uses the official OpenAI Python client inside its
nodes so it sends only h2a's supported text fields.

Follow [gateway setup](../README.md#start-a-bounded-gateway), then:

```sh
python -m pip install langgraph==0.6.11 openai==3.49.0
export H2A_MODEL=codex/default
export H2A_REVIEW_MODEL=gemini/default
python examples/langgraph/main.py
```

The reviewer defaults to `H2A_MODEL` if unset. Enable and scope both models for a
cross-harness run. Exactly two accepted requests; no tools, agentic loop, or
automatic retry. Both stages print their output. No LangSmith account is required;
tracing is explicitly disabled. No local output files are created.

Cleanup: use the shared gateway cleanup. [LangGraph graph API](https://docs.langchain.com/oss/python/langgraph/quickstart).
