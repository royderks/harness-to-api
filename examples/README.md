# Examples

Use installed coding harnesses from ordinary API clients, workflow frameworks,
and sandboxes. Start with one local example; add a remote sandbox after it works.

| Example | What it demonstrates | Accepted requests per run | Extra account |
| --- | --- | --- | --- |
| [Harness switching](harness-switch/) | Stream the same prompt through different harnesses using the OpenAI Python SDK | One per selected model | Each selected harness's login |
| [LangGraph](langgraph/) | Draft with one harness, review with another in a fixed two-step graph | 2 | None beyond harness login |
| [OpenAI Agents SDK](agents-sdk/) | Run a text-only agent through Chat Completions | 1 | No OpenAI API account required |
| [Docker](docker/) | Run the client in a container, leaving harness login on the host | 1 | None for local Docker; remote route needed |
| [OpenShell](openshell/) | Use h2a as an endpoint-bound inference provider | 1 | None for local OpenShell; Docker required |
| [E2B](e2b/) | A hosted sandbox calls your host using a scoped gateway key | 1 | E2B account and API key |
| [Modal](modal/) | An ephemeral cloud sandbox calls your host using a scoped gateway key | 1 | Modal account and CLI login |

All commands run from the repository root. Python examples require Python 3.11+
(tested with 3.13). The gateway itself is a native binary. The examples install
client libraries, not another gateway runtime.

## Set up a client

```sh
python3 -m venv .venv
. .venv/bin/activate
python -m pip install -r examples/requirements.txt
export H2A_BIN="$PWD/target/debug/h2a"  # or the absolute path to your installed binary
```

For a source checkout, first run `cargo build --locked --bin h2a --example harness-fixture`.
Each example also lists only the dependencies it needs, so installing the full
requirements file is optional. The pinned versions are tested versions, not a claim
that every newer SDK version works.

## Start a bounded gateway

Install and authenticate your chosen harnesses first. This example enables Codex
and Gemini; for one harness remove the other from both lists. A key controls
gateway access, not the provider's account entitlement.

Terminal 1 (POSIX shell):

```sh
umask 077
export H2A_EXAMPLE_STATE="$(mktemp -d)"
"$H2A_BIN" keys create --state-dir "$H2A_EXAMPLE_STATE" \
  --name examples --models 'codex/default,gemini/default' \
  --ttl 30m --max-requests 20 --request-timeout 90s \
  --rate-limit 10/min --max-concurrent 1 > "$H2A_EXAMPLE_STATE/client.key"
printf 'Client key file: %s/client.key\n' "$H2A_EXAMPLE_STATE"
"$H2A_BIN" serve --state-dir "$H2A_EXAMPLE_STATE" --provider codex,gemini
```

Terminal 2 (same virtual environment):

```sh
export OPENAI_BASE_URL=http://127.0.0.1:8787/v1
export OPENAI_API_KEY="$(cat /PATH/PRINTED/ABOVE/client.key)"
export H2A_MODEL=codex/default
python examples/agents-sdk/main.py
```

The key file and environment contain a secret. Do not print them or enable shell
tracing. Keep provider logins on the gateway host. Stop the gateway with Ctrl+C,
then in terminal 1 remove only this task's temporary state:

```sh
rm -rf -- "$H2A_EXAMPLE_STATE"
unset H2A_EXAMPLE_STATE
```

In terminal 2, `unset OPENAI_API_KEY`. For a persistent gateway, create a separate
bounded key per sandbox and revoke its ID with `h2a keys revoke` after use.
Accepted failures also consume quota. Re-running a workflow costs its full request
count; these examples disable client retries where configurable.

## Remote sandboxes

A cloud sandbox cannot reach your laptop's `127.0.0.1`. Use an existing HTTPS
endpoint through [Tailscale Funnel or a named Cloudflare Tunnel](../docs/remote-access.md),
and set `OPENAI_BASE_URL=https://YOUR-HOSTNAME/v1`. Keep h2a running on the host.
Cloudflare Quick Tunnels are unsuitable for the streaming example.

Sandbox code receives an h2a key, never a Codex/Google/Anthropic login. E2B and
Modal can access the gateway key injected into their workload; it is not invisible
to sandbox code. OpenShell has a separate credential-substitution mechanism.
The harness still executes on the gateway host; a remote client sandbox does not
isolate that host. See [execution boundaries](../docs/architecture.md).

## API contract for developers and coding agents

- Send `model` as `provider/model` and text `messages` to `/v1/chat/completions`.
- Use `stream: true` for SSE. No Responses API, tool calls, structured-output
  schema, `temperature`, `max_tokens`, or other sampling fields.
- The framework examples orchestrate text steps. They are not drop-in backends
  for a tool-using agent. No request fields are silently stripped.
- `_client.py` holds shared configuration and the dependency-free HTTP client
  copied into sandboxes. It rejects redirects and requires HTTPS outside literal
  loopback. SDK clients also disable redirects, proxy environment inheritance,
  and automatic retries. Framework tracing exports are disabled.
- Choose an example and read its README before running it. Do not create cloud
  resources, consume live inference, or assume access solely from a model alias.
- Client exceptions print the exception class and HTTP code, not raw provider
  diagnostics or headers. Inspect private diagnostics separately when debugging.

## Verification

```sh
H2A_BIN="$PWD/target/debug/h2a" \
H2A_FIXTURE="$PWD/target/debug/examples/harness-fixture" \
python scripts/check-examples.py
```

This runs the actual clients and gateway against canned harness subprocesses:
six streaming adapters, the two-node graph, the agent, and plain HTTP. It also
checks exact request accounting, quota denial, revocation, and secret-free output.
It creates and removes its own temporary state. **It makes no live inference or
cloud sandbox calls.** See [verification evidence](VERIFICATION.md).
