# OpenShell with h2a as its provider

Run a Python client in a local OpenShell sandbox. OpenShell sends its requests to
h2a; h2a runs your installed harness on the host. The sandbox receives an opaque
credential placeholder. OpenShell substitutes the scoped h2a key only at the
endpoint allowed by the provider profile. Harness login stays on the host.

**No OpenShell account, NVIDIA API key, E2B account or Modal account is needed.**
You need OpenShell with a working local runtime, Python 3.11+, h2a, and an
installed/logged-in harness for real inference. This example uses Codex; select
another enabled harness by changing the provider and model together.

## 1. Start OpenShell

Follow NVIDIA's [local installation instructions](https://docs.nvidia.com/openshell/v0.1.8/about/installation).
The official installer sets up the CLI and local gateway. Verify the gateway's
connection and authentication in `openshell status`. OpenShell 0.1.8 was tested
with Docker 29.5.2 on Colima; a Docker runtime is local compute, not a paid service.
Do not replace an existing gateway configuration to run this example.

## 2. Start a bounded h2a gateway

From the repository root, in terminal 1:

```sh
umask 077
export H2A_EXAMPLE_STATE="$(mktemp -d)"
h2a keys create --state-dir "$H2A_EXAMPLE_STATE" \
  --name openshell --models codex/default --ttl 15m \
  --max-requests 1 --request-timeout 90s --max-concurrent 1 \
  > "$H2A_EXAMPLE_STATE/client.key"
printf 'Key file: %s/client.key\n' "$H2A_EXAMPLE_STATE"
h2a serve --state-dir "$H2A_EXAMPLE_STATE" --provider codex --host 127.0.0.1 --port 8787
```

Use your installed `h2a`, or put the development binary on PATH. Save the printed
key ID for revocation. The key is written to a private file; do not paste it into
commands or source files.

## 3. Run the sandbox

In terminal 2, set the key-file path printed above:

```sh
export OPENAI_API_KEY="$(cat /PATH/PRINTED/ABOVE/client.key)"
export OPENAI_BASE_URL=http://host.openshell.internal:8787/v1
export H2A_MODEL=codex/default
```

`host.openshell.internal` is OpenShell's host route, not sandbox localhost. On a
Docker VM such as Colima, VM loopback differs from macOS loopback. If that route
cannot reach h2a, keep this forward running in another terminal on macOS:

```sh
ssh -F "$HOME/.colima/_lima/colima/ssh.config" \
  -o ControlMaster=no -o ControlPath=none -o ExitOnForwardFailure=yes -N \
  -R 127.0.0.1:8787:127.0.0.1:8787 lima-colima
```

This forwards the quickstart's h2a port 8787 to Colima VM loopback. If you change
h2a's port, change both forwarding ports and `OPENAI_BASE_URL` together. If the
sandbox supervisor also cannot reach your host-side OpenShell gateway, add a
second `-R` before `lima-colima` for that gateway's actual port. For the default
installer gateway on port 17670, use `-R 127.0.0.1:17670:127.0.0.1:17670`.
Keep its existing TLS and authentication settings; do not replace its configuration.
If a forward reports a port already in use, inspect the existing listener rather
than stopping an unrelated service.

The [advanced isolated-gateway recipe](../../docs/openshell-live.md#configure-a-local-gateway)
uses separate example ports 18767 and 18768; those are not this quickstart's ports.
Keep all forwards private; do not expose the gateway to your LAN.

<!-- example-command: openshell-1 -->
```sh
python examples/openshell/main.py --local
```
<!-- /example-command -->

`--local` permits HTTP only for `host.openshell.internal`. Use it only with a
trusted local OpenShell gateway/runtime. The script creates a temporary provider
profile, provider and Python sandbox, makes one completion request, verifies
nonempty text, and deletes its resources. It prints confirmation after cleanup.
A second run with this single-request key fails with quota denial. Revoke it with
`h2a keys revoke --state-dir /PATH/TO/STATE KEY_ID`, then stop terminal 1 and
remove only your temporary state directory. Unset `OPENAI_API_KEY` in terminal 2.

## Remote h2a endpoint

Keep OpenShell's normal authenticated gateway configuration. Set
`OPENAI_BASE_URL=https://YOUR-H2A-HOST/v1` and provide a dedicated scoped key.
Run without the local flag:

<!-- example-command: openshell-2 -->
```sh
python examples/openshell/main.py
```
<!-- /example-command -->

Remote h2a endpoints require HTTPS. Credentials are passed to the OpenShell CLI
by environment-variable name, never expanded into process arguments.

## Verification and cleanup

On 2026-11-12, real OpenShell 0.1.8 sandboxes on macOS/Colima reached a local
fixture-backed h2a: completion, HTTP 429 after one accepted request, and HTTP 401
after revocation passed. This is real sandbox/networking evidence with canned
harness output, **not live Codex inference**. See [verification](../../docs/openshell-live.md#verification).

Before creating resources, the script prints `OpenShell resource name: h2a-...`
to stderr. Save that name: the profile, provider and sandbox all use it. Normal
cleanup deletes only this run's resources. A forced kill or unavailable gateway
can leave resources. After checking the printed name against your gateway's
inventory, remove only those leftovers:

<!-- example-command: openshell-3 -->
```sh
openshell sandbox delete NAME_PRINTED_BY_THIS_RUN
openshell provider delete NAME_PRINTED_BY_THIS_RUN
openshell profile delete NAME_PRINTED_BY_THIS_RUN
```
<!-- /example-command -->

An already-removed resource may report not found. Never prune unrelated Docker
resources. Stop any SSH forward you started when finished.
The [opt-in live runner](../../docs/examples-testing.md) supports the HTTPS route;
its hosted workflow does not provision an OpenShell gateway.
