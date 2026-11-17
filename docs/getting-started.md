# Your first local request

harness-to-api (`h2a`) gives clients a local, OpenAI-compatible text API for an installed coding harness. Your provider login stays on the host; the client reads a separate gateway credential.

## Install and start

Install and sign in to the official [Codex CLI](https://developers.openai.com/codex/cli/): run `codex login` and check `codex --version` in your terminal. Login and model access depend on your account.

On macOS or Linux (amd64 or arm64), install the published native binary:

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

The installer checks the archive checksum and defaults to `~/.local/bin`; add it to PATH in each fresh shell, or use the absolute path. If you set `H2A_INSTALL_DIR`, use that directory. The installer uses its pinned release; see [releases](https://github.com/royderks/harness-to-api/releases) if it is unavailable.

In **terminal 1**, start the interactive setup:

```sh
h2a init
```

Choose Codex, accept `codex/default`, port `8787`, and the default state directory, confirm the plan, and decline the optional probe. Leave this terminal running. `Ready (unverified)` means the local listener is up; it does not confirm account or model access. Init uses loopback and creates a key scoped to exactly that model, expiring after one hour, with 20 accepted requests, five per minute, one concurrent request and a 90-second request timeout. Accepted failures count; these limits do not cap provider spending. The harness runs with your host account's permissions.

## Send one request

In **terminal 2**, run the following on macOS or Linux with Python 3. It reads the default credential file directly, without printing the key or placing it in an environment variable or command argument. If you chose a different state directory, model or port, use the values shown by init.

```sh
python3 - <<'PYTHON'
import json, pathlib, urllib.request
key = (pathlib.Path.home() / '.harness-to-api/init/client.key').read_text().strip()
body = json.dumps({'model': 'codex/default', 'messages': [
    {'role': 'user', 'content': 'Say hello in one short sentence.'}]}).encode()
request = urllib.request.Request('http://127.0.0.1:8787/v1/chat/completions', data=body,
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

On Windows, use this in a second PowerShell terminal instead:

```powershell
$key = (Get-Content -Raw -LiteralPath "$HOME\.harness-to-api\init\client.key").Trim()
$body = @{ model = 'codex/default'; messages = @(@{ role = 'user'; content = 'Say hello in one short sentence.' }) } | ConvertTo-Json -Depth 5
$result = Invoke-RestMethod 'http://127.0.0.1:8787/v1/chat/completions' -Method Post -Headers @{ Authorization = "Bearer $key" } -ContentType 'application/json' -Body $body
if ($result.error -or -not $result.choices -or $result.choices[0].finish_reason -ne 'stop' -or [string]::IsNullOrWhiteSpace($result.choices[0].message.content)) { throw 'Completion did not finish with text' }
$result.choices[0].message.content
Remove-Variable key
```

Expect a short greeting; the exact text varies. A successful response proves access for this request only. To stop, press Ctrl+C in terminal 1 and wait for the gateway to exit. No public tunnel is started.

To return later, restart the same setup: it preserves the key's expiry and usage. An expired or revoked key requires explicit repair; it is not silently replaced. See [troubleshooting](troubleshooting.md), [onboarding and repair](onboarding.md#automation-and-recovery), [other harnesses](onboarding.md#harness-installation-guidance), and [client examples](../examples/README.md). For a noninteractive guide that an agent can follow, use [agent quickstart](agent-quickstart.md). The POSIX fixture checker does not verify the Windows PowerShell workflow.
