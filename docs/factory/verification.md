# Factory foundation verification

Recorded on 2026-11-07. The gateway baseline is `072fa7497995113afffa07fcd7a38398cd92c0e9`. Factory work is on `factory/github-foundation` and does not change gateway runtime code.

## Local checks

- `python3 scripts/factory.py validate` passed for seven seeds and eleven labels.
- `python3 -m unittest discover -s scripts/tests -v` passed eight tests that invoke the CLI with a fake `gh` executable. They cover dry runs, repeat publication, edited and closed issues, all-state pagination, lost responses, explicit recovery, rejected creates, explicit absence confirmation, invalid sources, dependency cycles, duplicate markers, public targets, corrupt journals, missing mapped issues, and reserved marker text rejected before publication.
- `make check GO=/tmp/h2a-go-toolchain/go/bin/go` passed Go race tests, vet, and the formatting check on macOS arm64.

The fake CLI is evidence of the local publication protocol. It does not establish GitHub consistency or exactly-once delivery across concurrent publishers.

## Hosted baseline checks

[CI run 42425919094](https://github.com/royderks/harness-to-api/actions/runs/42425919094) passed at `072fa74` on Ubuntu, macOS, and Windows. Each job passed Go vet, race tests, and a native build. Ubuntu and macOS also passed the offline demo and bounded-key HTTP lifecycle check. Those two integration checks were skipped on Windows by workflow design.

This supplies native Windows Go-test evidence. It does not establish Windows descendant-process cleanup, every archive architecture, signing, live provider availability, or cloud deployment.

## Repository setup

The repository [royderks/harness-to-api](https://github.com/royderks/harness-to-api) was created private and its visibility was read back through the GitHub connector. The original baseline was pushed to `main`.

GitHub accepted strict required checks for `test (ubuntu-latest)`, `test (macos-latest)`, `test (windows-latest)`, and `validate`. Force pushes and branch deletion are disabled by the rule. Admin enforcement is disabled, so the owner can bypass protection. No required GitHub approval count is configured; independent review and human merge remain operating rules. Auto-merge remains disabled. These settings are an observed snapshot, not a promise that future administrators cannot change them. See GitHub's [branch protection behavior](https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/managing-protected-branches/about-protected-branches).

## Boundaries

No provider credentials were added to GitHub. No live inference, worker launcher, scheduled coordinator, public release, tunnel, cloud deployment, or Docker cleanup was started for this foundation.

Only one publisher may operate across machines. Local file locking excludes concurrent publishers sharing the Git common directory. The journal must be retained; if both a marker and its journal mapping are lost, automatic identity recovery is unavailable. An explicit absence confirmation is an operator assertion after reconciliation, not a distributed proof.

The issue forms and source documents become default-branch content when the foundation PR is merged. Seed source links to new files on `main` become readable then. The PR contains the reviewable source before that merge.

## Live publication

The actual CLI created eleven labels and seven issues in the private repository. A second `seed --apply` preserved every issue number, title, body, state, and label. The comparison read all issue pages before and after the repeat run and asserted exact equality of those fields.

- [#1 Foundation](https://github.com/royderks/harness-to-api/issues/1) tracks this work.
- [#2 OpenShell bounded demo](https://github.com/royderks/harness-to-api/issues/2) and [#3 native platform proof](https://github.com/royderks/harness-to-api/issues/3) are ready candidates.
- [#4 provider evidence](https://github.com/royderks/harness-to-api/issues/4) and [#5 Docker proof](https://github.com/royderks/harness-to-api/issues/5) identify unavailable prerequisites.
- [#6 one-worker pilot](https://github.com/royderks/harness-to-api/issues/6) and [#7 public name](https://github.com/royderks/harness-to-api/issues/7) remain in triage.

The real repeat run checked ordinary idempotence. Lost responses, edited/closed issues and absence recovery were tested through the fake CLI, not by deliberately interrupting a live GitHub request.

## Independent review

A separate GPT-6.1 Terra review found one latent validation defect. Seed prose could contain the reserved issue-identity marker and cause later inventory reads to reject an issue created by the tool itself. The current seed manifest did not contain such text. Validation now rejects that marker syntax in seed text before any GitHub writes, with a regression check through the CLI. The comment review found no actionable issues.
