# Bounded API keys

Create one key per application, demo, or sandbox. The key controls access to h2a; your official CLI retains the provider login.

```sh
export OPENAI_API_KEY="$(h2a keys create \
  --name sandbox-demo --models 'codex/*' \
  --ttl 1h --request-timeout 90s \
  --max-requests 20 --rate-limit 5/min --max-concurrent 1)"
h2a serve --provider codex
```

Only the secret is printed to stdout. The key ID is printed to stderr. Limits are optional; existing keys retain their existing unlimited policy. The server's own timeout and concurrency limit still apply.

## Choose limits

| Flag | Meaning |
| --- | --- |
| `--ttl 1h` | Expire one hour after creation. |
| `--expires-at 2027-01-01T00:00:00Z` | Expire at an absolute RFC3339 timestamp with a timezone. Cannot be combined with `--ttl`. |
| `--request-timeout 90s` | Maximum duration of each admitted run. The earlier server timeout or key expiry wins. |
| `--max-requests 20` | Accept at most 20 completion requests over this key's lifetime. Set 1 for a single-use key. |
| `--rate-limit 5/min` | Accept at most five completion requests in a rolling 60-second window. |
| `--max-concurrent 1` | Allow one active run for this key. Other keys still share the server's capacity. |

An accepted request consumes its allowance before the harness starts. Startup failure, timeout, cancellation, and client disconnect still count. Invalid requests and policy denials do not count. Listing models and local inspection do not count. Usage persists across server restarts. A request can conservatively count if the server crashes after saving the debit but before starting the CLI.

Expired keys cannot start new requests. Active runs are canceled at expiry. Revocation also cancels active runs after the server observes the deletion; this uses polling, not a hard real-time guarantee. SSE requests already in progress can end with an error after HTTP 200 and must not be interpreted as a successful completion.

## Validate models

By default, h2a forwards syntactically valid model names permitted by the key to the enabled harness. It does not invent a catalog of the models your subscription can use.

For a server that accepts only configured names, supply an explicit list:

```sh
# Replace the example suffix with a model supported by your installed CLI/account.
h2a serve --provider codex --allowed-models 'codex/YOUR_MODEL_ID'
```

The list is intersected with each key's scopes. Wildcards and `/default` are rejected in this server list because they do not identify a concrete request model. A provider-defined name can still be an alias upstream; h2a does not guarantee an immutable underlying model version.

Validate a new exact-scoped key against the same configuration:

```sh
h2a keys create --name specific-model --models 'codex/YOUR_MODEL_ID' \
  --ttl 1h --validate --provider codex \
  --allowed-models 'codex/YOUR_MODEL_ID'
```

Validation is local. It checks syntax, enabled providers, scopes, and configured names. It neither contacts a provider nor establishes account access. `/v1/models` shows the configured intersection when this server list is supplied; otherwise it retains its synthetic alias/scope listing.

## Inspect and explain

```sh
h2a keys inspect KEY_ID
h2a keys explain --model codex/YOUR_MODEL_ID --provider codex \
  --allowed-models 'codex/YOUR_MODEL_ID' KEY_ID
```

Inspection returns policy and durable usage without the token or its digest. Explanation is a read-only snapshot against the configuration you supply. It is not a reservation and does not claim that the running server has identical flags or that the account can access the model. Use the same state directory as the server.

## Probe actual access

```sh
# OPENAI_API_KEY must contain the gateway key, supplied through your environment.
h2a probe --base-url http://127.0.0.1:8787/v1 --model codex/YOUR_MODEL_ID
```

This makes a real completion request through the gateway and consumes its allowance. It also uses provider/subscription capacity. The result records the check time; success is evidence of that request only. The command never prints the key or the returned completion text. Remote endpoints must use HTTPS, and redirects are rejected.

## Operate locally

Only one server may own a state directory at a time. Another server fails startup rather than keeping independent concurrency counters. Different state directories are separate gateways and do not share limits.

Use local storage with reliable OS file locks. Do not share the directory over a network filesystem or modify key records manually. Corrupt records or failed accounting writes deny admission rather than resetting usage. Clock rollback can temporarily prevent rate-limited requests until the stored window catches up.

These limits govern gateway admission and locally managed execution. They do not cap exact provider token usage, compute, or dollars. A force-killed server can leave orphaned CLI processes; normal graceful shutdown and cancellation remain the supported cleanup path. See [architecture](architecture.md) for the ownership and platform boundaries.
