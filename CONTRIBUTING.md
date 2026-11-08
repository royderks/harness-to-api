# Contributing

Read [governance](GOVERNANCE.md) for roles, voting, and issue claims. People and coding agents use the same issue-to-PR process. Agent operators also follow [AI_POLICY.md](AI_POLICY.md) and the [external-agent guide](docs/factory/external-agents.md).

## Choose work and submit evidence

1. Search existing issues. Support an existing request with a thumbs-up on its opening description, rather than another issue or a `+1` comment.
2. For substantial work, use an issue marked `factory:ready` or agree on scope with a maintainer first. Keep each PR focused on one problem. Report small obvious fixes in a linked issue too; they do not need a separate design proposal.
3. Comment with your approach and coordinate with existing assignees. Implement in a fork and an isolated branch/worktree. A comment alone does not reserve the issue.
4. Open a PR with the issue link, the behavior changed, reproduction or before/after evidence, exact checks and results, and known limitations. Use the PR template and disclose AI assistance when used.
5. Respond to review. A maintainer decides whether to merge after independent review and current-head checks. First-time contributors do not need a vouch, and passing CI alone does not guarantee acceptance.

## Develop locally

Use Go 1.24 or newer. The gateway has no third-party Go modules. End users can use a prebuilt binary; only source builds need Go.

```sh
make build
make check
```

Normal tests use HTTP servers and compiled CLI fixtures. They need no provider credentials and do not make inference calls. To test your existing Codex login explicitly:

```sh
H2A_LIVE_CODEX=1 go test ./internal/harness -run TestLiveCodex -v
```

## Add a harness

Implement `internal/harness.Adapter` using the provider's official SDK or CLI protocol. Emit assistant text through the callback, return usage only when reported, and return success only after a terminal success event. Honor context cancellation and callback errors. Keep command construction outside the HTTP request surface.

Use `runLines` for a CLI with newline-delimited JSON. It passes a restricted environment, creates a temporary working directory, bounds output, and cancels the process. Provider-specific permissions still need explicit configuration in the adapter.

Register the adapter in the CLI and add protocol fixtures for success, malformed output, early exit, provider failure, and cancellation. Record the official documentation and tested CLI version. Do not claim live-provider verification from fixture tests.

## Change the API

Keep compatibility explicit. Unsupported OpenAI fields should fail with an actionable error rather than being ignored. Test through HTTP, including streaming and errors. A model scope restricts the requested model string; it is not a filesystem or tool permission.

Never commit credentials, captured private prompts, local provider state, or raw provider diagnostics. Demo recordings must use purpose-created keys and avoid printing them.

## Releases

`scripts/release.sh v0.1.0` produces native archives and checksums in `dist/`. Test the extracted native archive, not just the source build. GitHub tag publication, package registration, signing, and domain registration are separate release actions.
