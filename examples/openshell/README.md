# OpenShell inference provider

OpenShell routes sandbox HTTP requests to h2a and substitutes an endpoint-bound
gateway credential. Harness login stays on the host. The sandbox receives an
opaque credential placeholder rather than the real gateway key.

Prerequisites: OpenShell 0.1.6 on PATH, a configured authenticated OpenShell
gateway with Docker available, an h2a HTTPS endpoint, and a dedicated bounded key.
No separate paid sandbox-service account is required for a local installation.

Follow [gateway setup](../README.md#remote-sandboxes), then set your existing
OpenShell gateway connection using its normal authenticated CLI configuration:

```sh
export OPENSHELL_GATEWAY_ENDPOINT=https://YOUR-OPENSHELL-GATEWAY
export OPENAI_BASE_URL=https://YOUR-H2A-HOST/v1
export H2A_MODEL=codex/default
```

`OPENAI_API_KEY` must contain the scoped h2a key. It is passed to OpenShell by
environment-variable name, never expanded into command arguments.

<!-- example-command: openshell -->
```sh
python examples/openshell/main.py
```
<!-- /example-command -->

The script creates a temporary profile from [h2a-provider.yaml](h2a-provider.yaml),
sets its endpoint, and creates a provider and Python sandbox with unique names.
The client makes one completion request. Cleanup attempts deletion of all three
resources, including after a failure, and a cleanup failure fails the example.
A forced kill or interrupted gateway can leave resources named `h2a-live-*`;
inspect and remove only the resources from your run. Revoke the dedicated key.

This live route is **not yet verified**. [Verification instructions](../../docs/examples-testing.md)
produce dated evidence after an actual run. The manual workflow for a
Colima-based local gateway is documented [separately](../../docs/openshell-live.md).
The automated cloud workflow does not provision an OpenShell gateway or run this
example; run it on your trusted configured host.

See [NVIDIA inference documentation](https://docs.nvidia.com/openshell/how-it-works/inference).
