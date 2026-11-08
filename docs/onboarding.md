# Foreground onboarding

Install and sign in to an official harness first, then run `h2a init`. The interactive flow discovers executables, lets you choose a provider and model, and shows the plan before asking for setup consent. Defaults are Codex when present, `provider/default`, port 8787, and the usual state directory (`~/.harness-to-api`, or `HARNESS_API_STATE_DIR`). Presence of a binary does not establish login or model access. A separate inference probe defaults to no; a successful probe establishes access only for that request and consumes one accepted request.

Init listens on loopback in the foreground until Ctrl+C. Its onboarding key is scoped to the exact chosen model, expires one hour after creation, and allows 20 accepted requests, 5 per minute, one concurrent request, and 90 seconds per request. The server timeout is 90 seconds and its global concurrency is one. Accepted requests count even if they fail or are cancelled. These are admission limits, not provider dollar limits. A model scope does not isolate the harness from the host filesystem or tools.

The plan and ready output identify the credential file. With the default state directory, an ordinary POSIX client can use it in a separate shell:

```sh
export OPENAI_API_KEY="$(cat "$HOME/.harness-to-api/init/client.key")"
curl http://127.0.0.1:8787/v1/chat/completions \
  -H "Authorization: Bearer $OPENAI_API_KEY" \
  -H 'Content-Type: application/json' \
  -d '{"model":"codex/default","messages":[{"role":"user","content":"Say hello."}]}'
```

PowerShell equivalent:

```powershell
$env:OPENAI_API_KEY = (Get-Content -Raw "$HOME/.harness-to-api/init/client.key").Trim()
$headers = @{ Authorization = "Bearer $env:OPENAI_API_KEY" }
$body = @{ model = 'codex/default'; messages = @(@{ role = 'user'; content = 'Say hello.' }) } | ConvertTo-Json -Depth 5
Invoke-RestMethod http://127.0.0.1:8787/v1/chat/completions -Method Post -Headers $headers -ContentType 'application/json' -Body $body
```

Use the path shown by init if you selected another state directory. Treat this file as a secret. It is separate from the hash and policy stored by the auth store; do not include it in logs, commits, or command arguments. Init does not change existing client settings.

## Automation and recovery

```sh
h2a init --provider codex --model codex/default --state-dir /private/path/h2a --port 8787 --no-probe --json
```

Supply every shown choice explicitly for noninteractive operation; `--probe` opts into an actual request instead of `--no-probe`. JSON mode never prompts. Standard output is versioned NDJSON (`schema_version: 1`): `plan`, `ready`, `probe`, `stopped`, and `error` events. Ready means listening, not verified; a successful probe provides verification. Errors carry a stable code and actionable next step. A hard kill may truncate the stream.

A repeat with the same configuration resumes the existing key and preserves its expiry and usage. It cannot start another server while the state directory is owned. If the key is revoked, expired, exhausted, missing, or inconsistent, init requires repair rather than silently issuing a replacement. Temporary rate exhaustion can clear with time. Stop the process, then explicitly discard its onboarding setup:

```sh
h2a init repair --state-dir /private/path/h2a --discard --yes
```

Repair revokes only the recorded onboarding key and removes its profile and client credential. Without `--yes`, repair prompts on a terminal. It does not need an installed harness. A corrupt identity requires the manual steps reported by the command. You can also revoke the key with `h2a keys revoke KEY_ID`; a subsequent init requires repair. A new setup creates a new bounded key.

For init, exit status 0 means normal stop or decline; 2 means usage or local model validation; 3 means a prerequisite, unsafe path, configuration mismatch, repair requirement, ownership, or port conflict; 4 means probe failure; 1 means unexpected I/O or runtime failure; and 130 means cancellation before readiness. Existing commands retain their own exit behavior.
