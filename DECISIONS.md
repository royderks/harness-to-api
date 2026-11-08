# Design decisions

These choices define the architecture of harness-to-api. Changes to them belong in a scoped issue and reviewed pull request.

## Runtime and compatibility

- Use Rust for a single native gateway binary. Keep HTTP, admission, provider protocols, and protected storage in distinct modules within one Cargo package.
- Invoke documented harness CLI protocols and retain their normal login paths. Do not extract OAuth tokens or use private provider endpoints.
- Support text chat completions, legacy completions, model listing, and SSE. Reject unsupported fields rather than silently dropping them.
- Keep provider-specific parsing and execution restrictions in adapters. Keep HTTP behavior and key policy independent of the provider.
- Use harness-to-api as the project name and `h2a` as the executable.

## Credentials and admission

- Store gateway key hashes and metadata in the auth store. Keep raw keys out of subprocess arguments, diagnostics, and provider environments.
- Store the optional onboarding client credential separately in a protected local file so clients can reconnect.
- Own expiry, quotas, rate limits, request duration, and concurrency in the auth admission layer. Debit accepted requests before provider execution; failures do not refund them.
- Keep one active gateway owner per state directory. Use OS locks and durable record replacement; shared network filesystems are not supported.
- Validate model syntax and configured scopes locally. Only an explicit authenticated probe supplies evidence of account access.

## Onboarding and recovery

- Run `h2a init` in the foreground using the existing server admission lock.
- Persist a pending profile before inserting its key. Resume only an existing matching identity; never recreate a missing record because it may have been revoked.
- Verify profile, credential, and key identity before repair. Refuse conflicting identities without mutation.
- Bind the loopback listener before publishing onboarding credentials. Do not contact or terminate an unrelated port occupant.

## Deployment and execution

- Default to loopback. Use a separately managed HTTPS tunnel for remote clients.
- Treat harness execution as host-side work under the host OS account. Model scopes are not filesystem or tool permissions.
- Use persistent private local storage for gateway state and harness login. Cloud platforms need explicit lifecycle and persistence integration.
- Keep documented platform limitations explicit, including Windows direct-child cancellation and storage durability differences.

## Factory defaults

- GitHub Issues are the live queue. The seed manifest bootstraps issues and does not track their current state or launch workers.
- One coordinator assigns at most two project-dispatched workers to isolated worktrees with disjoint writable paths. Independent community forks are outside that resource limit.
- Treat public issue text, reactions, and fork contents as untrusted input. Votes never authorize execution.
- Use ordinary fork PRs for external coding agents. Their operators provide compute, credentials, and accountability.
- Require independent review and passing checks on the current head before a human-authorized merge. Repository roles and procedural rules do not imply platform-enforced permissions.
- Pin imported issue identities in a local journal. Treat issue-body markers as discovery hints; require explicit adoption or recovery when identity is ambiguous.

See [architecture](docs/architecture.md), [key policies](docs/policies.md), [onboarding](docs/onboarding.md), and the [factory guide](docs/factory.md) for the implementation contracts.
