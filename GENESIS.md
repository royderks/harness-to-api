# Product intent

harness-to-api exposes installed coding harnesses through an authenticated, OpenAI-compatible text-completion API. The command is `h2a`.

## G1. Make installed coding harnesses usable through an API

Use documented harness CLIs and their normal login mechanisms. Provider credentials remain on the execution host. Clients receive a separate gateway key. IBM Bob uses the community Pi extension, not an official IBM CLI adapter.

## G2. Start from a CLI and work from sandboxes

Start locally with one command and connect existing OpenAI clients or sandbox workloads. Native binary users do not need a Rust toolchain. Network reachability, sandbox isolation, and host-side harness execution have separate owners.

## G3. Make delegated access bounded and explainable

Support key expiry, model scopes, request timeouts, request quotas, rate limits, and concurrency limits. Distinguish configured model names from verified account access. Return explicit errors for unsupported API behavior and policy denials.

## G4. Keep the project simple and demonstrable

Prefer small, clear ownership boundaries. Provide reproducible examples and tests. Distinguish offline protocol fixtures from live-provider requests. Do not add infrastructure without a concrete requirement.

## G5. Make installation straightforward

Use harness-to-api as the project name and `h2a` as the executable. Build native archives with checksums. Keep installation instructions accurate for the distribution channels actually available.

## G6. Preserve intent across agent sessions

Use GitHub Issues for actionable work, pull requests for implementation and review, and repository documents for durable product and architecture guidance. Keep work bounded by the issue's acceptance criteria. Do not generate an unlimited backlog from agent preferences.

## G7. Support a public contributor community

Accept contributions from people and coding agents through GitHub. Reactions communicate demand; maintainers choose priorities. External agents use their operator's compute and credentials. Product direction, access grants, and merges remain maintainer decisions.

## Scope

The gateway supports text completions. It is not a full OpenAI API implementation, billing service, provider-quota oracle, or distributed worker scheduler. See [API compatibility](docs/api.md), [architecture](docs/architecture.md), and [contribution guidelines](CONTRIBUTING.md).
