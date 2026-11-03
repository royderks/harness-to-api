# Verification

Checked locally on November 3, 2026, on macOS arm64 using Go 1.27.3.

Passed:

- `go test -race ./...`: HTTP routes, auth failures, key revocation, model scopes, concurrent key creation, text content blocks, malformed/unsupported requests, body limits, streaming frames/usage, timeout/error handling, capacity limits, live HTTP flush, and disconnect cancellation.
- Subprocess protocol tests for both official CLI formats: valid output, filtered reasoning/tool data, missing results, malformed JSON, nonzero exits, cancellation, missing executables, and excluding gateway secrets from the child environment. These use a compiled fixture CLI, not provider inference.
- `H2A_LIVE_CODEX=1 go test ./internal/harness -run TestLiveCodex -v`: a real Codex 0.165.0 call using the local account login returned the requested text.
- Official OpenAI JavaScript SDK 7.47.0 against the built Go server over localhost: model listing, chat completions, legacy completions, SSE with usage, rejected unsupported fields, and immediate key revocation. Provider output for this client-compatibility check came from the subprocess fixture.
- Live demos with Codex 0.165.0 and Antigravity 1.4.12: real responses, missing-key HTTP 401, and revoked-key HTTP 401. Both recordings are under `docs/assets/` as HTML replays, asciicast files, and transcripts. The demos cleaned up their temporary gateway and keys.
- OpenShell 0.1.6: a Python 3.13 client in a Docker-backed sandbox called h2a and received a real Codex response. See [setup, evidence, limitations, and cleanup](openshell-live.md). This was one non-streaming request on macOS/Colima, using the built local gateway.
- CLI onboarding, command help, invalid flags, key lifecycle, and secret-output checks. Integration review fixed recursive help dispatch and rejected previously ignored or invalid flags.
- Offline fixture demo with a prebuilt binary; Python and shell syntax checks. Replay rendering at desktop/mobile widths and play/pause/seek controls checked in a browser.
- `go vet ./...`.
- Native release build plus cross-compilation for macOS/Linux/Windows, arm64/amd64. Archives and SHA-256 checksums generated under `dist/`.

Not verified:

- Public HTTPS tunnels and streaming through OpenShell. The validated OpenShell route used loopback and local VM forwarding.
- Runtime tests on Linux/Windows or macOS amd64. CI is configured for Linux, macOS, and Windows, but has not run on a hosted repository.
- Windows process-tree cleanup beyond the direct child; macOS signing/notarization and Windows signing.
- npm/Homebrew/public release distribution: not published.

The local Go toolchain was downloaded into `/tmp/h2a-go-toolchain` from go.dev and its SHA-256 was checked against the official release metadata. It was not installed globally. The official Antigravity 1.4.12 CLI was downloaded into `/tmp/h2a-antigravity-cli` and checked against its updater manifest SHA-512; it used the existing account login. The temporary OpenShell tools were checked against NVIDIA release SHA-256 files. No global harness or gateway service was installed.

## IBM Bob and deployment follow-up

On 2026-11-03, added the Pi-backed IBM Bob adapter. The installed Pi 1.5.0 and pi-bob 0.2.6 passed a real runtime check against a local fake IBM endpoint, with isolated configuration and test credentials. This is protocol evidence, not live IBM inference. See [IBM details](ibm-bob.md). Compose configuration validates; a Docker storage database I/O error blocked the image build. See [Docker status](docker.md). No Cloudflare or Vercel resources were provisioned.
