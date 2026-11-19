# Docker

The image runs the gateway as UID 10001 and includes OpenAI Codex CLI 0.166.0 and Pi CLI 1.5.0. It also includes `pi-bob` 0.2.6, a **third-party** Pi extension for IBM Bob, not IBM's Bob CLI. Package versions are pinned in the Dockerfile; base image tags and transitive npm dependencies are not digest/lockfile pinned. The image does not contain the Antigravity `agy` CLI: `antigravity`, `gemini`, `claude`, `opencode`, and `all` are unsupported by this image. Those adapters are available with a native h2a installation and the corresponding CLI. The gateway includes the `ibm-bob` adapter.

| Component | Bundled in image | Details |
| --- | --- | --- |
| Build stage | `rust:1.97.1-bookworm` | Compiles `h2a` with `cargo build --locked --release --bin h2a` |
| Runtime base | `node:24.22.2-bookworm-slim` | Runs as non-root `10001:10001` (`h2a:h2a`), `WORKDIR /home/h2a`, `EXPOSE 8787` |
| Gateway binary | `/usr/local/bin/h2a` | Default command `serve --host 0.0.0.0 --port 8787 --provider codex` |
| Supported adapters | `codex`, `ibm-bob` | `@openai/codex@0.166.0`, `@earendil-works/pi-coding-agent@1.5.0`, `pi-bob@0.2.6`, and `/usr/local/bin/pi-bob` |
| Unsupported in image | `antigravity`, `gemini`, `claude`, `opencode`, `all` | Their CLIs are not installed in the container image; use a native `h2a` installation |
| `gateway-state` volume | `/var/lib/h2a` (`HARNESS_API_STATE_DIR`) | Persists gateway key hashes and usage (`keys/` mode `0700`, `<id>.json` mode `0600`) |
| `harness-home` volume | `/home/h2a` (`HOME`) | Persists interactive CLI sign-in and configuration outside the image |

Use a full source checkout; native release archives alone are not a Docker build context. From the repository root, build and create a gateway key (the key is printed once; save it securely):

```sh
docker compose -f deploy/compose.yaml build
docker compose -f deploy/compose.yaml run --rm gateway keys create --name local --models 'codex/*'
```

Sign in to Codex interactively using its device authorization flow. Complete the instructions in the terminal and browser; authentication is stored in the dedicated `harness-home` volume, not the image.

```sh
docker compose -f deploy/compose.yaml run --rm --entrypoint codex gateway login --device-auth
```

Start the service and check it:

```sh
docker compose -f deploy/compose.yaml up -d
docker compose -f deploy/compose.yaml ps
curl http://127.0.0.1:8787/healthz
```

Use `http://127.0.0.1:8787/v1` as the client base URL and the gateway key as its bearer token. The server binds inside the container; Compose publishes only on the host's loopback interface. The `gateway-state` volume retains gateway key hashes and `harness-home` retains CLI configuration and credentials across container rebuilds. Treat both as sensitive, particularly the home volume. `docker compose -f deploy/compose.yaml down` stops the service while retaining them; `down -v` permanently deletes them.

For IBM Bob, create a key scoped to `ibm-bob/*`, sign in with the bundled extension, then override the Compose service command to select it:

```sh
docker compose -f deploy/compose.yaml run --rm gateway keys create --name bob --models 'ibm-bob/*'
docker compose -f deploy/compose.yaml run --rm --entrypoint pi-bob gateway
# In Pi: /login ibm-bob, then follow the prompts and exit.
```

Start the included Bob override with `docker compose -f deploy/compose.yaml -f deploy/compose.bob.yaml up -d`. The login wrapper loads the packaged extension. The gateway loads that same package explicitly and disables other extension discovery. IBM Bob access and its SSO flow require an eligible account; no live login or inference is tested by the image build.

Do not pass provider credentials as build arguments or bake them into the image. Avoid committing credentials or sharing the home volume. No Docker socket, privileged mode, host home directory, or public listener is required. For remote access, use a separately managed authenticated tunnel or private network; this Compose file does not configure a Cloudflare tunnel.

References: [Codex installation](https://github.com/openai/codex#installing-and-running), [Pi installation](https://github.com/earendil-works/pi), [pi-bob package](https://www.npmjs.com/package/pi-bob).

## Validate a deployment

Validate the Compose configuration and run an isolated offline smoke check with a task-owned project name (`-p h2a-smoke`) so existing containers, networks, and volumes on the host remain untouched:

```sh
docker compose -f deploy/compose.yaml config
docker compose -f deploy/compose.yaml -f deploy/compose.bob.yaml config
docker compose -p h2a-smoke -f deploy/compose.yaml build
docker compose -p h2a-smoke -f deploy/compose.yaml run --rm gateway doctor
CODEX_KEY="$(docker compose -p h2a-smoke -f deploy/compose.yaml run --rm gateway keys create --name local --models 'codex/*' --max-requests 2 --ttl 20m --request-timeout 60s --max-concurrent 1 | tr -d '\r\n')"
BOB_KEY="$(docker compose -p h2a-smoke -f deploy/compose.yaml run --rm gateway keys create --name bob --models 'ibm-bob/*' --max-requests 1 --ttl 20m --request-timeout 60s --max-concurrent 1 | tr -d '\r\n')"
docker compose -p h2a-smoke -f deploy/compose.yaml up -d
curl -fsS http://127.0.0.1:8787/healthz
curl -fsS -H "Authorization: Bearer $CODEX_KEY" http://127.0.0.1:8787/v1/models
docker compose -p h2a-smoke -f deploy/compose.yaml down
docker compose -p h2a-smoke -f deploy/compose.yaml up -d
curl -fsS -H "Authorization: Bearer $CODEX_KEY" http://127.0.0.1:8787/v1/models
docker compose -p h2a-smoke -f deploy/compose.yaml -f deploy/compose.bob.yaml up -d
curl -fsS -H "Authorization: Bearer $BOB_KEY" http://127.0.0.1:8787/v1/models
docker compose -p h2a-smoke -f deploy/compose.yaml down -v --rmi local
```

Checked on 2026-11-13 on macOS arm64 (Colima Docker Engine 29.5.2, Docker Compose v5.6.1) from `32587b5`:

- Both `deploy/compose.yaml` and the `deploy/compose.bob.yaml` override rendered valid Compose configurations publishing only `127.0.0.1:8787->8787/tcp`.
- The multi-stage build produced the runtime image running as `uid=10001(h2a) gid=10001(h2a)` with `h2a`, `node v24.22.2`, `codex-cli 0.166.0`, `pi 1.5.0`, and `/usr/local/bin/pi-bob` (`/usr/local/lib/node_modules/pi-bob/extensions/bob.ts`), while `agy`, `gemini`, `claude`, and `opencode` were absent as documented.
- Offline key creation stored key records under `/var/lib/h2a/keys` (`0700` directory, `0600` key JSON owned by `10001:10001`).
- `/healthz` returned `{"status":"ok"}`, unauthenticated `/v1/models` returned HTTP 401, scoped keys returned `codex/default` under the default command and `ibm-bob/default` under `deploy/compose.bob.yaml`, key state survived `down` + `up -d` across container recreation, and `keys revoke` immediately caused `/v1/models` to return HTTP 401.
- Removing the task-owned project (`down -v --rmi local`) cleaned up `h2a-smoke` containers, network, volumes, and local image while preserving all unrelated Docker containers and volumes on the host.

Provider inference remains opt-in: verify interactive CLI login and completion streaming separately on your deployment host. Do not treat successful configuration parsing or an offline container smoke check as proof of live provider authentication or model access.
