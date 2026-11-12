# Modal sandbox → h2a → host harness

Run the same dependency-free HTTP client in a Modal sandbox. No harness installation
or provider login is copied into the container.

Prerequisites: [gateway and HTTPS route](../README.md#remote-sandboxes), a Modal
account with available quota, and local Modal authentication:

```sh
python -m pip install -r examples/requirements.txt
modal setup
export OPENAI_BASE_URL=https://YOUR-HOSTNAME/v1
export H2A_MODEL=codex/default
# OPENAI_API_KEY must contain a dedicated h2a key.
```

<!-- example-command: modal -->
```sh
python examples/modal/main.py
```
<!-- /example-command -->

The script uses an ephemeral app, a Python 3.13 image, and `Secret.from_dict` to
inject only the gateway configuration. This is ordinary environment injection,
not Modal's separate network-level secret-injection feature. One completion
request, a 300-second sandbox timeout, and `terminate()` in `finally`. Image build
and sandbox compute may consume Modal credits; h2a does not cap those costs.

Revoke the dedicated h2a key after use. Check the Modal dashboard if local execution
is force-killed or cleanup fails. Cloud execution is **not yet verified**.

[Sandbox API](https://modal.com/docs/guide/sandboxes) · [Secret.from_dict](https://modal.com/docs/guide/secrets).
