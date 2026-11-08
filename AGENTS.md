# Repository instructions

Read [GENESIS.md](GENESIS.md) for product intent and [DECISIONS.md](DECISIONS.md) for decisions. Read the relevant architecture, policy, and verification docs before changing behavior. Human product intent governs agent choices.

GitHub Issues are the live work queue. Link planned work to an issue with product intent or evidence of a defect. `factory/seeds.json` is a seed snapshot, not a second status board. The bootstrap tool is `scripts/factory.py`. It publishes seed issues and does not launch workers. See [factory roles](docs/factory/roles.md) for handoffs.

Each open managed issue has exactly one status label: `factory:triage`, `factory:ready`, `factory:running`, `factory:review`, or `factory:blocked`. Use `priority:p1`, `priority:p2`, or `priority:p3` for priority. Close an issue as done only with evidence. A closed dependency is not automatically accepted.

One coordinator is active by convention. Run at most two workers, counting jobs whose state is uncertain as active. Give each issue an isolated branch and worktree, and give concurrent workers disjoint paths. Bootstrap does not start workers. Proceed autonomously with authorized reversible work. Escalate unresolved product decisions and irreversible actions. Do not auto-merge, release, deploy, or change secrets. Independent review and passing checks on the current PR head precede a human merge by default. These are operating rules, not claims of enforced branch protection.

Protect credentials and private prompts. Do not put secrets or raw diagnostics in issues, PRs, or logs. h2a request caps count accepted requests; they are not provider dollar limits. Keep unsupported API behavior explicit and distinguish fixture evidence from live-provider evidence.

Run `go test ./...` and `go vet ./...` for relevant changes. For factory changes, run `python3 scripts/factory.py validate` and `python3 -m unittest discover -s scripts/tests -v`. Report exact commands and results. Use [verification](docs/verification.md) for additional checks. Hosted Go tests and native builds passed on Linux, macOS, and Windows at `072fa74`. Do not infer native execution from cross-compilation.
