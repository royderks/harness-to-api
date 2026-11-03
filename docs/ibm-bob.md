# IBM Bob through Pi

This adapter runs the official Pi CLI with the community [pi-bob extension](https://pi.dev/packages/pi-bob). It does not invoke IBM's official Bob Shell CLI. It was checked with `@earendil-works/pi-coding-agent` 1.5.0 and `pi-bob` 0.2.6. Pi requires Node.js 22.19 or later; the h2a binary itself still has no Node dependency.

Install the packages, then start Pi explicitly with the Bob extension:

```sh
npm install -g @earendil-works/pi-coding-agent@1.5.0 pi-bob@0.2.6
PI_OFFLINE=1 pi -e "$(npm root -g)/pi-bob"
# Inside Pi: /login ibm-bob
```

Follow the extension's browser login flow and exit Pi when complete. Pi manages its own Bob credentials. h2a does not read or copy Bob Shell's stored tokens. `PI_OFFLINE=1` skips Pi's general catalog refresh; the extension's Bob discovery remains separate. For an approved IBM API key instead, configure `IBM_BOB_API_KEY` through your secret manager; the extension also accepts `IBM_BOB_KEY`. Never put your IBM credential in the gateway's `OPENAI_API_KEY` variable.

Create a separate gateway key and serve:

```sh
export OPENAI_API_KEY="$(h2a keys create --name bob-client --models 'ibm-bob/premium')"
h2a serve --provider ibm-bob --pi-bob-extension "$(npm root -g)/pi-bob"
```

Use `model: "ibm-bob/premium"` in the normal `/v1/chat/completions` request. `ibm-bob/default` also maps to `premium`; it is a distinct scope, so a key for `premium` does not authorize `default`. Other exposed routes may be selected by name, including nested IDs such as `ibm-bob/openai/gpt-oss-20b`. Model availability is controlled by your IBM account and can change. Run `pi -e "$(npm root -g)/pi-bob" --list-models ibm-bob` to inspect the extension's catalog. h2a's `/v1/models` still lists enabled aliases and explicit key scopes, not the live IBM catalog.

`--pi-bin /absolute/path/to/pi` selects another Pi executable. The extension path must be an absolute, already-installed package directory containing `extensions/bob.ts`. h2a does not install packages during a request. `--provider all` includes IBM Bob and requires this extension path as well as all six harness CLIs. The bundled Docker image includes Codex and Pi/Bob; Antigravity remains a native installation.

Each request sends the prompt through stdin in a fresh temporary directory. Pi runs in JSON mode with tools, other extension discovery, context files, skills, prompt templates, themes, and session persistence disabled. Only assistant text deltas and reported token usage are forwarded. Provider/model mismatches, tool execution and failed or incomplete assistant results fail the request. The gateway passes an explicit allowlist of Bob environment settings only to Pi, excluding the client gateway key. Pi's normal login directory remains accessible; this is not an OS isolation boundary. The extension is executable code you are choosing to trust.

## Verification

Go subprocess fixtures check text filtering, model selection, slash-containing IDs, key scopes, usage, subprocess failure, cancellation, and gateway-secret exclusion. The optional test below also runs **real Pi 1.5.0 and pi-bob 0.2.6 against a local fake IBM HTTP endpoint**, with isolated login/config directories. It checks request authentication, tool disabling, streaming and usage without an IBM account:

```sh
H2A_PI_BIN="$(command -v pi)" H2A_PI_BOB_EXTENSION="$(npm root -g)/pi-bob" \
  go test ./internal/harness -run TestPiBobRuntime -v
```

That check passed locally on 2026-11-03. It does not prove live IBM login, entitlement, refresh or inference. Those remain unverified here. The [Pi source and documentation](https://github.com/earendil-works/pi) define the CLI protocol; the [extension source](https://github.com/songlining/pi-bob) defines Bob auth and model discovery.
