# CLI guide

Install and sign in to the official harness CLI first (for example, `codex login`). Run `h2a doctor` to see which executables are on your PATH. Doctor does not check login or model access. The gateway binary itself does not require Go.

Create a key and capture it in your shell, then start the server:

```sh
export OPENAI_API_KEY="$(h2a keys create --name local --models 'codex/*')"
h2a serve --provider codex
```

The key is printed once to standard output. Save it securely; it cannot be recovered from the gateway. The key ID and a reminder go to standard error. The server stores a hash and metadata, not the secret. If the secret is lost, create another key and revoke the old one. Each `keys create` creates a new key, so do not rerun it as an idempotent setup step.

In another shell, set `OPENAI_API_KEY` through your secret manager or shell environment, then call the gateway:

```sh
curl http://127.0.0.1:8787/v1/chat/completions \
  -H "Authorization: Bearer $OPENAI_API_KEY" \
  -H 'Content-Type: application/json' \
  -d '{"model":"codex/default","messages":[{"role":"user","content":"Say hello."}]}'
```

The default state directory is `~/.harness-to-api`. Set `HARNESS_API_STATE_DIR` or pass `--state-dir PATH` to both key commands and `serve` to use another location. Server startup displays its listening address on standard error, followed by structured JSON diagnostics. Ctrl+C stops it.

```sh
h2a keys list
h2a keys revoke KEY_ID
h2a serve --help
h2a keys create --help
h2a help
```

For automation, `keys create` writes only the token and a newline to stdout; `keys list` writes a JSON array of key metadata to stdout, including IDs and scopes but no secrets. Successful `keys revoke` writes its confirmation to stderr. Diagnostics and flag help go to stderr; top-level help and version go to stdout. Commands exit nonzero on errors. A key grants access to the local harness under the server's OS account; model scopes do not isolate filesystem or tool access.

IBM Bob uses Pi with the community `pi-bob` extension. Pass `--provider ibm-bob --pi-bob-extension /absolute/path/to/pi-bob`; use `--pi-bin PATH` if Pi is not on PATH. `--provider all` enables all six adapters and also requires the extension path. See [IBM Bob setup](ibm-bob.md).

Select any installed subset with `h2a serve --provider codex,gemini,claude`. Provider names are `codex`, `antigravity`, `ibm-bob`, `gemini`, `claude`, and `opencode`; duplicates and empty entries are errors. Only selected CLIs must be installed. Use `--gemini-bin`, `--claude-bin`, or `--opencode-bin` to select their executable paths. `doctor` reports their versions but does not authenticate them.

## Key policies and local model checks

A key is unlimited by default. Optional flags set independent limits:

```sh
h2a keys create --name assistant --models codex/gpt-5 \
  --ttl 24h --request-timeout 2m --max-requests 100 \
  --rate-limit 10/min --max-concurrent 2
```

Use either `--ttl DURATION` or `--expires-at RFC3339` for expiry. Expiry must be in the future. The request timeout is at most one hour; rate limits are 1–100000 per minute and concurrent requests are 1–128. Explicit zero and negative limits are invalid. Accepted requests count against the lifetime quota even if they fail or are cancelled; denied requests do not. Revocation rejects new requests and cancels active requests.

For an explicit local catalog, supply exact names to the server and validate key scopes against the same provider subset and catalog:

```sh
h2a keys create --models codex/gpt-5 --validate --provider codex --allowed-models codex/gpt-5
h2a serve --provider codex --allowed-models codex/gpt-5
```

With `--validate`, every scope must be an exact catalog entry; wildcards and dynamic `default` names are rejected. Without a catalog, the server retains its normal passthrough behavior. Validation checks supported and configured names locally; it does not check account access or upstream availability. Provider aliases can change upstream.

```sh
h2a keys inspect KEY_ID
h2a keys explain --model codex/gpt-5 --provider codex --allowed-models codex/gpt-5 KEY_ID
```

Flags must precede `KEY_ID`. Inspect returns key metadata and usage as JSON, without the secret. Explain adds a local model decision and reports `account_access` as `unknown`. Both are read-only snapshots and do not consume quota or report live concurrency. Remaining lifetime quota is included when limited. These commands do not prove that a request will be admitted later.

## Probe a model

```sh
h2a probe --base-url http://127.0.0.1:8787/v1 --model codex/gpt-5
```

Probe reads `OPENAI_API_KEY` and sends a real completion request with a fixed prompt. It announces the request on stderr; it consumes quota when admitted. Its stdout JSON contains `checked_at`, `model`, `available`, and an HTTP status when received, without the completion text or provider errors. It requires a valid completed chat response; arbitrary HTTP 200 bodies do not count as success. A failed probe exits nonzero after printing its result. Success indicates the request succeeded at that moment, not lasting account entitlement. Redirects are disabled, requests time out after 90 seconds, and plain HTTP requires a literal loopback IP address; use HTTPS for remote gateways.
