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
the README Commands table with built CLI help. Release verification repeats that
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

## Factory checks

```sh
h2a factory validate
python3 -m unittest discover -s scripts/tests -v
```

Factory tests use a fake GitHub CLI transport. They cover publication retries, ambiguous responses, explicit recovery, journal identity, and untrusted issue markers.

## Provider checks

Live checks consume provider access and gateway quota. Run them only with an authenticated account and a bounded allowance:

```sh
h2a probe --base-url http://127.0.0.1:8787/v1 --model codex/default
./demo/run.sh --provider codex
./demo/run.sh --provider antigravity
```

Offline fixtures cover all six provider protocols. Successful fixture tests do not establish live account access or compatibility with every CLI release. Use the provider's setup guide and `h2a probe` to check your own account and model. OpenShell setup instructions are in [the sandbox guide](openshell-live.md).

## Deployment limits

The Docker recipe, public tunnels, and managed cloud deployments require end-to-end validation on the chosen host. Test streaming, timeouts, login persistence, restart recovery, and revocation before relying on a deployment.

Windows cancellation currently targets the direct CLI child, not its descendants. The POSIX CLI integration scripts do not validate Windows signal handling. Storage tests do not simulate every power-loss ordering. See [operational limits](architecture.md#operational-limits).
