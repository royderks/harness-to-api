# One client, multiple harnesses

Stream the same incident-update prompt through multiple installed harnesses.
Only the `model` changes between requests. This demonstrates routing, not a model
quality or speed benchmark.

1. Follow [gateway setup](../README.md#start-a-bounded-gateway).
2. Install `openai==3.49.0` in your virtual environment.
3. Run:

```sh
export H2A_MODELS=codex/default,gemini/default
```

<!-- example-command: harness-switch -->
```sh
python examples/harness-switch/main.py
```
<!-- /example-command -->

One accepted request per model, sequentially. SDK streaming prints text as h2a
receives it; some harnesses emit whole messages rather than token deltas.

| Harness | Server provider | Example model | Host prerequisite |
| --- | --- | --- | --- |
| Codex | `codex` | `codex/default` | Official `codex` CLI and login |
| Antigravity | `antigravity` | `antigravity/default` | Official `agy` CLI and login |
| Gemini | `gemini` | `gemini/default` | Official `gemini` CLI and login |
| Claude Code | `claude` | `claude/default` | Official `claude` CLI and login |
| OpenCode | `opencode` | `opencode/default` | `opencode` and a configured provider |
| IBM Bob via Pi | `ibm-bob` | `ibm-bob/premium` | Pi, Bob access, community `pi-bob` extension |

Enable only the providers you installed using `serve --provider codex,gemini`;
extend the key's exact model scopes when changing the list. Bob additionally needs
`--pi-bob-extension /absolute/path/to/pi-bob`. See [Bob setup](../../docs/ibm-bob.md).
The `/default` aliases do not establish account access. Provider availability and
subscription eligibility are controlled upstream.

Cleanup: stop the gateway and remove the temporary key as described in the shared
setup. No state is written by this client. All six protocols are fixture-tested;
that does not establish six live account integrations.
