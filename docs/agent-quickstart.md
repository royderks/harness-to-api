# Run a local gateway with an agent

This is a copyable, noninteractive path for an agent working on the user's machine. A user request can authorize installation, setup, and one completion; do not ask again when it does. If authorization is missing, ask before those actions. Login may require the user to interact with the harness. Repair and remote access require separate consent when not already authorized. Never ask for their provider credentials or print the gateway key. Run each command on the host, not inside an untrusted sandbox. For a human-oriented overview see [getting started](getting-started.md); for other harnesses and clients see [harness setup](onboarding.md#harness-installation-guidance) and [examples](../examples/README.md).

## Install and sign in

Install the published native binary on macOS or Linux (amd64 or arm64):

```sh
H2A_INSTALL_ONLY=1 sh -c 'curl -fsSL https://raw.githubusercontent.com/royderks/harness-to-api/main/install.sh | sh'
export PATH="$HOME/.local/bin:$PATH"
h2a --version
```

On Windows PowerShell:

```powershell
$env:H2A_INSTALL_ONLY = '1'
irm https://raw.githubusercontent.com/royderks/harness-to-api/main/install.ps1 | iex
$env:PATH = "$HOME\.local\bin;$env:PATH"
h2a --version
```

Installers default to `~/.local/bin` and verify the release archive checksum. They do not persist PATH: add that directory in every fresh shell or use the absolute binary path. If `H2A_INSTALL_DIR` is set, use that directory instead. The installer points to its pinned release; if that release is unavailable, stop and consult [releases](https://github.com/royderks/harness-to-api/releases). Do not build or install a different version without the user's consent.

Install the official [Codex CLI](https://developers.openai.com/codex/cli/) if needed, run `codex login` interactively, and confirm `codex --version` works in the same shell as h2a. Login and model access are not established by finding the executable. For another harness follow its [installation and login guide](onboarding.md#harness-installation-guidance), and change both provider and model below. Bob also needs the extension path described there.

## Start and connect

The following blocks use `H2A_STATE`, `H2A_PORT`, and `H2A_EVENTS` as nonsecret configuration. Defaults are shown; set the same values in both terminals if overriding them. Choose a private local state directory and a free loopback port. Do not change an existing setup's settings to make this example fit.

In a terminal or managed session you own, run this in the foreground. Keep the session and its process handle; do not daemonize it or kill an unrelated listener. Its event file contains no key, but may contain local paths and key metadata.

<!-- quickstart-command: init -->
```sh
: "${H2A_STATE:=$HOME/.harness-to-api}"
: "${H2A_PORT:=8787}"
: "${H2A_EVENTS:=$H2A_STATE-events.ndjson}"
exec h2a init --provider codex --model codex/default --state-dir "$H2A_STATE" --port "$H2A_PORT" --no-probe --json > "$H2A_EVENTS"
```
<!-- /quickstart-command -->

Wait for a `ready` event in the event file; `plan` alone is not ready. If the process exits, inspect its `error` event and [troubleshooting](troubleshooting.md). Ready means listening, **not** successful inference or confirmed login. No inference happens at startup with `--no-probe`.

The onboarding key is scoped to exactly `codex/default`, expires one hour after creation, and permits 20 accepted requests, five per minute, one concurrent request, and 90 seconds per request. The server also has a 90-second timeout and concurrency of one. Accepted failures count; these limits are not provider spending limits. The harness still runs with the host user's filesystem and tool permissions.

Once one real completion is authorized, run this in a second terminal on the same machine. It reads the credential path, endpoint, and model from the ready event, then reads the secret directly from the file into the client process. It does not put the secret in an environment variable, argument, or output.

<!-- quickstart-command: client -->
```sh
: "${H2A_STATE:=$HOME/.harness-to-api}"
: "${H2A_EVENTS:=$H2A_STATE-events.ndjson}"
export H2A_EVENTS
python3 - <<'PYTHON'
import json, os, pathlib, urllib.request

events = [json.loads(line) for line in pathlib.Path(os.environ['H2A_EVENTS']).read_text().splitlines()]
ready = next(event for event in reversed(events) if event['event'] == 'ready')
key = pathlib.Path(ready['credential_path']).read_text().strip()
body = json.dumps({'model': ready['model'], 'messages': [
    {'role': 'user', 'content': 'Say hello in one short sentence.'}]}).encode()
request = urllib.request.Request(ready['endpoint'] + '/chat/completions', data=body,
    headers={'Authorization': 'Bearer ' + key, 'Content-Type': 'application/json'})
class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None
opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), NoRedirect())
with opener.open(request, timeout=100) as response:
    result = json.load(response)
if result.get('error') or not result.get('choices'):
    raise ValueError('Completion failed or contained no choices')
choice = result['choices'][0]
text = choice.get('message', {}).get('content')
if choice.get('finish_reason') != 'stop' or not isinstance(text, str) or not text.strip():
    raise ValueError('Completion did not finish with text')
print(text)
PYTHON
```
<!-- /quickstart-command -->

Expect a short greeting; the exact words depend on the harness. A successful response proves only this request. Errors do not prove model access, and an accepted failed request still consumes allowance. The client requires Python 3; it has no third-party dependencies. Native Windows process cancellation has not been verified by the POSIX fixture checker.

For Windows PowerShell, use these equivalent commands in two terminals. The first stays in the foreground:

```powershell
$env:H2A_STATE = Join-Path $HOME '.harness-to-api'
$env:H2A_EVENTS = "$env:H2A_STATE-events.ndjson"
h2a init --provider codex --model codex/default --state-dir $env:H2A_STATE --port 8787 --no-probe --json > $env:H2A_EVENTS
```

After `ready`, and with the request authorized, in the second terminal:

```powershell
$events = Get-Content "$HOME\.harness-to-api-events.ndjson" | ForEach-Object { $_ | ConvertFrom-Json }
$ready = @($events | Where-Object { $_.event -eq 'ready' })[-1]
$key = (Get-Content -Raw -LiteralPath $ready.credential_path).Trim()
$body = @{ model = $ready.model; messages = @(@{ role = 'user'; content = 'Say hello in one short sentence.' }) } | ConvertTo-Json -Depth 5
$result = Invoke-RestMethod "$($ready.endpoint)/chat/completions" -Method Post -Headers @{ Authorization = "Bearer $key" } -ContentType 'application/json' -Body $body
if ($result.error -or -not $result.choices -or $result.choices[0].finish_reason -ne 'stop' -or [string]::IsNullOrWhiteSpace($result.choices[0].message.content)) { throw 'Completion did not finish with text' }
$result.choices[0].message.content
Remove-Variable key
```

If the state path is changed, use the matching event path in the second terminal. With explicit repair consent, stop init and run `h2a init repair --state-dir "$HOME\.harness-to-api" --discard --yes --json`, adjusting the path if needed. The POSIX fixture checker does not execute these Windows commands.

Stop the foreground process you started with Ctrl+C in its terminal (or send SIGINT to that process through your session manager on POSIX), and wait for it to exit. Do not terminate other processes. The listener is loopback only; this guide does not start a public tunnel.

## Return later

Restart the same command with the same values. A valid setup resumes the same key and preserves expiry and usage; it does not create a fresh allowance. After expiry, exhaustion, revocation, or missing/inconsistent state, init refuses to silently replace the key. Explain the error and ask the user before discarding the onboarding setup. With their explicit consent, after stopping your gateway, run:

<!-- quickstart-command: repair -->
```sh
: "${H2A_STATE:=$HOME/.harness-to-api}"
h2a init repair --state-dir "$H2A_STATE" --discard --yes --json
```
<!-- /quickstart-command -->

Repair revokes the recorded onboarding key and removes its profile and credential; unrelated keys are retained. If identity is inconsistent, follow the reported manual recovery instructions instead of guessing. Only after consent to a new setup, run init again; it creates a new bounded key. See [onboarding and recovery](onboarding.md#automation-and-recovery) for details.
