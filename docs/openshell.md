# Where h2a fits with OpenShell

OpenShell provides sandbox isolation, network policy, and provider credential substitution. Its current provider system attaches authorized endpoints to workloads, whose native clients choose the base URL and model. h2a adds an endpoint for a different kind of backend: an authenticated coding harness running through its official CLI.

The integration flow is:

1. An application inside an OpenShell sandbox calls the gateway endpoint (HTTPS for remote use).
2. An OpenShell custom provider profile authorizes that hostname/path and client binary, and can substitute the sandbox's h2a credential placeholder.
3. h2a validates the gateway key and requested model scope, then starts Codex or Antigravity.
4. The official CLI uses its existing login to reach its provider and returns text through h2a.

A real OpenShell 0.1.6 sandbox completed a Codex request through h2a on macOS/Colima. See the [validated local setup](openshell-live.md) and [custom provider profile](../examples/openshell/h2a-provider.yaml). The check covers one non-streaming request; remote HTTPS and sandbox streaming remain unverified.

A custom profile is necessary: the standard OpenAI profile binds credentials to OpenAI's host and should not be repurposed just by changing `OPENAI_BASE_URL`. See [OpenShell's custom endpoint documentation](https://docs.nvidia.com/openshell/how-it-works/inference#use-a-custom-or-self-hosted-endpoint).

h2a's distinct value is the harness-to-HTTP adapter, shared text-completion interface, gateway credentials, and model allowlists. If the workload already calls a supported native inference API, using that API through OpenShell directly is simpler. h2a adds process startup latency and supports only a subset of OpenAI fields.

For strict model restrictions, issue a key for a named model such as `codex/<approved-model-id>`. A `/default` scope permits the CLI's default, which may change with CLI upgrades; it does not pin an underlying model version. Scopes do not govern internal tool or subagent calls.

The calling sandbox's OpenShell policy does not automatically apply to the separate machine executing the harness. Give that host its own isolation and tool policy before allowing untrusted workloads. h2a is not a replacement for OpenShell's security boundary.

Tailscale/Cloudflare solve reachability. OpenShell solves sandbox access control. h2a makes an official harness callable as an API. Keep these responsibilities separate, and use existing infrastructure for the first two.
