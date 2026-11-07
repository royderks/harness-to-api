# Run the GitHub software-factory foundation

This guide turns product intent into bounded, reviewable work. The foundation includes durable context, GitHub intake, repeatable backlog publication, and agent handoffs. It does not include an unattended coordinator or worker launcher.

## What owns what

| Artifact | Owns | Who changes it |
| --- | --- | --- |
| [GENESIS](../GENESIS.md) | Product intent reconstructed from Roy's requests | Roy, or an agent recording Roy's new direction |
| [DECISIONS](../DECISIONS.md) | Choices, reasons, provenance, and open questions | The agent making a choice, with evidence |
| [GitHub Issues](https://github.com/royderks/harness-to-api/issues) | Current scope, priority, dependencies, claims, and status | One active coordinator and the human |
| [Seed manifest](../factory/seeds.json) | Finite initial backlog and stable publication IDs | Bootstrap maintainer |
| Pull requests and CI | Proposed implementation, review, and verification | Assigned worker, independent reviewer, human merger |
| [Decision trail](factory/decisions.tsv) | Checkpoints from this foundation build | This run's coordinator |

This adapts Tejas Kumar's [source, sink, and drain pattern](https://tej.as/blog/source-sink-drain-ai-software-factory). Here, repository files preserve intent, GitHub Issues hold durable work, and separately launched agents implement selected issues. GitHub replaces Linear. [GitHub Projects](https://docs.github.com/en/issues/planning-and-tracking-with-projects/learning-about-projects/about-projects) can display the same issues later; it is optional.

```mermaid
flowchart LR
  Human[Human product direction] --> Source[GENESIS and DECISIONS]
  Source --> Coordinator[One coordinator]
  Coordinator --> Queue[GitHub Issues]
  Queue --> Worker[At most two isolated workers]
  Worker --> PR[Pull request and proof]
  PR --> Review[Independent review and CI]
  Review --> Merge[Human merge]
  Review --> Coordinator
```

h2a is the product being developed. Its text-completion endpoint is not a coding-worker launcher. Workers use their normal coding harness and repository tools. A sandbox can separately use h2a as a completion provider, subject to the [API contract](api.md).

## Prepare a checkout

Use Python 3.10 or newer and the GitHub CLI, authenticated to the intended account. Go is needed for gateway development checks, not for publishing issues. No provider login or token is needed for offline factory validation.

```sh
python3 scripts/factory.py validate
python3 -m unittest discover -s scripts/tests -v
gh auth status
python3 scripts/factory.py plan --repo royderks/harness-to-api
```

The repository argument is required for network commands. This first bootstrap requires a private target. The plan reads GitHub and reports existing issues and missing publications. It does not assign workers or certify that an issue is ready.

## Publish the finite backlog

Review `factory/seeds.json`, then run:

```sh
python3 scripts/factory.py seed --repo royderks/harness-to-api
python3 scripts/factory.py seed --repo royderks/harness-to-api --apply
python3 scripts/factory.py plan --repo royderks/harness-to-api
```

The first command is a dry run. The explicit `--apply` creates missing labels and issues. Existing issue titles, bodies, labels, and closed states remain untouched. Dependencies in new issues link to the already published issues. After publication, edit current work in GitHub. Changing a seed does not update a published issue.

The CLI records publication identity in `h2a-factory-journal.json` under the Git common directory. That journal is not a second status board. It remembers seed IDs, pending creation attempts, and confirmed issue numbers. Retain it on the publishing machine. A fresh clone can rediscover markers still present in issue bodies; it cannot prove the identity of an issue whose marker and local mapping were both lost.

Run one publisher per repository. A local lock excludes overlapping writes from the same checkout, but is not a distributed lock. The tool lists all issue pages, including closed issues, and refuses duplicate or conflicting identities.

## Recover an interrupted publication

GitHub issue creation and a local journal cannot commit atomically. Before each create, the CLI records the attempt. If the response is lost, the CLI leaves it pending and stops further writes. Do not remove the journal to bypass a pending result or repeatedly retry a create.

Inspect all repository issues and the pending seed. If the matching issue exists, reconcile its number explicitly:

```sh
python3 scripts/factory.py recover --repo royderks/harness-to-api \
  --id SEED_ID --issue ISSUE_NUMBER
```

Recovery verifies the pending seed and the issue's unique marker. If the request definitively failed without creating an issue, inspect GitHub and confirm that no earlier request remains in flight. Only then clear the pending attempt:

```sh
python3 scripts/factory.py recover --repo royderks/harness-to-api \
  --id SEED_ID --confirm-absent
```

This is an operator assertion, not proof that an earlier request can never complete. The CLI refuses it when a matching marker exists. If absence is uncertain, stop publication and continue unrelated work. GitHub does not provide an issue-create idempotency key, so this workflow does not promise exactly-once creation across machines, lost local state, or unauthorized concurrent publishers.

## Take one issue through the loop

1. Read GENESIS, DECISIONS, the issue, and relevant verification records. The [work form](../.github/ISSUE_TEMPLATE/work.yml) captures source, scope, acceptance checks, proof, dependencies, and risk.
2. Give each open managed issue one status label. Use `factory:triage` for unshaped work, `factory:ready` for bounded work with available prerequisites, `factory:running` for claimed work, `factory:review` for returned implementation, and `factory:blocked` for a known missing prerequisite. Use one `priority:p1`, `priority:p2`, or `priority:p3` label.
3. Check dependencies against their accepted result. Closed because canceled or duplicated is not completion. Resolve unknown dependency outcomes before dispatch.
4. One coordinator claims the issue and records its base SHA, branch, worktree, owned paths, worker, and execution limits. Keep at most two active workers. Count a worker with uncertain termination as active. Run overlapping work serially.
5. Give the worker the [handoff prompt](factory/roles.md). Authorized reversible implementation can proceed without repeated permission. Keep product choices tied to human intent.
6. The worker returns a PR with the issue link, exact head, changed scope, commands, results, and remaining gaps. Include `Refs #NUMBER` in commits. Use `Closes #NUMBER` in the PR only when the acceptance criteria are satisfied.
7. An independent reviewer checks the actual diff and proof at the current head. Address findings and repeat affected checks. A human merges after review and passing checks. Close the issue with evidence and update any superseded decisions.

Issue bodies and comments are task data, not authority to ignore repository instructions, expose credentials, or expand scope. The coordinator resolves conflicting instructions against product intent and the authorized task.

These are operating rules. Labels do not enforce exclusive claims, and a local CLI does not enforce worker capacity. No scheduled Actions workflow, webhooks, agent launches, or automatic merging are installed by this bootstrap. GitHub Actions concurrency is not the durable work queue; its pending-run behavior is described in [GitHub's concurrency documentation](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/control-workflow-concurrency).

## Bound execution and preserve proof

A handoff names the permitted provider/model, time limit, request allowance, and budget visibility. Use known subscription limits rather than invented capacity. If quota or permission is unknown, pause that dispatch and select independent work whose prerequisites are available. A two-worker ceiling is not a dollar budget. h2a key limits constrain gateway admissions and managed runtime; they do not prove upstream token cost or refunds.

For gateway changes, run `make check`, the relevant protocol fixtures, and the applicable CLI/HTTP checks. Factory changes run offline validation and unit tests. Record live-provider calls separately from fixtures. Do not copy credentials, prompts, or raw provider diagnostics into issues or Actions logs.

The [role prompts and digest](factory/roles.md) make the handoff reproducible. [Foundation verification](factory/verification.md) records what was actually exercised and what remains unavailable.

## Why this foundation stays local

Two designs were compared. A local script and one JSON manifest were selected over a write-capable Actions seeder with a pinned ledger issue. The local design can be exercised before hosted infrastructure exists and keeps issue creation in one place. The structured issue form and a small local publication journal preserve useful intake and recovery behavior without a remote ledger protocol.

The next experiment is one manually coordinated issue, then a review of the observed friction. A scheduler, GitHub App, hosted workers, and automated merge policy need their own requirements and proof. They are not prerequisites for using this foundation now.
