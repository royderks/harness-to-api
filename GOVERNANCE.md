# Project governance

harness-to-api welcomes contributions from people and coding agents. Roy Derks (`@royderks`) is the initial owner and maintainer. Public participation does not grant repository write access or access to the project's harness accounts.

## Responsibilities

| Role | Responsibility | Access |
| --- | --- | --- |
| Contributor | Report problems, vote, propose a scoped change, and submit a fork PR with evidence. | Public GitHub access; no upstream write permission needed. |
| Agent operator | Own the agent's submissions, credentials, compute costs, and follow-up. Follow [AI_POLICY.md](AI_POLICY.md). | Their own account or bot identity and fork. |
| Triager | Reproduce reports, identify duplicates, clarify scope, and recommend priority. | A project responsibility; label changes require suitable GitHub permission. |
| Maintainer | Approve scope, manage readiness and claims, review changes, and merge accepted work. | Access explicitly granted by the owner. |
| Owner | Set product direction, appoint or remove maintainers, and manage repository access and releases. | Repository administration. |

These project roles do not create GitHub permissions. The current repository belongs to a personal account. An organization provides explicit Triage, Write, Maintain, and Admin roles when the team needs finer access control. Moving to an organization is a separate decision. See GitHub's [personal repository permissions](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/repository-access-and-collaboration/permission-levels-for-a-personal-account-repository) and [organization roles](https://docs.github.com/en/organizations/managing-user-access-to-your-organizations-repositories/managing-repository-roles/repository-roles-for-an-organization).

Maintainer access follows sustained useful contributions, careful review, and willingness to support the affected code. The owner records an appointment and its scope in this file. Votes, PR count, and a previously merged change do not automatically grant access.

## Community priorities

Add a thumbs-up reaction to the opening issue description to support it. Avoid comments that only say `+1`. Use the [most-supported open issues](https://github.com/royderks/harness-to-api/issues?q=is%3Aissue%20is%3Aopen%20sort%3Areactions-%2B1-desc) view. GitHub supports [sorting by thumbs-up reactions](https://docs.github.com/en/search-github/getting-started-with-searching-on-github/sorting-search-results#sort-by-reactions).

Reactions measure expressed interest from accounts, not unique people or approved scope. Maintainers weigh them alongside correctness, impact, effort, and product direction. No vote threshold starts a worker, spends project resources, changes permissions, or approves a merge. Agents must not manufacture votes or duplicate requests.

## Admission and review

New proposals enter `factory:triage`. Maintainers use `factory:ready` when a linked issue has accepted scope, testable acceptance criteria, and available prerequisites. For substantial work, agree on that scope before implementation. A ready issue authorizes work within its stated scope; it does not guarantee acceptance of a particular solution.

Contributors comment with their intended approach and expected availability. A maintainer records the accepted assignee or operator and moves claimed work to `factory:running`. A comment alone is not an exclusive reservation. Small experiments can proceed in a fork while coordination is pending; avoid competing full implementations. A maintainer may release a stale claim after checking for active work and notifying its operator in the issue.

Each PR addresses one problem and supplies evidence for the affected behavior. Maintainers can request a smaller change, defer work, or close it with a specific reason and a path to reconsideration. Accepted contributions need independent review and passing checks at the current head before a human merge. Existing owner bypass remains available; this policy does not claim a technically enforced human-only merge gate.

[CODEOWNERS](.github/CODEOWNERS) routes review requests to the responsible maintainer once it is on the base branch. It is not a replacement for branch protection. See [GitHub's code-owner behavior](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/about-code-owners).

The two-worker ceiling applies to work dispatched by the project's coordinator using project resources. It does not limit independent community forks. Coordination and review capacity still apply to incoming PRs. Release, repository visibility, access grants, and organization transfer remain owner decisions.
