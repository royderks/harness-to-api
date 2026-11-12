# E2B sandbox → h2a → host harness

Run a Python HTTP client inside an ephemeral E2B sandbox. The sandbox receives one
scoped gateway key; the actual harness and provider login stay on your host.

Prerequisites: [local gateway and HTTPS route](../README.md#remote-sandboxes),
an E2B account with available quota, and `E2B_API_KEY` configured privately in your
shell. Do not paste the key into a command or commit it.

```sh
python -m pip install e2b==2.10.2
export OPENAI_BASE_URL=https://YOUR-HOSTNAME/v1
export H2A_MODEL=codex/default
# OPENAI_API_KEY: a dedicated h2a key; E2B_API_KEY: your E2B account key.
```

<!-- example-command: e2b -->
```sh
python examples/e2b/main.py
```
<!-- /example-command -->

The script creates a sandbox with a 300-second lifetime, uploads only the
dependency-free `_client.py`, makes one completion request, and kills the sandbox
in `finally`. It does not copy your environment wholesale or forward `E2B_API_KEY`
to the workload. Failed accepted completions still consume h2a quota. Sandbox
compute costs are separate from h2a request limits.

Afterward revoke the dedicated h2a key. If the process is forcibly terminated or
the control API is unavailable, check the E2B dashboard; the sandbox timeout is a
fallback, not proof of immediate cleanup. Cloud execution is **not yet verified**.

[Official E2B quickstart](https://docs.e2b.dev/quickstart).
