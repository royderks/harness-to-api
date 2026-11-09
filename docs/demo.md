# Terminal demo

From the repository root, run `./demo/run.sh`. It builds h2a with Rust 1.89+ unless `H2A_BIN` points to a prebuilt binary, creates an isolated temporary key store and an exact-model-scoped key, starts a loopback gateway, shows a missing key rejected, streams a short response through an OpenAI-compatible standard-library Python client, revokes the key, and shows the revoked key rejected. It stops the gateway and removes its temporary files on exit. Python 3 is required; no Python packages are needed. The live default uses the installed, signed-in official Codex CLI and may consume account usage.

```sh
./demo/run.sh
H2A_BIN="$PWD/bin/h2a" ./demo/run.sh --provider codex --model codex/default
./demo/run.sh --provider antigravity --model antigravity/default
./demo/run.sh --provider gemini
./demo/run.sh --provider claude
./demo/run.sh --provider opencode --model opencode/PROVIDER/MODEL
./demo/run.sh --provider ibm-bob --model ibm-bob/premium --pi-bob-extension "$(npm root -g)/pi-bob"
```

Choose an available model as `--model provider/model-id`. Use `--harness-bin /absolute/path/to/cli` when the selected executable is outside PATH. Gemini, Claude Code, and OpenCode require their own installed and authenticated CLI; see their provider guides for compatibility notes. The Antigravity run requires an installed, signed-in `agy`. The IBM Bob run requires Pi with the `pi-bob` extension and its Bob login; see [IBM Bob setup](ibm-bob.md). IBM Bob has not been tested against a live account here. The stream follows the harness's output behavior; Codex can emit a complete message at once. The demo never prints the gateway secret or passes it as a command argument. Do not run with shell tracing or record other terminals that might expose credentials.

To rehearse without credentials or network access:

```sh
./demo/run.sh --fixture
```

The fixture is a fake Codex protocol executable with a canned response. It exercises the gateway and key lifecycle but is **not evidence of a live provider call**. `H2A_BIN` can also be used in fixture mode. The selected ephemeral port is released before the gateway binds it; if another process takes it, rerun the demo.

## OpenShell sandbox

Follow the [OpenShell setup and request recipe](openshell-live.md) for a macOS/Colima example, including private temporary state, networking, credential substitution, and cleanup. Validate the integration on your host, including streaming and policy denials, before relying on it.

The [provider profile template](../examples/openshell/h2a-provider.yaml) is for your own HTTPS endpoint. Set its hostname, port, and allowed Python executable paths to match your deployment. Start h2a with a dedicated model-scoped key, then import the profile and give OpenShell that gateway key through an environment variable:

```sh
openshell profile lint -f examples/openshell/h2a-provider.yaml
openshell profile import -f examples/openshell/h2a-provider.yaml
# OPENAI_API_KEY must contain your scoped h2a key in this shell.
openshell provider create --name h2a-demo --type h2a-demo --credential OPENAI_API_KEY
unset OPENAI_API_KEY
```

These commands require a working OpenShell gateway. Use a fresh profile ID/provider name for each independent setup. OpenShell 0.1.6 does not accept `--upload` together with a trailing sandbox command: either embed a small client as in the setup example, or upload first and run it with a separate `openshell sandbox exec`.

For a remote sandbox, use an existing reachable HTTPS endpoint; see [remote access](remote-access.md). Sandbox `localhost` refers to the sandbox itself. The profile authorizes the endpoint and client binary, while the client selects the base URL and model. Revoke the dedicated key and remove only your test resources afterward. The calling sandbox's isolation does not extend to the host running the harness.

## Record a real run

```sh
python3 scripts/record-demo.py --output docs/assets/codex-demo --title 'Codex to an API' -- ./demo/run.sh
python3 scripts/record-demo.py --output docs/assets/antigravity-demo --title 'Antigravity to an API' -- ./demo/run.sh --provider antigravity
```

The recorder executes the command and captures actual output timing. It writes a standalone `.html` replay with play/pause, seek, and playback speed, an asciicast `.cast`, and a plain `.txt` transcript. Replay files contain no external dependencies. Gateway token patterns are redacted before saving. Recorded elapsed time describes one run, not a performance benchmark.
