# Decision record

Recorded on 2026-11-07. Earlier decisions are reconstructed from the project conversation, committed code, and verification records. The recording date does not imply that every choice was made today. No agent choice below is attributed to Roy unless the conversation contains that direction.

Statuses mean implemented, operational default, proposed, or deferred. Supersede a decision with a new entry and link both entries. Do not rewrite history to make an earlier choice appear inevitable. This is a compact index of material decisions, not a copy of every tool call.

## Product and runtime

| ID | Decision and reason | Authority and status | Evidence |
| --- | --- | --- | --- |
| D001 | Expose harness access through authenticated OpenAI-compatible completions. Existing clients should be able to call installed harnesses. | Roy request; implemented. | [G1](GENESIS.md#g1-make-installed-coding-harnesses-usable-through-an-api), commit `9d0ece5`. |
| D002 | Use a native Go CLI and the standard library. Keep gateway installation independent of language runtimes and avoid a framework dependency. Rust and a runtime-based prototype were considered; Go is the selected implementation, not a claim of a measured performance win. | Agent language choice; implemented. | [go.mod](go.mod), [distribution](docs/distribution.md), commit `9d0ece5`. |
| D003 | Use official harness CLI protocols and normal logins. Do not extract credentials or call private endpoints. | Roy requested official SDKs or CLIs; implemented. | [adapters](internal/harness/harness.go), [README](README.md). |
| D004 | Begin with Codex and Antigravity. Their real calls were verified. Add Gemini, Claude Code, OpenCode, and IBM Bob without overstating their verification. | Roy request and subsequent authorization; implemented with verification gaps. | Commits `9d0ece5`, `bd1dfd6`, `f63f319`; [verification](docs/verification.md). |
| D005 | IBM Bob uses official Pi with the specifically requested community pi-bob extension. Do not call this an official IBM CLI adapter. | Roy supplied the package; implemented. | [IBM Bob](docs/ibm-bob.md). |
| D006 | Support chat completions, legacy string completions, model listing, and SSE for text. Reject unsupported fields instead of silently ignoring them. | Agent compatibility boundary; implemented. | [API contract](docs/api.md), [request parser](internal/api/request.go). |
| D007 | Serialize supplied conversation messages into a fresh prompt. Do not claim provider-native role precedence, persistent sessions, tool calling, or full Responses API compatibility. | Agent scope choice; implemented. | [API contract](docs/api.md). |
| D008 | Each adapter reports only assistant text and observed usage. Missing usage remains unknown. Success requires the harness's terminal success event. | Agent correctness choice; implemented. | [adapter contract](internal/harness/harness.go), protocol fixtures. |

## Credentials and admission

| ID | Decision and reason | Authority and status | Evidence |
| --- | --- | --- | --- |
| D009 | Store a hash of each randomly generated gateway key. Print the secret once. Keep it out of subprocess arguments, diagnostics, recordings, and normal harness environments. | Roy requested API-key security; implemented. | [key store](internal/auth/keys.go), [process runner](internal/harness/process.go). |
| D010 | Add expiry, per-request timeout, accepted-request quota, rolling-minute rate, and per-key concurrency. Existing keys remain unlimited unless replaced. | Roy requested limits and approved the first-release proposal; implemented. | Commit `126f747`; [policies](docs/policies.md). |
| D011 | Debit accepted requests durably before starting the harness. Failures and cancellation do not refund a debit. Invalid requests and denied admissions do not debit. A crash between debit and execution can conservatively consume one allowance. | Agent accounting choice; implemented. | [admission](internal/auth/admission.go), [end-to-end check](scripts/check-policies.py). |
| D012 | Keep policy and usage in one key record and one admission owner. Allow one server per local state directory. This avoids split accounting ownership and distributed coordination. | Agent synthesis after two designs; implemented. | [architecture synthesis](docs/architecture.md#synthesis-decision), commit `126f747`. |
| D013 | Apply the earliest caller, server, key-duration, or key-expiry deadline. Observe revocation through polling and cancel active managed runs. Do not promise instantaneous termination or an upstream refund. | Agent operational choice; implemented. | [admission](internal/auth/admission.go), [platform limits](docs/architecture.md#operational-limits). |
| D014 | Validate configured request model names separately from account access. An explicit probe uses the ordinary authenticated HTTP path and consumes quota when admitted. | Roy requested model limits and validation; implemented. | [model rules](internal/api/models.go), [probe](cmd/h2a/probe.go). |
| D015 | Reject dynamic default names in a strict server catalog. Provider-defined aliases can still change upstream. Do not pretend a model string pins an immutable model version. | Agent scope choice; implemented. | [policies](docs/policies.md#validate-models). |
| D016 | Keep dollar budgets, exact upstream token caps, hierarchical child keys, rotation, active-run administration, and automatic fallback out of this first release. Their semantics need separate work. | Agent sequencing choice; deferred. | Prior feature proposal; [policy limits](docs/policies.md#operate-locally). |

## Deployment, proof, and distribution

| ID | Decision and reason | Authority and status | Evidence |
| --- | --- | --- | --- |
| D017 | Start with a local binary. Use existing Tailscale or a named Cloudflare Tunnel for remote access; consider a Linux VM with Compose for continuous operation. Tunnel ingress is not harness execution. | Roy requested remote and deployment options; researched recommendation. | [deployment](docs/deployment.md), [remote access](docs/remote-access.md). |
| D018 | Keep h2a and OpenShell responsibilities separate. OpenShell authorizes the endpoint and substitutes a gateway credential; h2a enforces its key policy and invokes the harness. | Agent integration choice; one live path verified. | [OpenShell setup](docs/openshell-live.md), [integration boundary](docs/openshell.md). |
| D019 | Treat Cloudflare and Vercel deployment as researched recipes, not deployed services. The Docker image build was blocked by local Docker storage errors. Do not reset unrelated containers to fix a demo. | Agent operational choice; deployment remains unverified. | [Docker](docs/docker.md), [deployment](docs/deployment.md). |
| D020 | Build macOS, Linux, and Windows archives for amd64 and arm64. Cross-compilation is not native execution. At the original baseline, native storage tests were unverified. Hosted Go tests later passed on Windows; descendant process-tree cleanup remains a separate gap. | Agent distribution choice; builds verified. | [verification](docs/verification.md), [release script](scripts/release.sh). |
| D021 | Keep h2a as the working name. Reinrelay is a recommendation only. Registry search results do not prove trademark or name ownership. | Agent research; rename proposed, not approved. | [naming research](docs/naming.md). |
| D022 | Keep live inference opt-in. Normal CI uses deterministic fixtures and tests the CLI, HTTP, cancellation, and accounting. Record failures and unavailable credentials without substituting a fixture pass. | Agent verification choice; implemented. | [CI](.github/workflows/ci.yml), [verification](docs/verification.md). |
| D023 | Avoid mandatory manual recovery after every crash. Preserve durable quotas, but document that a force-killed server can leave orphaned processes. | Agent tradeoff from architecture review; implemented. | [architecture tradeoffs](docs/architecture.md#tradeoffs-accepted). |

## Factory defaults

| ID | Decision and reason | Authority and status | Evidence |
| --- | --- | --- | --- |
| D024 | Use GitHub Issues as the durable work queue. GitHub Projects is an optional view, not another source of status. | Roy's 2026-11-07 instruction; operational default. | [G6](GENESIS.md#g6-preserve-intent-across-agent-sessions). |
| D025 | Keep GENESIS for reconstructed human intent, DECISIONS for reasoned choices, and GitHub issues for actionable work. Link work to intent or observed defects. | Agent adaptation of the referenced article; operational default. | [Factory guide](docs/factory.md). |
| D026 | One coordinator triages and claims work. Start with at most two implementation workers, each on a separate branch or worktree. Pause dispatch on unavailable quota rather than inventing subscription capacity. | Agent conservative default; configurable later. | [Factory guide](docs/factory.md). |
| D027 | Require independent review and passing checks on the current PR head before a human merges. Bootstrap does not launch workers, enable auto-merge, schedule jobs, or publish releases. | Agent first-release operational default; not a retrospective approval requirement. | [Factory guide](docs/factory.md). |
| D028 | Create a private repository using the existing project name. This preserves optional public naming and launch decisions. | Agent reversible setup default; long-term private scope superseded by D034. | [Decision trail](docs/factory/decisions.tsv). |
| D029 | Import a small seed backlog once. Preserve edited and closed GitHub issues on repeat bootstrap. Do not keep a competing local status board. | Agent idempotence choice; operational default. | [Factory guide](docs/factory.md). |
| D030 | Keep a bounded handoff and feedback loop. Record evidence, exact commits, residual risks, next work, and decisions in durable artifacts. No claim of a running unattended factory until a scheduler and worker adapter are explicitly implemented and verified. | Agent scope choice; operational default. | [Factory guide](docs/factory.md). |

## Foundation implementation decisions

| ID | Decision and reason | Authority and status | Evidence |
| --- | --- | --- | --- |
| D031 | Use a local Python and GitHub CLI bootstrap rather than a write-capable Actions seeder and remote ledger issue. Keep gateway runtime dependencies unchanged. | Agent design comparison; implemented. | [Factory design rationale](docs/factory.md#why-this-foundation-stays-local). |
| D032 | Remember published seed IDs in a private local journal. Record an attempt before issue creation and stop on an ambiguous response. This preserves human edits without claiming distributed exactly-once delivery. | Agent synthesis; implemented. | [Recovery contract](docs/factory.md#recover-an-interrupted-publication). |
| D033 | Correct stale README and distribution claims about expiry, rate limits, and active-run revocation while establishing the source documents. | Agent maintenance choice; implemented. | [README key policy](README.md#keys-and-permissions), commit `126f747`. |

## Public participation decisions, 2026-11-08

| ID | Decision and reason | Authority and status | Evidence |
| --- | --- | --- | --- |
| D034 | Prepare for a public repository, community voting, and external coding-agent contributions. Supersedes private preparation as the long-term direction in D028. | Roy's explicit direction; implementation prepared, visibility change separate. | [Issue #9](https://github.com/royderks/harness-to-api/issues/9), [G7](GENESIS.md#g7-support-a-public-contributor-community). |
| D035 | Use thumbs-up reactions on opening issue descriptions as a demand signal. Maintainers decide scope and priority; votes never authorize execution or merging. | Agent implementation default. | [Governance](GOVERNANCE.md#community-priorities). |
| D036 | Accept external agents through ordinary fork PRs, with an accountable operator and their own compute and credentials. The two-worker ceiling in D026 covers project-dispatched work only. | Agent implementation default. | [External-agent guide](docs/factory/external-agents.md), [AI policy](AI_POLICY.md). |
| D037 | Keep first contributions open while requiring scoped work and evidence. Do not adopt a vouch gate, model denylist, or automatic closure bot at launch. | Agent choice after examining T3 Code and Ghostty. | [Comparison](docs/factory/community-research.md). |
| D038 | Separate project roles from GitHub permissions. Start with Roy as owner/maintainer and route code review through CODEOWNERS. Consider organization roles when more granular access is needed. | Agent operating default; no access grants or transfer. | [Governance](GOVERNANCE.md#responsibilities). |
| D039 | Support public and private targets without trusting issue-body seed markers as identity. Preserve journal pins and require explicit adoption of untracked candidates. | Agent design synthesis; same local journal format. | [Importer rationale](docs/factory/community-research.md#choices-for-h2a). |

## Foreground onboarding decisions

| ID | Decision and reason | Authority and status | Evidence |
| --- | --- | --- | --- |
| D040 | Use one foreground init profile and the existing server owner. Persist a pending identity before inserting the key; never recreate an absent key on resume because it may have been revoked. Explicit repair discards only the recorded onboarding setup. | Approved issue #11 synthesis; implementation in progress. | [Onboarding](docs/onboarding.md), [issue #11](https://github.com/royderks/harness-to-api/issues/11). |
| D041 | Keep the auth store hash-only as in D009. Init additionally stores an optional client secret in a protected local file so an ordinary client can reconnect; it is separate from the auth store and removed by repair. | Approved issue #11 synthesis; implementation in progress. | [Onboarding](docs/onboarding.md). |

## Open decisions and evidence gaps

- Public repository visibility is the chosen direction; publication has not occurred. The public name, package registries, and domains remain unselected or unprovisioned.
- Public distribution requires signing and installation decisions. A private GitHub repository is not a public launch.
- Gemini needs a valid project configuration for the tested account. Claude has no verified login. OpenCode and IBM Bob still lack successful live-account proof here.
- The expanded OpenShell bounded-key demonstration has not yet run inside OpenShell.
- The factory's actual provider quota, unattended runtime, and future merge authorization have not been configured.

These gaps become tracked work, not implicit promises. See the GitHub queue and [factory bootstrap](docs/factory.md).
