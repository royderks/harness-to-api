# Foreground onboarding

Run `h2a init` and choose the harness you want to add by number or name. Executable harnesses found on PATH are marked **installed** in the chooser (Pi is labeled separately because its presence does not verify the Bob extension); choose one to continue setup or another to see how to install it. If the selected CLI is not on PATH, init immediately shows its installation and sign-in commands, then exits without creating gateway state. Run those commands yourself and rerun init. It does not print a missing-binary inventory or execute installers. When the selected harness is available, setup asks for a model and shows the plan before asking for consent. The default harness is the first supported CLI found on PATH, or Codex when none are installed. Other defaults are `provider/default`, port 8787, and the usual state directory (`~/.harness-to-api`, or `HARNESS_API_STATE_DIR`). Presence of a binary does not establish login or model access. A separate inference probe defaults to no; a successful probe establishes access only for that request and consumes one accepted request.

Init listens on loopback in the foreground until Ctrl+C. Its onboarding key is scoped to the exact chosen model, expires one hour after creation, and allows 20 accepted requests, 5 per minute, one concurrent request, and 90 seconds per request. The server timeout is 90 seconds and its global concurrency is one. Accepted requests count even if they fail or are cancelled. These are admission limits, not provider dollar limits. A model scope does not isolate the harness from the host filesystem or tools.

The plan and ready output identify the credential file. With the default state directory, an Python standard-library client can read it in a separate shell:

```sh
H2A_KEY_FILE="$HOME/.harness-to-api/init/client.key" python3 - <<'PYTHON'
import json, os, pathlib, urllib.request
key = pathlib.Path(os.environ['H2A_KEY_FILE']).read_text().strip()
body = json.dumps({'model': 'codex/default', 'messages': [
    {'role': 'user', 'content': 'Say hello.'}]}).encode()
request = urllib.request.Request(
    'http://127.0.0.1:8787/v1/chat/completions', data=body,
    headers={'Authorization': 'Bearer ' + key, 'Content-Type': 'application/json'})
with urllib.request.urlopen(request) as response:
    print(response.read().decode())
PYTHON
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

IBM Bob additionally requires `--pi-bob-extension` with the absolute path to the installed community pi-bob package. The presence of `pi` alone does not establish Bob setup.

A repeat with the same configuration resumes the existing key and preserves its expiry and usage. It cannot start another server while the state directory is owned. If the key is revoked, expired, exhausted, missing, or inconsistent, init requires repair rather than silently issuing a replacement. Temporary rate exhaustion can clear with time. Stop the process, then explicitly discard its onboarding setup:

```sh
h2a init repair --state-dir /private/path/h2a --discard --yes
```

Repair revokes only the recorded onboarding key and removes its profile and client credential. Without `--yes`, repair prompts on a terminal. It does not need an installed harness. A corrupt identity requires the manual steps reported by the command. You can also revoke the key with `h2a keys revoke KEY_ID`; a subsequent init requires repair. A new setup creates a new bounded key.

For init, exit status 0 means normal stop or decline; 2 means usage or local model validation; 3 means a prerequisite, unsafe path, configuration mismatch, repair requirement, ownership, or port conflict; 4 means probe failure; 1 means unexpected I/O or runtime failure; and 130 means cancellation before readiness. Existing commands retain their own exit behavior.

## Credential storage

The nonsecret profile is `<state>/init/profile.json`; the client secret is `<state>/init/client.key`. The existing `keys/` directory keeps hashes, policies and usage. Init requires a private local filesystem and refuses unsafe existing directories, symbolic links, shared files and credentials with broad access. It does not silently change existing permissions. On Linux/macOS, its directories use mode 0700 and files use 0600. Windows uses a protected current-user DACL rather than relying on Unix mode bits. Auth-store hash records and locks inside the verified private `keys/` directory may be owned by the current token user or its default owner. The private directories, client credential and profile still require current-user ownership and the strict DACL. Same-user processes and administrators remain outside this protection.

The profile records setup before the key is inserted. If interruption leaves a matching profile, credential and key record, repeat can finish setup. A missing key requires explicit repair because it may have been revoked. Repair records its intent before revoking the key, then removes the credential and profile. Corrupt identity cannot be guessed; inspect the recorded files before manual recovery. Forced termination can leave harness descendants; the existing [platform limits](architecture.md#operational-limits) still apply.

Init's stricter storage checks apply to onboarding, not a migration of existing manual key stores. Linux credential publication supports amd64 and arm64 on ext-family, XFS, Btrfs, tmpfs, overlayfs, ZFS and ramfs filesystems; unfamiliar filesystem types fail closed. macOS requires a local filesystem and rejects any extended ACL on private objects, including harmless ACLs. Only the verified system `/tmp` and `/var` aliases are accepted as symlink ancestors. Windows requires a fixed local drive with working security descriptors. Its write-through rename does not promise POSIX directory synchronization; interrupted removal can leave an ignored tombstone until repair is retried.

## Harness installation guidance

The commands shown by init follow the provider setup guides: [Codex](https://learn.chatgpt.com/docs/codex/cli), [Antigravity](https://antigravity.google/docs/cli/tutorial), [Claude Code](https://code.claude.com/docs/en/quickstart), [Gemini CLI](https://geminicli.com/docs/get-started/installation/), and [OpenCode 2](https://dev.opencode.ai/docs/). Windows OpenCode currently requires downloading a native binary, so init provides a command to open that download page rather than an unsupported package-manager command. [IBM Bob through Pi](ibm-bob.md) uses the community [pi-bob extension](https://pi.dev/packages/pi-bob) and requires its package directory as well as Pi. Presence on PATH still does not establish login or model access.
