# harness-to-api

**Your coding harness. One OpenAI-compatible endpoint.**

Run Codex or Antigravity behind an API key. Point an OpenAI client at it—from your laptop, a container, or a remote sandbox.

A single Go binary, with no third-party Go dependencies. End users do **not** need Go, Node, or Python to run the gateway. Install and authenticate the official harness CLI separately.

**Status:** local preview. Real Codex and Antigravity requests have passed through the gateway, including a Codex request from an OpenShell sandbox. Native release archives are buildable; nothing has been published to a registry yet. The working binary is `h2a`; see the [researched name shortlist](docs/naming.md).

## Watch it work

Recorded real calls with temporary scoped keys, streamed responses, and revocation:

- **Codex:** [interactive replay](docs/assets/codex-demo.html) · [terminal recording](docs/assets/codex-demo.cast) · [transcript](docs/assets/codex-demo.txt)
- **Antigravity:** [interactive replay](docs/assets/antigravity-demo.html) · [terminal recording](docs/assets/antigravity-demo.cast) · [transcript](docs/assets/antigravity-demo.txt)

Run the same demo yourself:

```sh
./demo/run.sh                           # real Codex; needs Go, Python 3, and Codex login
./demo/run.sh --provider antigravity     # real Antigravity; needs agy login
H2A_BIN=./bin/h2a ./demo/run.sh           # use a prebuilt h2a; no Go required
./demo/run.sh --fixture                 # explicitly offline, no provider call
```

The demo cleans up its server and key store. Python uses only its standard library. See [demo instructions](docs/demo.md) for the OpenShell recipe and recording commands. GitHub displays HTML source; download/open a replay file locally to play it, or use an asciicast player for `.cast` files.

## Start locally

For local development, build from this checkout with Go 1.24+:

```sh
go build -o bin/h2a ./cmd/h2a
```

Install the [official Codex CLI](https://learn.chatgpt.com/docs/cli), then sign in:

```sh
codex login
```

Create a gateway key. The command prints the key once; the server stores only its hash. In a POSIX shell you can capture it directly:

```sh
export OPENAI_API_KEY="$(./bin/h2a keys create --name local --models 'codex/*')"
./bin/h2a serve --provider codex
```

See the [CLI guide](docs/cli.md) for command help and stdout/stderr contracts. Your base URL is `http://127.0.0.1:8787/v1`. Keep the server running. Pass the generated key to your client using its secret/environment configuration.

```sh
curl http://127.0.0.1:8787/v1/chat/completions \
  -H "Authorization: Bearer $OPENAI_API_KEY" \
  -H 'Content-Type: application/json' \
  -d '{"model":"codex/default","messages":[{"role":"user","content":"Explain a reverse proxy in one sentence."}],"stream":true}'
```

`codex/default` uses the CLI's default model. Select a specific available model with `codex/<model-id>`. Codex user configuration is deliberately skipped; account login is retained. The Codex adapter requires a CLI supporting `--ignore-user-config`, `--ignore-rules`, and `--ephemeral` (validated against 0.165.0).

## Antigravity

Install the [official Antigravity CLI](https://antigravity.google/docs/cli/tutorial/) and run `agy` interactively once to sign in. Then:

```sh
./bin/h2a keys create --name antigravity-client --models 'antigravity/*'
./bin/h2a serve --provider antigravity
```

Use `antigravity/default` or `antigravity/<model-slug>`. Run `agy models` for your available models. `--provider all` enables both harnesses in one server; both CLIs must be installed. Keys can allow both using `--models 'codex/*,antigravity/*'`.

The gateway invokes supported official CLIs. It does not extract OAuth tokens or call private provider endpoints. Harness login, subscription access, model eligibility, and provider limits remain controlled by each provider. An h2a key protects this gateway; it is not an OpenAI or Google provider key.

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

See [JavaScript](examples/client.mjs) and [Python](examples/client.py) examples. SDK dependencies are needed by the example client, not by the Go server.

## API and execution boundary

Text chat completions, legacy completions, model discovery, and SSE streaming are implemented. See [the API contract](docs/api.md) for supported fields and errors. This is a text-completion adapter; tool calling and the full OpenAI API are not implemented.

## Keys and permissions

```sh
h2a keys create --name sandbox-42 --models 'codex/default'
h2a keys list
h2a keys revoke KEY_ID
```

Keys contain 256 bits of random secret material. Hashes and metadata live in `~/.harness-to-api/keys`, with restrictive file permissions on Unix. Use `--state-dir PATH` on key and server commands, or `HARNESS_API_STATE_DIR`, to change the location. Revocation affects the next request without restarting; it does not cancel requests already running.

Exact-model and provider scopes are enforced now. The stored permission names are `models:read` and `completions:create`. Future work: expiration, per-key budgets, rate limits, tool permissions, and stronger per-client execution isolation. Do not put keys in URLs.

## Remote sandboxes

Keep the server on loopback and put an HTTPS tunnel in front of it:

```sh
# Show the private-tailnet command; does not change networking:
h2a share --via tailscale

# Show the public HTTPS command for sandboxes outside your tailnet:
h2a share --via funnel
```

See [remote access](docs/remote-access.md) for Tailscale, named Cloudflare tunnels, and sandbox setup. See the [validated OpenShell setup](docs/openshell-live.md) and [integration boundaries](docs/openshell.md) for using it with a sandbox runtime. No tunnel is started automatically.

**Execution boundary:** an API key grants access to a local harness running under the host's OS account. Fresh temporary directories and harness restrictions are not a VM boundary. Codex uses read-only mode with shell tools and hooks disabled. Antigravity uses `--sandbox` and retains its local permission configuration, which can allow tools. Use a dedicated OS account or isolated host before giving keys to clients you do not trust. Never treat model scopes as filesystem or tool scopes.

## Operational controls

```sh
h2a serve --provider all --port 8787 --timeout 120s --max-concurrent 4
h2a doctor
```

Defaults: loopback only, a 1 MiB request limit, 8 MiB response text limit, two-minute timeout, four active requests, and immediate HTTP 429 when busy. Provider output is capped. Client disconnects and timeouts cancel the child. Unix cancellation kills the process group; Windows currently kills the direct CLI process, so descendant cleanup needs further validation there.

Keys and client environment secrets are excluded from the child environment. Official CLI credential locations remain accessible for login. Gateway logs include request IDs and error codes, not prompts, keys, raw subprocess diagnostics, or model output. The official harness can retain its own logs; Codex is invoked with `--ephemeral`.

`--codex-bin` and `--antigravity-bin` select executable paths locally. The HTTP API cannot override executables, working directories, or command-line flags.

## Develop and release

```sh
go test -race ./...
go vet ./...
go build -o bin/h2a ./cmd/h2a
H2A_LIVE_CODEX=1 go test ./internal/harness -run TestLiveCodex -v
```

The live test makes one model call using your existing Codex login. Normal tests use subprocess protocol fixtures and never require credentials.

Add harnesses by implementing [`harness.Adapter`](internal/harness/harness.go), registering the adapter in the CLI, and testing its official output protocol. The HTTP and key layers stay unchanged.

`scripts/release.sh v0.1.0` creates six binaries (macOS/Linux/Windows × arm64/amd64), archives, and SHA-256 checksums. The included GitHub workflow runs on version tags after the repository is published. See [distribution decisions](docs/distribution.md). No release or registry publication has been performed.

MIT licensed. Independent project; not affiliated with OpenAI or Google.
