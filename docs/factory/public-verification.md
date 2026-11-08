# Public participation verification

Recorded on 2026-11-08 for [issue #9](https://github.com/royderks/harness-to-api/issues/9), stacked on foundation commit `3379926e9f8a428326a9fd7aedb166ecf5450155`. This change prepares public participation. The repository remains private; no visibility change or access grant was performed.

## GitHub reads

The documented ready-queue query returned issues #2 and #3 with their labels and assignees:

```sh
gh issue list --repo royderks/harness-to-api --state open \
  --label factory:ready --limit 100 \
  --json number,title,url,body,labels,assignees
```

The ranked query returned eight open issues, all with zero thumbs-up reactions, and `incomplete_results: false`:

```sh
gh api --method GET search/issues \
  -f q='repo:royderks/harness-to-api is:issue is:open sort:reactions-+1-desc'
```

These are authenticated reads of the current private repository. They do not prove anonymous public access or exercise a community fork PR. No synthetic votes were added.

## Local checks

- `python3 scripts/factory.py validate` passed: seven seeds and eleven labels.
- `python3 -m unittest discover -s scripts/tests -v` passed twelve CLI tests using fake GitHub responses. Coverage includes public targets, copied markers, preserved pins, explicit adoption, scoped absence, lost responses, permission denial, invalid pagination, and corrupt state.
- `git diff --check` passed.

- `/tmp/h2a-public-go/go/bin/go test ./...` and `/tmp/h2a-public-go/go/bin/go vet ./...` passed on macOS arm64 with Go 1.27.3. The earlier temporary installation lacked standard-library sources; a fresh official archive matched its published SHA-256 before extraction.
- Ruby Psych parsed the edited issue form and both edited workflows. Relative documentation file links resolve.
- `python3 scripts/factory.py plan --repo royderks/harness-to-api` read back all seven journal-pinned seed issues without remote writes.

## Workflow boundary

The CI and factory workflows use `pull_request` and `push`, GitHub-hosted runners, `contents: read`, and offline checks. This change disables checkout credential persistence and sets a 15-minute job timeout. Neither workflow executes contributor code through `pull_request_target` or supplies provider credentials. CODEOWNERS routes review after it reaches the base branch; it does not enforce approval by itself.

## Limits

Claim coordination, the project-worker ceiling, independent review, and human merge are operating rules. Personal-repository permissions do not provide separate Triage and Maintain roles. GitHub's existing approval and token behavior still applies to external fork workflows.

Importer fixtures establish local decisions under simulated GitHub responses. They cannot establish distributed exactly-once publication. A maintainer must retain the journal, inspect adoption and absence decisions, and use one publisher across machines. Untracked forged markers can require reconciliation; they cannot redirect a journal-pinned identity.
