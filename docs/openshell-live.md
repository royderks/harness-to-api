# OpenShell live integration check

**PASS:** On 2026-11-03, OpenShell 0.1.6 ran a Python 3.13 client in a Docker-backed sandbox. The client sent a non-streaming chat completion through the OpenShell provider proxy to a loopback h2a gateway, which invoked the signed-in local Codex CLI. The response contained `hello` and model `codex/default`. This was a real provider request, not the demo fixture. Baseline: `9d0ece5938105fa7a6d73a033733fb73de97dcde`, with the existing uncommitted CLI and demo edits. The test used the existing `bin/h2a` binary.

The machine was macOS arm64 with Docker 29.5.2 in Colima. The OpenShell CLI and native gateway archives for v0.1.6 came from the [official release](https://github.com/NVIDIA/OpenShell/releases/tag/v0.1.6). Both archive hashes passed `shasum -a 256 -c` against their respective `openshell-checksums-sha256.txt` and `openshell-gateway-checksums-sha256.txt` release assets. No installer or Homebrew service was used.

## Validated setup

The following records the commands and environment used for this check. Paths, ports, and names are specific to this machine; choose unused ports and names for another run. Keep temporary state private and do not enable shell tracing. The unauthenticated HTTP gateway is **only** for an isolated local test: it accepts user API calls without authentication. Do not expose it on a network. The gateway and h2a both listened on `127.0.0.1`.

The native gateway needed the Colima Docker socket and a launch-signing bundle. Its temporary `gateway.toml` was:

```toml
[openshell]
version = 2
[openshell.gateway.auth]
allow_unauthenticated_users = true
[openshell.drivers.docker]
socket_path = "/Users/rd/.colima/default/docker.sock"
sandbox_label = "h2a-test-1103"
```

With `/tmp/h2a-openshell-tools` holding the extracted binaries and private temporary directories, the tested gateway command was:

```sh
umask 077
/tmp/h2a-openshell-tools/openshell-gateway generate-certs --output-dir /tmp/h2a-openshell-tools/tls
XDG_CONFIG_HOME=/tmp/h2a-openshell-tools/config \
XDG_STATE_HOME=/tmp/h2a-openshell-tools/state \
XDG_DATA_HOME=/tmp/h2a-openshell-tools/data \
OPENSHELL_LOCAL_TLS_DIR=/tmp/h2a-openshell-tools/tls \
/tmp/h2a-openshell-tools/openshell-gateway \
  --config /tmp/h2a-openshell-tools/gateway.toml --name h2a-test \
  --bind-address 127.0.0.1 --port 18767 --disable-tls --compute-driver docker --log-level warn
```

In a separate shell, `openshell status` reported `Connected`, version `0.1.6`, with `OPENSHELL_GATEWAY_ENDPOINT=http://127.0.0.1:18767` and the same temporary XDG directories. Colima's VM has its own loopback. The supervisor initially failed to reach the Mac gateway; this task-owned SSH process forwarded both ports to VM loopback, without a public listener:

```sh
ssh -F "$HOME/.colima/_lima/colima/ssh.config" \
  -o ControlMaster=no -o ControlPath=none -o ExitOnForwardFailure=yes -N \
  -R 127.0.0.1:18767:127.0.0.1:18767 \
  -R 127.0.0.1:18768:127.0.0.1:18768 lima-colima
```

The profile used [h2a-provider.yaml](../examples/openshell/h2a-provider.yaml), with its ID set to `h2a-test-1103`, host to `host.openshell.internal`, and port to `18768`. The gateway verified it and accepted the separate scoped key:

```sh
openshell profile lint -f /tmp/h2a-openshell-tools/profile.yaml
openshell profile import -f /tmp/h2a-openshell-tools/profile.yaml
export OPENAI_API_KEY="$(bin/h2a keys create --state-dir /tmp/h2a-openshell-tools/h2a-state --name openshell-test-2 --models codex/default)"
openshell provider create --name h2a-test-1103 --type h2a-test-1103 --credential OPENAI_API_KEY
unset OPENAI_API_KEY
bin/h2a serve --state-dir /tmp/h2a-openshell-tools/h2a-state --provider codex --host 127.0.0.1 --port 18768
```

The last command runs in its own shell. The key creation prints the key once on stdout and its ID on stderr; capture the key privately and save its ID for revocation. `--credential OPENAI_API_KEY` reads the environment instead of placing the key in the argument list. The sandbox receives an opaque placeholder; OpenShell substitutes the real key for requests to the profile-authorized endpoint. The equivalent task command, run with the same OpenShell endpoint and XDG environment, was:

```sh
openshell sandbox create --name h2a-test-1103 --from python:3.13-slim \
  --provider h2a-test-1103 \
  --env OPENAI_BASE_URL=http://host.openshell.internal:18768/v1 \
  --env H2A_MODEL=codex/default --no-tty -- \
  python3 -c 'import os, json, urllib.request; req=urllib.request.Request(os.environ["OPENAI_BASE_URL"]+"/chat/completions", data=json.dumps({"model":os.environ["H2A_MODEL"],"messages":[{"role":"user","content":"Reply with hello"}]}).encode(), headers={"Content-Type":"application/json","Authorization":"Bearer "+os.environ["OPENAI_API_KEY"]}); print(urllib.request.urlopen(req,timeout=150).read().decode())'
```

It exited zero with assistant content `hello`. The effective policy listed the profile endpoint and allowed binaries. This test establishes the single non-streaming request path on this specific Colima setup; it does not validate streaming in OpenShell, other runtimes, or a remotely exposed h2a gateway.

## Pitfalls and cleanup

- OpenShell 0.1.6 uses `openshell profile lint` and `openshell profile import`, not `openshell provider profile ...`. Lint requires a reachable gateway and rejects an ID already imported. To change an existing profile, export it with its resource version and use `profile update`, or delete and reimport it.
- `--upload` cannot be combined with a trailing command in this CLI. Create the sandbox with the upload, then invoke a separate `openshell sandbox exec`. The command above instead embeds a small client and needs no upload.
- Match the actual interpreter executable in the profile. The Python image resolves `python3` to `/usr/local/bin/python3.13`; without the versioned pattern, network access failed with `PermissionError`. Narrow the template patterns to the executables in your image.
- `host.openshell.internal` names a host-side route; sandbox `localhost` is not the host. The Colima reverse forwards were necessary for this loopback-bound native gateway. This HTTP path is a local experiment, not a substitute for authenticated, encrypted transport on an untrusted network. OpenShell requires both network policy and endpoint-bound credentials; see NVIDIA's [inference documentation](https://docs.nvidia.com/openshell/how-it-works/inference).
- The OpenShell runtime image is not necessarily a workload image. Using `ghcr.io/nvidia/openshell/sandbox:0.1.6` as the workload failed identity resolution; `python:3.13-slim` worked.

After the test, `openshell sandbox delete h2a-test-1103`, `openshell provider delete h2a-test-1103`, and `openshell profile delete h2a-test-1103` removed the task resources. The gateway, h2a, and SSH forwarding processes were stopped; the isolated state, keys and generated private material were deleted, and the task-pulled Python image was removed. `openshell sandbox list` returned no sandboxes and the task-labeled Docker containers were gone. The two unrelated Docker containers remained running. For a persistent h2a state directory, revoke the saved key ID with `h2a keys revoke --state-dir PATH KEY_ID` before removing the state.
