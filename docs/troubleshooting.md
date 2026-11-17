# Troubleshooting the local quickstart

- **`h2a` or `codex` not found:** Check the executable is installed and on PATH in the shell starting init. The h2a installer defaults to `~/.local/bin` but does not persist PATH. Open a fresh shell with that directory on PATH, or use its absolute path. Follow the [harness installation guide](onboarding.md#harness-installation-guidance) and sign in using the harness's own login.
- **No `ready` event:** Read the JSON `error` event and its next step. Check the selected port, private local state directory, and whether another process owns that state. Do not kill a listener you do not own. A plan alone is not a running service.
- **Ready but completion fails:** Check login, model, and the error. `codex/default` and local validation do not prove access; a request uses provider capacity and accepted failures count against the gateway key. Do not repeatedly retry without user consent.
- **Expired, exhausted, revoked, or inconsistent setup:** Restart does not reset limits or resurrect a key. Stop your process and ask before the explicit repair in [return later](agent-quickstart.md#return-later). Conflicting identity requires the reported manual recovery, not guessing.
- **Windows:** The POSIX fixture checker does not verify native Windows sessions or signals; direct-child cancellation has [platform limits](architecture.md#operational-limits).

See [onboarding](onboarding.md) for storage requirements, exit codes, repair and other harnesses, and [policies](policies.md) for admission limits.
