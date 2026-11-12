# Keep examples working

Examples are supported entry points. Their executable commands, documentation and
compatibility checks change together. A passing fixture test establishes local
protocol compatibility, not a working provider account or cloud sandbox.

## Checks on pull requests

Every PR runs `python3 scripts/check-example-docs.py`. It checks the seven example
README launch blocks against `examples/commands.json`, declared dependencies
against `examples/requirements.txt`, and local README links. The offline SDK runner
executes the same launch commands. Full README setup, remote links and account
sign-in instructions are not automatically replayed.

Code, dependency and example changes run the real SDK clients through a freshly
built h2a and six offline harness protocols. The Linux job also runs the actual
Docker example through a temporary HTTPS proxy, requiring certificate trust,
correct output, a one-request quota, revocation and container cleanup. Docker
being unavailable fails the test. It does not silently skip. The Python base image
can remain in the Docker cache. No harness account or cloud sandbox credentials
are used by PR jobs, including fork and Dependabot PRs.

Root and `docs/` Markdown-only edits run lightweight documentation checks without
native builds. Example README edits currently run the full checks as well.

```sh
python3 scripts/check-example-docs.py
python3 -m unittest discover -s examples/tests -p test_example_docs.py -v
python3 -m unittest discover -s scripts/live_tests -v
H2A_BIN="$PWD/target/debug/h2a" H2A_FIXTURE="$PWD/target/debug/examples/harness-fixture" python3 scripts/check-docker-example.py
```

## Dependency updates

Dependabot checks `examples/requirements.txt` weekly and groups client updates
into pull requests. These PRs run the same real SDK and Docker checks. They are not
automatically merged. README install commands use this requirements file, so a
version update does not require copying pins into several documents. Direct
versions are pinned; transitive dependencies are still resolved by pip, and an
upstream service changing without a package release still requires a live check.
See [Dependabot configuration](https://docs.github.com/en/code-security/concepts/supply-chain-security/about-the-dependabot-yml-file).

## Trusted live checks

The `Live examples` workflow is manually dispatched from `main` only. It checks
out that exact main commit on a GitHub-hosted runner and uses `contents: read`.
It has no PR, scheduled, or `pull_request_target` trigger. Dependency installation
runs before credential injection. Fork PRs never receive these credentials.

Before enabling it, the repository owner must create the `examples-live`
environment, restrict deployments to the exact `main` branch, and configure
required reviewers. This is an environment setup requirement, not a protection
enforced by a YAML file alone. Keep live secrets in this environment, not generic
repository or organization secrets. Review dependency changes before approving a
live run. See [GitHub environment protection](https://docs.github.com/en/actions/concepts/workflows-and-actions/deployment-environments).

Environment configuration:

| Setting | Purpose |
| --- | --- |
| Variable `H2A_LIVE_BASE_URL` | Operator-managed HTTPS h2a endpoint ending in `/v1` |
| Secret `H2A_LIVE_API_KEY` | Dedicated exact-model key with a short TTL and small request quota |
| Secret `E2B_API_KEY` | Needed only for E2B |
| Secrets `MODAL_TOKEN_ID`, `MODAL_TOKEN_SECRET` | Needed only for Modal |

Provision a fresh key on the gateway host before a live run. Allow two requests
for LangGraph and one for each other selected example. Use a short expiry,
90-second request timeout and concurrency one. Revoke the key afterward. These
caps bound gateway requests, not provider or cloud dollars. E2B/Modal sandbox
lifetimes are limited to 300 seconds; normal cleanup is also attempted. Forced
workflow cancellation or a failed cloud cleanup can still leave resources until
the service timeout, so inspect the account after an interrupted run.

E2B and Modal require separate accounts. Adding this workflow provisions no
accounts, credentials, tunnels, gateways or paid resources.
Missing configuration produces `not_run` with a nonzero exit, never a pass.

For a configured trusted host, the same runner also supports OpenShell:

```sh
python3 scripts/check-live-example.py --allow-live --example openshell
```

Supply the example's normal environment, plus `OPENSHELL_GATEWAY_ENDPOINT` for
OpenShell. A configured OpenShell CLI/gateway is required. The hosted workflow
covers the other six examples; it does not install or expose an OpenShell gateway.

## Evidence and last successful verification

The local runner writes `artifacts/live-example.json` with the checked example,
source commit, SDK versions, time, and `passed`, `failed`, `not_run` or `incomplete`.
The hosted job initializes its report in runner temporary storage before checkout
and dependency installation, so setup failures retain `not_run`. Once execution
starts, an interrupted attempt retains `incomplete`. A passed report
also records the model and `last_success_at`. Failed or unconfigured runs have no
success timestamp. A job that never starts, or loses its runner before artifact
upload, can have no report; that means unknown, not success. Reports omit endpoint URLs, keys, stdout, stderr and model
output. The gateway and provider CLI versions are not remotely introspected;
operators must record those separately when diagnosing provider drift.

The workflow preserves reports as 90-day artifacts and in its job summary. Use
the latest **passed report for the particular example and model**, not merely the
latest successful run for a different example. Download/retain reports for a
longer audit trail. Missing or expired evidence means unknown, not verified.

```sh
gh run list --workflow examples-live.yml --status success
gh run download RUN_ID --name live-example-EXAMPLE-RUN_ID
```

No real-provider or cloud success date is claimed until an actual live run passes.
See [current example evidence](../examples/VERIFICATION.md).
