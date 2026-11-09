# Remote access

Run the harness and gateway on a machine you control. The sandbox is an HTTP client; it does not need Rust, the harness CLI, or the harness's login credentials. The host must stay online for requests to work.

## Private clients: Tailscale Serve

Choose this when both host and clients can join your tailnet. Install and authenticate Tailscale on the host and the client, enable HTTPS as directed by Tailscale, then run:

```sh
h2a serve --provider codex
tailscale serve http://127.0.0.1:8787
```

These are separate foreground processes/terminals. Use the HTTPS address printed by Tailscale, plus `/v1`, as the client base URL. The API key is still required. Tailnet grants should restrict who can reach the host.

For containers without a TUN device, Tailscale offers userspace networking through a SOCKS5/HTTP proxy. This requires explicit proxy support/configuration in the client; installing Tailscale alone does not automatically route that client's traffic. See [Serve](https://tailscale.com/docs/reference/tailscale-cli/serve) and [userspace networking](https://tailscale.com/docs/concepts/userspace-networking).

## Hosted sandboxes: Tailscale Funnel

Choose this for a first demo when the sandbox cannot install Tailscale. Only the host needs Tailscale:

```sh
tailscale funnel http://127.0.0.1:8787
```

Funnel exposes a public HTTPS address. Follow Tailscale's prompts to enable the required tailnet policy. The sandbox calls it over normal HTTPS using its gateway key. Stopping the foreground command stops that tunnel. Funnel has product limits and is not the default recommendation for high-volume hosting. See [Funnel](https://tailscale.com/docs/features/tailscale-funnel).

## Stable hostname: named Cloudflare Tunnel

For a service intended to stay online, create a named tunnel and route a hostname you control to `http://127.0.0.1:8787`. A locally managed ingress configuration looks like:

```yaml
tunnel: YOUR_TUNNEL_ID
credentials-file: /absolute/path/to/tunnel-credentials.json
ingress:
  - hostname: api.example.com
    service: http://127.0.0.1:8787
    originRequest:
      disableChunkedEncoding: false
  - service: http_status:404
```

Provision the tunnel and DNS using [Cloudflare's official setup](https://developers.cloudflare.com/tunnel/get-started/). Keep tunnel credentials on the host. Cloudflare Access service tokens can add another boundary; configure the corresponding headers in your client if enabled. h2a's Bearer key remains required.

Avoid temporary `trycloudflare.com` Quick Tunnels for this project: [Cloudflare documents that they do not support SSE](https://developers.cloudflare.com/tunnel/get-started/quick-tunnels/). Named tunnels support streaming; verify flush behavior and request-duration limits on the deployed route.

ngrok or an SSH reverse tunnel to a TLS reverse proxy are also possible. The required contract is HTTPS, preserved Authorization headers, unbuffered SSE, and a timeout long enough for a harness turn. h2a does not depend on a particular tunnel provider.

## Configure a sandbox

Create a separate scoped key on the host:

```sh
h2a keys create --name sandbox-42 --models 'codex/default'
```

Put the key in the sandbox provider's secret facility and set:

```text
OPENAI_BASE_URL=https://YOUR-HOSTNAME/v1
OPENAI_API_KEY=<the gateway key>
```

Test model discovery with `GET /v1/models`, then a streaming chat request. Confirm unauthenticated calls return 401. Never send the host's Codex/Google credentials to the sandbox. Revoke the sandbox key when the job ends.

The gateway currently trusts authenticated callers to use the host's harness. For untrusted workloads, run it under a dedicated account or within a separately isolated machine, and audit the harness's tool policy. A tunnel secures transport; it does not isolate agent execution.

## Check the route

Verify unauthenticated rejection, authenticated model listing, incremental SSE delivery, request timeout, and revocation through your deployed endpoint. Local HTTP tests do not validate a remote tunnel's buffering or timeouts.
