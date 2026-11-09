# Connect an OpenShell sandbox

This recipe connects a Python client in an OpenShell sandbox to a local Codex-backed gateway. The example uses OpenShell 0.1.6, Python 3.13, and Colima on macOS. Validate the request path on your own host, including streaming if your client uses it. Download the CLI and gateway from the [official release](https://github.com/NVIDIA/OpenShell/releases/tag/v0.1.6) and verify the published checksums.

## Configure a local gateway

The commands below use example paths, ports, and names. Create private temporary directories and choose unused ports for your run. Keep temporary state private and do not enable shell tracing. The unauthenticated HTTP gateway is **only** for an isolated local test: it accepts user API calls without authentication. Do not expose it on a network. Bind both OpenShell and h2a to `127.0.0.1`.

Configure the native gateway with the Colima Docker socket and a launch-signing bundle. Use a temporary `gateway.toml`:

```toml
[openshell]
version = 2
[openshell.gateway.auth]
allow_unauthenticated_users = true
[openshell.drivers.docker]
socket_path = "/Users/YOUR_USER/.colima/default/docker.sock"
sandbox_label = "h2a-example"
```

With `/tmp/h2a-openshell-tools` holding the extracted binaries and private temporary directories, start the gateway:

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

In a separate shell, set `OPENSHELL_GATEWAY_ENDPOINT=http://127.0.0.1:18767` and the same temporary XDG directories, then use `openshell status` to check the connection. Colima's VM has its own loopback. Forward both ports to VM loopback when the Colima supervisor cannot reach the host gateway:

```sh
ssh -F "$HOME/.colima/_lima/colima/ssh.config" \
  -o ControlMaster=no -o ControlPath=none -o ExitOnForwardFailure=yes -N \
  -R 127.0.0.1:18767:127.0.0.1:18767 \
  -R 127.0.0.1:18768:127.0.0.1:18768 lima-colima
```

Use [h2a-provider.yaml](../examples/openshell/h2a-provider.yaml), with its ID set to `h2a-example`, host to `host.openshell.internal`, and port to `18768`. Lint and import the profile, then provide a separate scoped key:

```sh
openshell profile lint -f /tmp/h2a-openshell-tools/profile.yaml
openshell profile import -f /tmp/h2a-openshell-tools/profile.yaml
export OPENAI_API_KEY="$(bin/h2a keys create --state-dir /tmp/h2a-openshell-tools/h2a-state --name openshell-client --models codex/default)"
openshell provider create --name h2a-example --type h2a-example --credential OPENAI_API_KEY
unset OPENAI_API_KEY
bin/h2a serve --state-dir /tmp/h2a-openshell-tools/h2a-state --provider codex --host 127.0.0.1 --port 18768
```

The last command runs in its own shell. The key creation prints the key once on stdout and its ID on stderr; capture the key privately and save its ID for revocation. `--credential OPENAI_API_KEY` reads the environment instead of placing the key in the argument list. The sandbox receives an opaque placeholder; OpenShell substitutes the real key for requests to the profile-authorized endpoint. Run the sandbox client with the same OpenShell endpoint and XDG environment:

```sh
openshell sandbox create --name h2a-example --from python:3.13-slim \
  --provider h2a-example \
  --env OPENAI_BASE_URL=http://host.openshell.internal:18768/v1 \
  --env H2A_MODEL=codex/default --no-tty -- \
  python3 -c 'import os, json, urllib.request; req=urllib.request.Request(os.environ["OPENAI_BASE_URL"]+"/chat/completions", data=json.dumps({"model":os.environ["H2A_MODEL"],"messages":[{"role":"user","content":"Reply with hello"}]}).encode(), headers={"Content-Type":"application/json","Authorization":"Bearer "+os.environ["OPENAI_API_KEY"]}); print(urllib.request.urlopen(req,timeout=150).read().decode())'
```

Check for an HTTP success response containing an assistant message. Also verify that a missing or revoked key is rejected before using the integration for real workloads.

## Pitfalls and cleanup

- OpenShell 0.1.6 uses `openshell profile lint` and `openshell profile import`, not `openshell provider profile ...`. Lint requires a reachable gateway and rejects an ID already imported. To change an existing profile, export it with its resource version and use `profile update`, or delete and reimport it.
- `--upload` cannot be combined with a trailing command in this CLI. Create the sandbox with the upload, then invoke a separate `openshell sandbox exec`. The command above instead embeds a small client and needs no upload.
- Match the actual interpreter executable in the profile. The Python image resolves `python3` to `/usr/local/bin/python3.13`; without the versioned pattern, network access failed with `PermissionError`. Narrow the template patterns to the executables in your image.
- `host.openshell.internal` names a host-side route; sandbox `localhost` is not the host. The Colima reverse forwards were necessary for this loopback-bound native gateway. This HTTP path is a local experiment, not a substitute for authenticated, encrypted transport on an untrusted network. OpenShell requires both network policy and endpoint-bound credentials; see NVIDIA's [inference documentation](https://docs.nvidia.com/openshell/how-it-works/inference).
- The OpenShell runtime image is not necessarily a workload image. Using `ghcr.io/nvidia/openshell/sandbox:0.1.6` as the workload failed identity resolution; `python:3.13-slim` worked.

After use, delete the resources you created:

```sh
openshell sandbox delete h2a-example
openshell provider delete h2a-example
openshell profile delete h2a-example
```

Stop the gateway, h2a, and SSH forwarding processes. Remove their private temporary state. For a persistent h2a state directory, revoke the saved key ID with `h2a keys revoke --state-dir PATH KEY_ID`. Leave unrelated Docker resources untouched.
