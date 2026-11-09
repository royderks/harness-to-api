# Docker

The image runs the gateway as UID 10001 and includes OpenAI Codex CLI 0.166.0 and Pi CLI 1.5.0. It also includes `pi-bob` 0.2.6, a **third-party** Pi extension for IBM Bob, not IBM's Bob CLI. Package versions are pinned in the Dockerfile; base image tags and transitive npm dependencies are not digest/lockfile pinned. The image does not contain the Antigravity `agy` CLI: `antigravity`, `gemini`, `claude`, `opencode`, and `all` are unsupported by this image. Those adapters are available with a native h2a installation and the corresponding CLI. The gateway includes the `ibm-bob` adapter.

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

Validate the Compose configuration with `docker compose -f deploy/compose.yaml config`. The image recipe requires an end-to-end check on your Docker host. Verify login, health, completion streaming, restart persistence, and revocation. Do not treat successful configuration parsing as a successful image build or provider request.
