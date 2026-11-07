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

## Gemini, Claude Code and OpenCode

Added adapters and individual or comma-separated provider selection on 2026-11-03. Combined race tests, vet, and fixture HTTP checks passed. The HTTP check ran all three new adapters in one server and covered model listing, normal completions, SSE, gateway-key exclusion, scope rejection and revocation. It used subprocess fixtures, not provider inference.

- Gemini: installed CLI 0.26.0 protocol/settings checked. The final adapter was exercised through HTTP, but live authentication failed because this account requires `GOOGLE_CLOUD_PROJECT` or `GOOGLE_CLOUD_PROJECT_ID`. Neither is configured here. Google authentication/project variables are now forwarded only to Gemini. No successful live response is claimed.
- Claude Code: official CLI 2.1.317 installed temporarily, accepted the adapter flags and reported empty tools/MCP lists. Safe mode preserves the normal login path; the CLI reported no existing login. Protocol fixtures passed; subscription inference remains unverified.
- OpenCode: official v2.2.5 CLI installed temporarily, confirmed standalone JSON flags and emitted the expected structured error for an unavailable model. Protocol fixtures passed; a successful real-provider request remains unverified.

No global installations or account logins were changed. These adapters are not yet bundled in the Docker image. Existing Codex and Antigravity live results remain as recorded above.

## Bounded keys and model validation

Implemented at `126f74742a0707fc0055d9418cca97cc6ea31c98` on 2026-11-03. The independent final review traced this commit against `f63f31941eeb5df39e537a017eaeaba49697b134` and found no proved defects. It ran targeted auth, API, and CLI tests; that review is not a claim of native Windows verification.

Verified on macOS arm64:

- Full `make check build`: race tests, vet, formatting, and native build.
- Auth tests for concurrent final-quota admission, rolling-window expiry, clock rollback, durable counters after restart, legacy records, corrupt/unknown records, expiration, cancellation, and separate-process lock/revocation behavior.
- API tests for identical admission across streaming/nonstreaming requests, strict model filtering, expired model listing, zero debit on invalid requests, quota/rate/concurrency denials, and refusing a second server on the same state directory.
- `GO=/tmp/h2a-go-toolchain/go/bin/go python3 scripts/check-policies.py`: the built CLI and HTTP server drove an actual offline fixture subprocess. It checked durable quotas, read-only explanation, rate limits, timed-out streams, expired streams, separate-process revocation, and a quota-counted probe. It checked generated keys were absent from diagnostics. CI now runs this check on Linux/macOS.
- One live `codex/default` probe at `2026-11-03T23:24:49Z` returned a valid HTTP 200 completion with `available: true`. Its key had a two-minute lifetime, 60-second request timeout, one-request quota, and concurrency one. Inspection then showed one accepted request and zero remaining allowance. The temporary server and state were cleaned up.
- Six preview.4 release targets built: macOS, Linux, and Windows, each on amd64 and arm64. Their archive SHA-256 checksums verified.

Integration review found and fixed a probe that sent unsupported `max_tokens` and accepted arbitrary HTTP 200 bodies. The probe now sends the supported request shape, validates a completed chat response, and exits nonzero on failure without reflecting response content or credentials.

Windows locking/storage uses native APIs and its tests cross-compile; it has not been executed on Windows here. Hosted CI remains unrun because this repository has not been published. No cloud resources or tunnels were started for this change. See [policy semantics](policies.md) and [architecture and platform limits](architecture.md).

## First hosted CI run

On 2026-11-07, the private repository's [first hosted run](https://github.com/royderks/harness-to-api/actions/runs/42425919094) passed at baseline `072fa7497995113afffa07fcd7a38398cd92c0e9`.

- Ubuntu, macOS, and Windows jobs ran `go vet ./...`, `go test -race ./...`, and the native build successfully.
- Ubuntu and macOS also passed the offline demo and bounded-key CLI/HTTP lifecycle check.
- The workflow intentionally skipped those two Python/shell integration checks on Windows. Passing Go tests do not establish Windows descendant process-tree cleanup or execution of every cross-compiled archive.

This supersedes the earlier statements that hosted CI and native Windows tests had not run. It adds no live-provider, public distribution, signing, or cloud deployment evidence.
