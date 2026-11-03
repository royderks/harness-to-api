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
