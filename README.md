# harness-to-api

**Your coding harness. One OpenAI-compatible endpoint.**

Put your coding harness behind an API key. Point an OpenAI client at it—from your laptop, a container, or a remote sandbox.

A single native Rust binary. End users do **not** need Rust, Node, or Python to run the gateway. Install and authenticate each harness CLI separately.

The project is **harness-to-api**; the executable is `h2a`.

## Getting started

Install and start h2a:

**macOS**
```sh
curl -fsSL https://raw.githubusercontent.com/royderks/harness-to-api/main/install.sh | sh
```

**Linux**
```sh
curl -fsSL https://raw.githubusercontent.com/royderks/harness-to-api/main/install.sh | sh
```

**Windows (PowerShell)**
```powershell
irm https://raw.githubusercontent.com/royderks/harness-to-api/main/install.ps1 | iex
```

The command installs h2a and asks which harness you want to add. If needed, setup shows its installation and sign-in commands. Keep the terminal open while the gateway runs. No Rust needed. See [installation details](docs/distribution.md) for manual downloads and requirements.

It shows the provider, endpoint, state location, and bounded key policy before setup. A live probe is a separate opt-in; setup alone does not make an inference request. The endpoint is unverified until a probe succeeds. Keep the process running and stop it with Ctrl+C. See [onboarding](docs/onboarding.md) for a client example, automation, and recovery.

The lower-level `keys create` and `serve` commands remain available; see the [CLI guide](docs/cli.md).

Give a sandbox bounded access with an expiring key:

```sh
export OPENAI_API_KEY="$(h2a keys create --name sandbox \
  --models 'codex/*' --ttl 1h --request-timeout 90s \
  --max-requests 20 --rate-limit 5/min --max-concurrent 1)"
```

Limits persist across restarts. Expiry and revocation cancel active managed runs. Use `keys inspect` to see policy and usage, `keys explain` for local checks, and `probe` for an explicit real request. See [key policies and model validation](docs/policies.md) and the [architecture](docs/architecture.md).

```sh
curl http://127.0.0.1:8787/v1/chat/completions \
  -H "Authorization: Bearer $OPENAI_API_KEY" \
  -H 'Content-Type: application/json' \
  -d '{"model":"codex/default","messages":[{"role":"user","content":"Explain a reverse proxy in one sentence."}],"stream":true}'
```

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
| `h2a factory validate` | Validate the repository's issue seed manifest locally. |
| `h2a factory plan` | Compare seeds with GitHub without publishing changes. |
| `h2a factory seed` | Publish seed labels/issues when explicitly requested with `--apply`. |
| `h2a factory adopt` | Explicitly bind a seed to an existing GitHub issue in the local journal. |
| `h2a factory recover` | Reconcile an uncertain issue publication before retrying. |
| `h2a version` | Print the gateway version; also available as `h2a --version`. |
| `h2a help` | Show usage; also available as `h2a --help` or `h2a -h`. |
<!-- commands:end -->

For command flags, use commands such as `h2a help serve` or `h2a keys create --help`. See the [CLI guide](docs/cli.md) and [factory guide](docs/factory.md) for full usage, including factory arguments.

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

## Use an OpenAI client

```python
import os
from openai import OpenAI

client = OpenAI(
    base_url=os.getenv("OPENAI_BASE_URL", "http://127.0.0.1:8787/v1"),
    api_key=os.environ["OPENAI_API_KEY"],
)
response = client.chat.completions.create(
    model="codex/default",
    messages=[{"role": "user", "content": "Say hello."}],
)
print(response.choices[0].message.content)
```

See [JavaScript](examples/client.mjs) and [Python](examples/client.py) examples. SDK dependencies are needed by the example client, not by the gateway.

## API and execution boundary

Text chat completions, legacy completions, model listing, and SSE streaming are implemented. See [the API contract](docs/api.md) for supported fields and errors. This is a text-completion adapter; tool calling and the full OpenAI API are not implemented.

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
