# Verification

Checked locally on November 3, 2026, on macOS arm64 using Go 1.27.3.

Passed:

- `go test -race ./...`: HTTP routes, auth failures, key revocation, model scopes, concurrent key creation, text content blocks, malformed/unsupported requests, body limits, streaming frames/usage, timeout/error handling, capacity limits, live HTTP flush, and disconnect cancellation.
- Subprocess protocol tests for both official CLI formats: valid output, filtered reasoning/tool data, missing results, malformed JSON, nonzero exits, cancellation, missing executables, and excluding gateway secrets from the child environment. These use a compiled fixture CLI, not provider inference.
- `H2A_LIVE_CODEX=1 go test ./internal/harness -run TestLiveCodex -v`: a real Codex 0.165.0 call using the local account login returned the requested text.
- Official OpenAI JavaScript SDK 7.47.0 against the built Go server over localhost: model listing, chat completions, legacy completions, SSE with usage, rejected unsupported fields, and immediate key revocation. Provider output for this client-compatibility check came from the subprocess fixture.
- `go vet ./...`.
- Native release build plus cross-compilation for macOS/Linux/Windows, arm64/amd64. Archives and SHA-256 checksums generated under `dist/`.

Not verified:

- Live Antigravity inference: `agy` is not installed here. The adapter follows Google's documented `--input-format stream-json` / `--output-format stream-json` protocol.
- Public HTTPS tunnels or an OpenShell deployment: none has been configured. Documentation describes deployment options, not a verified integration.
- Runtime tests on Linux/Windows or macOS amd64. CI is configured for Linux, macOS, and Windows, but has not run on a hosted repository.
- Windows process-tree cleanup beyond the direct child; macOS signing/notarization and Windows signing.
- npm/Homebrew/public release distribution: not published.

The local Go toolchain was downloaded into `/tmp/h2a-go-toolchain` from go.dev and its SHA-256 was checked against the official release metadata. It was not installed globally.
