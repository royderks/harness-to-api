# OpenShell inference provider

OpenShell routes sandbox HTTP requests to h2a and substitutes an endpoint-bound
gateway credential. h2a then invokes the harness on the host. This is different
from simply putting the real gateway key in the sandbox environment.

Prerequisites: a configured OpenShell gateway and Docker runtime, an h2a HTTPS
endpoint, and a dedicated bounded h2a key in `OPENAI_API_KEY`. No separate paid
sandbox-service account is required for a local OpenShell installation.

1. Copy [h2a-provider.yaml](h2a-provider.yaml) to a private temporary path. Replace
   `h2a.example.com` with your endpoint hostname. The commands below use ID `h2a-demo`;
   choose an unused ID/name and update all commands if it already exists.
2. Register the profile and supply the credential by environment-variable name:

```sh
openshell profile lint -f /PRIVATE/PATH/h2a-provider.yaml
openshell profile import -f /PRIVATE/PATH/h2a-provider.yaml
openshell provider create --name h2a-demo --type h2a-demo --credential OPENAI_API_KEY
```

3. Start one sandbox client. The argument contains source code, not credentials:

```sh
openshell sandbox create --name h2a-demo --from python:3.13-slim \
  --provider h2a-demo \
  --env OPENAI_BASE_URL=https://YOUR-HOSTNAME/v1 \
  --env H2A_MODEL=codex/default --no-tty -- \
  python3 -c "$(cat examples/_client.py)"
```

4. Clean up the resources you created and revoke the dedicated h2a key:

```sh
openshell sandbox delete h2a-demo
openshell provider delete h2a-demo
openshell profile delete h2a-demo
```

One accepted request. These commands target OpenShell 0.1.6. Cloud/sandbox live
execution of this example is **not yet verified**. The [local routing recipe](../../docs/openshell-live.md)
covers Colima and loopback routing; do not treat that specialized recipe as a
production gateway configuration. [NVIDIA inference documentation](https://docs.nvidia.com/openshell/how-it-works/inference).
