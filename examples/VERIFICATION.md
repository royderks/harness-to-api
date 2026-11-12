# Example verification

Checked on 2026-11-10 using macOS arm64, Python 3.13.18, and the development
gateway/fixture binaries built from `30716feace8af299211fe9044e79d6eb7524952a`.
This change does not modify the gateway. SDK versions are pinned in
[requirements.txt](requirements.txt).

| Path | Evidence | Remaining proof |
| --- | --- | --- |
| OpenAI Python streaming / six harnesses | Actual SDK → HTTP → h2a → six canned subprocess protocols passed | Live account and installed CLI version for each harness |
| LangGraph draft/review | Actual graph with Codex/Gemini fixture routing passed; two requests | Live inference content |
| OpenAI Agents SDK | Actual `Runner.run` → Chat Completions → h2a passed; one request | Live inference content |
| Plain HTTP sandbox workload | Actual `_client.py` → h2a passed | Real sandbox networking |
| Docker | Command/credential/cleanup unit test passed | Container → HTTPS tunnel → live host harness |
| E2B | SDK import/signature checked; injected fields, timeout and failure cleanup tested with doubles | Account, cloud resource creation, network route and cleanup |
| Modal | SDK import/signature checked; injected fields, timeout and failure cleanup tested with doubles | Account, cloud resource creation, network route and cleanup |
| OpenShell | Recipe reuses the documented 0.1.6 profile and provider commands | Current installation, profile lint, endpoint substitution, live request and cleanup |

Commands executed successfully from the example branch:

```sh
# With H2A_BIN and H2A_FIXTURE pointing to the above built artifacts:
python scripts/check-examples.py
python -m unittest discover -s examples/tests -v
python -m compileall -q examples scripts/check-examples.py
git diff --check
```

The client compatibility run accounts for six switched models, two graph nodes,
one SDK agent, and one plain HTTP request: exactly ten accepted requests. The
next request returns HTTP 429; after revocation, HTTP 401. Client output is checked
for the generated gateway token. The gateway and state directory are removed.
No paid provider call or cloud resource was created. Unit-test doubles are not
cloud integration evidence.

GitHub CI builds its own gateway and fixture from the tested commit, then runs
the client checks on Linux with Python 3.13. Local evidence does not imply that a
pending hosted run has passed. Do not describe canned output as live model output.

## Reproduce and extend

Follow [the example setup](README.md), then run the commands above. Set
`H2A_BIN` and `H2A_FIXTURE` to artifacts built from the checkout being tested.
For a live record, note the exact h2a SHA/version, SDK and harness versions,
provider/model, route, accepted request count, observed output and resource cleanup.
Keep credentials and private prompts out of committed evidence. Recording helpers
and social-post production material are not part of these developer examples.

## Continuous verification added 2026-11-12

On macOS with Docker Desktop's Linux/aarch64 VM, the actual Docker example passed
through an ephemeral HTTPS proxy to h2a and a canned Codex subprocess. The smoke
proved rejection of an untrusted certificate, successful completion with the
scoped test CA, exactly one debit, quota 429, revocation 401 and container cleanup.
The public Python image remains cached. Native Linux execution is delegated to CI.

All seven README launch commands and local links are now checked. The three SDK
examples execute their documented commands in the offline runner. Install commands
read the shared pinned requirements so Dependabot updates can run the compatibility
suite without copying version numbers into documentation. These checks do not
replay every setup/sign-in step or visit external documentation links.

The optional live runner and manual hosted workflow produce sanitized timestamped
reports. No real harness, E2B, Modal or OpenShell live verification is claimed for
this change. OpenShell is executable on a configured trusted host; the hosted
workflow does not provision its gateway. See [testing and account setup](../docs/examples-testing.md).
