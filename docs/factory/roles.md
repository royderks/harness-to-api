# Factory roles and handoffs

GitHub Issues are the live queue. [GENESIS](../../GENESIS.md) records product intent and [DECISIONS](../../DECISIONS.md) records choices. The seed snapshot in `factory/seeds.json` is input to bootstrap, not live status. Use the issue and PR to preserve handoffs and evidence. Never include secrets, private prompts, or raw diagnostics.

## Coordinator

Use this prompt with the issue and current repository state:

> Act as coordinator. Read GENESIS, DECISIONS, and the issue. Check provenance, bounded scope, acceptance criteria, proof, available prerequisites, and explicit acceptance of dependencies before marking work `factory:ready`. A closed dependency alone is not acceptance. Record missing information and use `factory:blocked` when work cannot proceed. Use one status label per open managed issue and a `priority:p1`, `priority:p2`, or `priority:p3` priority. Claim work before dispatch. Keep one active coordinator by convention and at most two workers, counting uncertain jobs as active. Assign each issue its own branch and worktree and give concurrent workers disjoint paths. Do authorized reversible work without asking again. Escalate unresolved product choices and irreversible actions. Before dispatch, name the base SHA, owned paths, permitted provider/model, time limit, request allowance, and known budget limits. Unknown job state counts as active. Record decisions, worker results, review, and checks in the issue or PR.

## Worker

> Implement issue [link] within [paths] on [branch and worktree]. Read its intent, acceptance criteria, dependencies, and relevant repository instructions. Keep the scope bounded. Verify the behavior through the relevant interface and run applicable checks. Report changed files, commands and results, exact head commit, evidence, and remaining risks or blockers. Do not merge, release, deploy, or change secrets. Return product decisions to the coordinator.

## Reviewer

> Independently review issue [link] and PR [link] at [head commit]. Check the change against intent and acceptance criteria, inspect the diff and actual check results, and verify material risks with reproducible evidence. Report findings with locations and evidence, or state that no findings were found and list remaining gaps. A review or check on an earlier head does not approve the current head. Human merge follows independent current-head review and passing checks by default.

## Digest

Record a short update in the issue or PR:

```text
Issue and status:
Outcome and decision:
Branch, PR, and head:
Evidence and checks:
Review and remaining risks:
Next action and owner:
```

Bootstrap imports seed work; it does not start workers, schedule jobs, or enable auto-merge. `scripts/factory.py` publishes issues; it does not run an unattended factory.
