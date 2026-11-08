# Product intent

This file records Roy Derks' product direction for harness-to-api. It was reconstructed on 2026-11-07 from the project conversation and checked against the repository. It is not a verbatim statement written by Roy. Implementation choices and coordinator defaults belong in [DECISIONS.md](DECISIONS.md).

## G1. Make installed coding harnesses usable through an API

Expose authenticated coding harnesses through an OpenAI-compatible completions endpoint. Start with the official Codex and Antigravity CLIs because Roy has those subscriptions. Add other harnesses through documented SDKs or CLIs. The requested IBM integration uses the community Pi Bob extension and must be described accurately.

A caller receives an h2a gateway credential. Provider credentials remain with the official harness login on the execution host. Do not extract OAuth tokens or depend on private provider endpoints.

## G2. Start from a CLI and work from sandboxes

A developer can start the endpoint locally and call it from another application or sandbox. Installation and onboarding should be short enough to demonstrate in a terminal. Native release users should not need a Go toolchain.

OpenShell is an integration target. Network reachability, sandbox isolation, and harness execution have separate owners. Document how to connect them without implying that an OpenShell sandbox isolates a harness running elsewhere.

## G3. Make delegated access bounded and explainable

A key can expire, restrict requested models, limit request duration, cap accepted requests, and limit concurrent work. Validate what can be established locally. Distinguish configured model names from evidence of account access.

An application must receive explicit errors for unsupported API fields, policy denials, and provider failures. Do not silently discard unsupported constraints or describe missing token usage as zero.

## G4. Keep the project simple and demonstrable

Roy explicitly asked for simple architecture and implementation. Prefer a small number of clear ownership boundaries. Add infrastructure when an observed requirement earns it.

Provide basic documentation and reproducible demonstrations. Record which paths ran against a real provider and which used fixtures. A passing fixture is not a successful subscription request.

## G5. Make distribution and naming deliberate

Roy wants a CLI project that people can discover, install, and share. Research names and distribution options before public launch. The current working name is harness-to-api and the binary is h2a. Reinrelay is a researched recommendation, not an approved rename or registered name.

A viral launch is an aspiration, not a measured outcome. Do not claim novelty over existing gateways without evidence.

## G6. Preserve intent across agent sessions

Roy requested a software-factory foundation based on the source, queue, and worker pattern, using GitHub instead of Linear. Every planned change needs a durable GitHub issue that links back to product intent or an observed defect. An agent must not expand the product by generating an unlimited backlog from its own preferences.

Proceed with authorized reversible work without repeatedly asking for permission. Record choices and results so Roy can inspect or redirect them later. Escalate actual irreversible actions and unresolved product decisions. Keep independent work moving.

## G7. Support a public contributor community

On 2026-11-08, Roy clarified that the repository will be public and requested contributor and maintainer roles, issue voting, and participation by external coding agents. He asked to examine T3 Code and Ghostty as references. Public participation is the intended direction; the earlier private setup was preparation, not the intended long-term access model.

People must be able to express demand and contribute through GitHub. External agents need a documented way to find scoped work and submit evidence. Product direction and access grants remain human decisions. The initial bring-your-own-compute workflow and voting mechanics are implementation choices recorded in DECISIONS.

## Coordinator defaults

The following ordering is an implementation default introduced with the factory foundation. It is not a claim that Roy supplied this exact ranking.

1. Protect credentials and honor declared access limits.
2. Preserve correct, explicit API behavior.
3. Verify behavior through the real calling interface.
4. Keep installation, operation, and code understandable.
5. Add harnesses and distribution channels when the existing behavior is dependable.

The coordinator may choose reversible implementation details consistent with this document. Changing the intended audience, subscription model, public brand, or product commitments requires new direction from Roy. Record that direction separately from agent recommendations.

## Current scope boundary

The first release is a text completion gateway. It is not a full OpenAI API implementation, a hosted billing service, a provider quota oracle, or a distributed worker scheduler. See [API compatibility](docs/api.md), [architecture](docs/architecture.md), and [verification](docs/verification.md) for the current implementation and its limits.
