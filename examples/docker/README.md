# Docker client

Run the HTTP client in a Python container while h2a and the harness stay on the
host. No Python packages, Rust, or harness login are installed in the container.

Follow [gateway and HTTPS setup](../README.md#remote-sandboxes), install/start
Docker, and run:

```sh
export OPENAI_BASE_URL=https://YOUR-HOSTNAME/v1
export H2A_MODEL=codex/default
python examples/docker/main.py
```

`OPENAI_API_KEY` is inherited by variable name, never expanded into Docker command
arguments. Docker administrators can inspect container environment values. Only
the client source is mounted, read-only. The container has a read-only root and
dropped capabilities. One accepted request, no separate sandbox-service account.

The script removes only its uniquely named container, including on normal failure
or Ctrl+C. After a forced termination inspect `docker ps -a --filter name=h2a-example-`
and remove only the task's container. Revoke the dedicated h2a key. The public
Python base image remains in Docker's cache. HTTPS is used to avoid platform-specific
container-to-host routing and exposing the local gateway on all interfaces.
The command and cleanup contract is unit-tested; the real container-to-HTTPS
request remains unverified. See [evidence](../VERIFICATION.md).
