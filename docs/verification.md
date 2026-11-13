# Test harness-to-api

Normal tests use offline subprocess fixtures and do not need provider credentials. Run from the repository root with Rust 1.89 or newer:

```sh
cargo build --locked --bin h2a --example harness-fixture
H2A_HARNESS_FIXTURE="$PWD/target/debug/examples/harness-fixture" cargo test --locked --all-targets
cargo clippy --locked --all-targets -- -D warnings
mkdir -p bin && cp target/debug/h2a bin/h2a
H2A_BIN="$PWD/bin/h2a" ./demo/run.sh --fixture
H2A_BIN="$PWD/bin/h2a" python3 scripts/check-policies.py
H2A_BIN="$PWD/bin/h2a" python3 scripts/check-init.py
```

The Python and shell lifecycle checks run on Linux and macOS. CI runs Rust tests and native builds on Linux, macOS, and Windows. Cross-compilation does not prove execution on every target architecture.

The local Unix socket-pressure regression uses synthetic state and loopback sockets, including a server with a 32-descriptor limit. It checks the 128-connection cap, recovery and cancellation under pressure:

```sh
H2A_FIXTURE="$PWD/target/debug/examples/harness-fixture" python3 scripts/check-connection-pressure.py
```

This test does not contact a provider or demonstrate resistance to arbitrary traffic.

## Documentation and CI selection

PRs that change only root Markdown files or Markdown under `docs/` run README link,
command-reference and CI selection checks without compiling Rust. The four required
check names still report results; native-check jobs report that no native build was
needed. Other paths, empty/unknown diffs and changes to CI itself run the full checks.
A failed diff calculation or documentation check fails the required checks.
Feature branches run checks on PR events; pushes to `main` are also checked.

```sh
python3 scripts/check_readme.py
python3 -m unittest discover -s scripts/ci_tests -v
python3 scripts/check_readme.py --binary ./bin/h2a
```

The fast check reads the CLI's help literal without a toolchain. Native CI compares
the README Commands table with built CLI help, excluding the repository-maintenance
`factory` commands documented in the factory guide. Release verification repeats that
comparison using the README and binary extracted from each archive, so missing or
obsolete command rows prevent a draft release.

## Installer checks

```sh
python3 -m unittest discover -s scripts/install_tests -v
```

The Unix tests use local archives and a mocked download boundary to check platform
selection, repeat installation, checksum refusal, and terminal input after a pipe.
The installer workflow also installs the published binary on native macOS, Linux,
and Windows runners, checks its version, and tests Windows repeat installation and
checksum refusal using Windows PowerShell 5.1. These checks use no provider accounts.

## Coverage

- API authentication, model visibility, text completions, SSE, strict request validation, and sanitized errors.
- Durable key quotas, rolling rate limits, request deadlines, expiry, revocation, concurrency, and server ownership.
- Harness protocol parsing, malformed output, tool-event rejection, cancellation, and credential exclusion.
- Onboarding, repeated setup, interrupted setup, identity-checked repair, unrelated-key preservation, occupied ports, optional probes, and secret-free CLI output.
- Native storage checks, including Windows reopening of auth-produced files and strict credential ownership.

## Example client checks

With Python 3.13 and the fixture binaries built above:

```sh
python3 -m venv .venv
. .venv/bin/activate
python -m pip install -r examples/requirements.txt
H2A_BIN="$PWD/target/debug/h2a" H2A_FIXTURE="$PWD/target/debug/examples/harness-fixture" python scripts/check-examples.py
python -m unittest discover -s examples/tests -v
python3 scripts/check-example-docs.py
H2A_BIN="$PWD/target/debug/h2a" H2A_FIXTURE="$PWD/target/debug/examples/harness-fixture" python3 scripts/check-docker-example.py
```

These run actual framework clients against offline harness fixtures. They do not
create cloud sandboxes or contact provider accounts. The Docker check uses real
local containers and a temporary test CA. See [example evidence](../examples/VERIFICATION.md)
and [continuous verification and live setup](examples-testing.md).

## Factory checks

```sh
h2a factory validate
python3 -m unittest discover -s scripts/tests -v
```

Factory tests use a fake GitHub CLI transport. They cover publication retries, ambiguous responses, explicit recovery, journal identity, and untrusted issue markers.

## Provider checks

Live checks consume provider access and gateway quota. Run them one provider at a time only after an authorized login, required project configuration, and a bounded request allowance are available. Create a one-request key with a short deadline (`--max-requests 1 --ttl 5m --request-timeout 30s --max-concurrent 1`), run `h2a probe`, inspect `accepted` and `remaining_requests` with `h2a keys inspect`, and revoke the key afterward. Gateway request caps constrain gateway admissions and runtime; they do not bound upstream provider dollar spend:

```sh
h2a probe --base-url http://127.0.0.1:8787/v1 --model codex/default
./demo/run.sh --provider codex
./demo/run.sh --provider antigravity
```

| Provider | Protocol fixture | Tested CLI version | Live account status in this checkout |
| --- | --- | --- | --- |
| `codex` | Passed | `codex-cli 0.165.0` (host), `0.166.0` (Docker image) | Verified with live completion, streaming, and revocation |
| `antigravity` | Passed | `agy 1.4.12` | Verified with live completion, streaming, and revocation |
| `gemini` | Passed | `gemini 0.26.0` | Blocked on `GOOGLE_CLOUD_PROJECT` / `GOOGLE_CLOUD_PROJECT_ID`; 1-request `h2a probe` on 2026-11-13 recorded `accepted: 1, remaining_requests: 0` and sanitized `http_status: 502` ([setup guide](gemini.md)) |
| `claude` | Passed | `claude 2.1.317` (flag/protocol check) | Unverified live; CLI and `claude auth login` not configured on this host ([setup guide](claude.md)) |
| `opencode` | Passed | `@opencode/cli 2.2.5` (protocol check) | Unverified live; CLI and `opencode auth login` not configured on this host ([setup guide](opencode.md)) |
| `ibm-bob` | Passed | `pi 1.5.0` + `pi-bob 0.2.6` (Docker image) | Unverified live; requires an eligible IBM Bob SSO login in Pi ([setup guide](ibm-bob.md)) |

Offline fixtures cover all six provider protocols. Successful fixture tests do not establish live account access or compatibility with every CLI release. Use the provider's setup guide and `h2a probe` to check your own account and model. OpenShell setup instructions are in [the sandbox guide](openshell-live.md).

## Deployment limits

The [Docker runtime recipe](docker.md#validate-a-deployment) has been validated offline for Compose configuration (`deploy/compose.yaml` and `deploy/compose.bob.yaml`), image build, non-root runtime layout, `/healthz`, `/v1/models` authentication, volume persistence across container recreation, key revocation, and task-owned project cleanup. Live provider sign-in, completion streaming, public tunnels, and managed cloud deployments still require end-to-end validation on the chosen host before relying on a deployment.

Windows cancellation currently targets the direct CLI child, not its descendants. The POSIX CLI integration scripts do not validate Windows signal handling. Storage tests do not simulate every power-loss ordering. See [operational limits](architecture.md#operational-limits).

Release PR jobs additionally stage version-switched installer copies from the
committed source and install the matching candidate archive at a mocked download
boundary. Unix runs the installed binary through the offline PTY init checks;
Windows checks installed version, repeat installation, and checksum/download
refusal, but does not prove the interactive chooser in a Windows terminal.
