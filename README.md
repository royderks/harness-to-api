# harness-to-api

**Use your coding harness from OpenAI clients, agent frameworks and sandboxes.**

Turn an installed, authenticated Codex, Claude Code, Gemini or other supported
harness into a local OpenAI-compatible chat endpoint. Give each client
its own key with a model scope, expiry and request limit. Provider logins stay on
the host.

One native binary, `h2a`. No Rust toolchain needed. Your harness still needs its
own installation and login.

[Getting started](docs/getting-started.md) · [Give this to your agent](docs/agent-quickstart.md) · [Examples](examples/README.md) · [API reference](docs/api.md)

## Getting started

Install and start setup in a terminal:

**macOS / Linux**
```sh
curl -fsSL https://raw.githubusercontent.com/royderks/harness-to-api/main/install.sh | sh
```

**Windows (PowerShell)**
```powershell
irm https://raw.githubusercontent.com/royderks/harness-to-api/main/install.ps1 | iex
```

Select your harness and model, then keep the terminal open while the gateway runs.
Your client connects to `http://127.0.0.1:8787/v1` using the separate gateway key
file shown by setup. The default onboarding key lasts one hour and permits 20
requests to the selected model. A live probe is optional and consumes a request.

Next, follow the [first-request guide](docs/getting-started.md) in a second terminal.
It reads the key without printing it and checks the completion response. Stop the
gateway with **Ctrl+C**. [Installation details](docs/distribution.md) cover manual
downloads, PATH and supported platforms.

## Give this to your agent

Paste this into your coding agent:

```text
Read https://raw.githubusercontent.com/royderks/harness-to-api/main/docs/agent-quickstart.md
and set up harness-to-api for this project using an installed harness. Connect a
client and verify one completion. Keep credentials local and do not enable remote
access. Ask me to complete any provider login myself.
```

The [agent guide](docs/agent-quickstart.md) covers noninteractive setup, process
ownership, credential files, verification and expired-key recovery. It uses the
same CLI as the human quickstart.

## Three things to try

| Use case | What you get | Start here |
| --- | --- | --- |
| Use Codex from an OpenAI client | Stream a completion through your installed harness; switch models by changing the prefixed model ID. | [Harness switching](examples/harness-switch/) |
| Draft with Codex, review with Gemini | A two-step LangGraph workflow using two harnesses through one API. | [LangGraph](examples/langgraph/) |
| Give an OpenShell sandbox bounded access | Run the client in a local sandbox while provider login stays on the host. No paid OpenShell account. | [OpenShell](examples/openshell/) |

Also available: [OpenAI Agents SDK](examples/agents-sdk/), [Docker](examples/docker/),
[E2B](examples/e2b/) and [Modal](examples/modal/). Each example lists prerequisites
and request usage. [Verification](examples/VERIFICATION.md) separates offline
fixture checks from live-provider and real-sandbox evidence.

## Use an OpenAI client

After starting the gateway, use the credential file shown by setup. With the
default state directory:

```python
from pathlib import Path
from openai import OpenAI

client = OpenAI(
    base_url="http://127.0.0.1:8787/v1",
    api_key=(Path.home() / ".harness-to-api/init/client.key").read_text().strip(),
    max_retries=0,
)
response = client.chat.completions.create(
    model="codex/default",
    messages=[{"role": "user", "content": "Say hello."}],
)
print(response.choices[0].message.content)
```

Use the model selected during setup. SDK dependencies belong to the client, not
the gateway. See complete [Python](examples/client.py) and
[JavaScript](examples/client.mjs) examples.

## Supported harnesses

Install and sign in to the harness you want to use. Select it with `--provider` and use its model prefix in API requests.

| Harness | Provider | Example model | Setup |
| --- | --- | --- | --- |
| Codex | `codex` | `codex/default` | [Getting started](#getting-started) |
| Antigravity | `antigravity` | `antigravity/default` | [CLI installation and login](https://antigravity.google/docs/cli/tutorial/) |
| Gemini CLI | `gemini` | `gemini/default` | [Setup guide](docs/gemini.md) |
| Claude Code | `claude` | `claude/default` | [Setup guide](docs/claude.md) |
| OpenCode | `opencode` | `opencode/<provider>/<model-id>` | [Setup guide](docs/opencode.md) |
| IBM Bob through Pi | `ibm-bob` | `ibm-bob/premium` | [Setup guide](docs/ibm-bob.md) |

Enable multiple harnesses in one server and scope a key to them:

```sh
h2a keys create --name mixed-client --models 'codex/*,gemini/*'
h2a serve --provider codex,gemini
```

`--provider all` enables all six adapters and requires their CLIs and extensions to be installed. `/default` uses the harness default, except `ibm-bob/default`, which selects `premium`. Select a specific model with its prefixed ID. Run `agy models` for available Antigravity model slugs. `/v1/models` lists configured aliases and key scopes; it does not discover account entitlements.

Codex skips user configuration but retains account login. Its CLI must support `--ignore-user-config`, `--ignore-rules`, and `--ephemeral` (validated against 0.165.0). The linked guides cover other harness-specific requirements and verification limits.

h2a invokes installed CLIs and extensions; it does not extract OAuth tokens or call private provider endpoints. Each provider controls login, subscription access, model eligibility, and limits. Gateway keys are separate from provider credentials.

## Commands

<!-- commands:start -->
| Command | Purpose |
| --- | --- |
| `h2a init` | Set up bounded local access and run the gateway in the foreground. |
| `h2a init repair` | Discard an interrupted onboarding setup after identity checks and explicit confirmation. |
| `h2a doctor` | Check installed harness and tunnel binaries; does not verify login. |
| `h2a serve` | Start the API with existing keys and selected harnesses. |
| `h2a probe` | Make a real completion request to verify access; consumes quota. |
| `h2a share` | Print tunnel setup instructions; does not change networking. |
| `h2a keys create` | Create a scoped key with optional expiry and limits; prints its secret once. |
| `h2a keys list` | List stored key metadata without secrets. |
| `h2a keys inspect` | Show a key's policy and durable usage. |
| `h2a keys explain` | Check whether a model would be allowed by a key and supplied configuration. |
| `h2a keys revoke` | Revoke a key and cancel its active managed requests. |
| `h2a version` | Print the gateway version; also available as `h2a --version`. |
| `h2a help` | Show usage; also available as `h2a --help` or `h2a -h`. |
<!-- commands:end -->

For command flags, use commands such as `h2a help serve` or `h2a keys create --help`. See the [CLI guide](docs/cli.md) for full usage.

## API and execution boundary

Text chat completions, legacy completions, model listing, and SSE streaming are implemented. See [the API contract](docs/api.md) for supported fields and errors. Chat Completions also supports [client function tools](examples/tool-calling/README.md): your application executes the function and returns its result. Tool turns use a validated structured-output bridge, not native harness tool dispatch. The full OpenAI API is not implemented. Tool support requires a build from this source revision; v0.1.0 does not include it.

## Keys and permissions

```sh
h2a keys create --name sandbox-42 --models 'codex/default'
h2a keys list
h2a keys revoke KEY_ID
```

Keys contain 256 bits of random secret material. Hashes and metadata live in `~/.harness-to-api/keys`, with restrictive file permissions on Unix. Use `--state-dir PATH` on key and server commands, or `HARNESS_API_STATE_DIR`, to change the location. Revocation affects new requests without restarting and cancels active managed runs when the admission watcher observes the change.

Exact-model and provider scopes, expiry, request quotas, rate limits, and concurrency limits are enforced now. The stored permission names are `models:read` and `completions:create`. Dollar budgets, tool permissions, and stronger per-client execution isolation remain future work. See [policy semantics](docs/policies.md). Do not put keys in URLs.

## Remote sandboxes

Keep the server on loopback and put an HTTPS tunnel in front of it:

```sh
# Show the private-tailnet command; does not change networking:
h2a share --via tailscale

# Show the public HTTPS command for sandboxes outside your tailnet:
h2a share --via funnel
```

See [remote access](docs/remote-access.md) for Tailscale, named Cloudflare tunnels, and sandbox setup. See the [OpenShell setup](docs/openshell-live.md) and [integration boundaries](docs/openshell.md) for using it with a sandbox runtime. No tunnel is started automatically.

**Execution boundary:** an API key grants access to a local harness running under the host's OS account. Fresh temporary directories and harness restrictions are not a VM boundary. Codex uses read-only mode with shell tools and hooks disabled. Antigravity uses `--sandbox` and retains its local permission configuration, which can allow tools. Use a dedicated OS account or isolated host before giving keys to clients you do not trust. Never treat model scopes as filesystem or tool scopes.

## Hosting

Use the native binary for local setup. For an always-on endpoint, use the included [Docker Compose recipe](docs/docker.md) on a Linux VM with persistent login and key volumes, then put a named Cloudflare Tunnel in front. No public tunnel is created by the recipe. Validate the image and its login lifecycle on your target Docker host before deployment.

[Deployment choices](docs/deployment.md) covers Cloudflare Containers and Vercel Functions, Sandbox, and Daemons. Container support alone does not preserve harness logins or key revocations across restarts and replicas.

## Operational controls

```sh
h2a serve --provider codex,gemini --port 8787 --timeout 120s --max-concurrent 4
h2a doctor
```

Defaults: loopback only, a 1 MiB request limit, 8 MiB response text limit, two-minute timeout, four active requests, and immediate HTTP 429 when busy. Provider output is capped. Client disconnects and timeouts cancel the child. Unix cancellation kills the process group; Windows currently kills the direct CLI process, so descendant cleanup needs further validation there.

Keys and client environment secrets are excluded from the child environment. Official CLI credential locations remain accessible for login. Gateway logs include request IDs and error codes, not prompts, keys, raw subprocess diagnostics, or model output. The official harness can retain its own logs; Codex is invoked with `--ephemeral`.

`--codex-bin`, `--antigravity-bin`, `--pi-bin`, `--gemini-bin`, `--claude-bin`, and `--opencode-bin` select executable paths locally. `--pi-bob-extension` selects the installed Bob package. The HTTP API cannot override executables, working directories, or command-line flags.

## Develop and release

For local development, build from this checkout with Rust 1.89+:

```sh
cargo build --locked --bin h2a --example harness-fixture
mkdir -p bin && cp target/debug/h2a bin/h2a
```

Run the offline checks, then optionally probe your running gateway:

```sh
H2A_HARNESS_FIXTURE="$PWD/target/debug/examples/harness-fixture" cargo test --locked --all-targets
cargo clippy --locked --all-targets -- -D warnings
./bin/h2a probe --base-url http://127.0.0.1:8787/v1 --model codex/default
```

The probe makes one model call using your existing Codex login. Normal tests use subprocess protocol fixtures and never require credentials.

Add harnesses by implementing [`harness::Adapter`](src/harness/mod.rs), registering the adapter in the CLI, and testing its official output protocol. The HTTP and key layers stay unchanged.

`scripts/release.sh v0.1.0` packages the host binary and its SHA-256 checksum; pass a Rust target triple as the second argument to select another target with a configured linker. The GitHub workflow builds and tests six targets (macOS/Linux/Windows × arm64/amd64) on native runners when an approved version tag is pushed. See [release build instructions](docs/distribution.md).

MIT licensed. Independent project; not affiliated with OpenAI or Google.

## Project direction and contribution workflow

[Product intent](GENESIS.md) records the intended product. [Decisions](DECISIONS.md) explains the implementation choices. The [GitHub factory guide](docs/factory.md) covers issue intake, bounded worker handoffs, verification, and human review.

Public participation is described in [contributing](CONTRIBUTING.md) and [governance](GOVERNANCE.md). Vote with a thumbs-up on an issue. External coding agents can [find ready work and submit fork PRs](docs/factory/external-agents.md) using their own compute and credentials.
