# Distribution and launch

Decision: Go for the server and CLI. The gateway is dominated by process and network I/O, so a small dependency-free implementation, cancellation behavior, and installation friction matter more than raw compute throughput. Rust is viable, but does not offer enough additional value here to justify its implementation overhead. TypeScript is convenient for the Codex SDK, but adds a runtime dependency for gateway users. Both official harness CLIs expose a process protocol usable from Go.

## What current projects show

On November 3, 2026, [GitHub Trending](https://github.com/trending) included agent-desktop and Worktrunk. These are examples, not evidence that a particular language causes growth:

- [agent-desktop](https://github.com/vercel-labs/agent-desktop) uses a Rust core, npm installation, and native GitHub release downloads.
- [Worktrunk](https://github.com/max-sixty/worktrunk) offers Homebrew, Cargo, and platform package managers.

Inference: use a native binary as the implementation artifact and offer familiar package managers as distribution surfaces. Users should not have to care which language implements the server. Trending changes daily and cannot predict virality.

## First release

The repository includes builds for macOS, Linux, and Windows on arm64 and amd64, with versioned archives and SHA-256 checksums. Start with GitHub Releases and a concise, working quickstart. Live Codex and Antigravity demos have passed. Choose the repository owner/name before publishing. The local module name is `harness-to-api`; change it to the canonical repository path before offering `go install` from GitHub.

Follow with:

1. Homebrew tap/formula generated from the release checksums.
2. An npm launcher with per-platform optional binary packages, enabling `npx harness-to-api`. npm users need Node for the launcher; native-binary users do not.
3. Signed/notarized macOS artifacts and Windows signing as distribution matures.

Those channels are planned, not already published. Avoid a remote shell installer until release identity, checksums, and signing are established.

## Demo worth sharing

Show one small flow: start the server, create a sandbox key, change an existing OpenAI client's base URL, and stream a real answer. Follow with the same request using the second harness. Emphasize interoperability and existing official login; do not promise unlimited API access or universal subscription/model eligibility.

Useful next contributions are additional official harness adapters, native message-role fidelity where supported, stronger isolation, expiring keys, and per-key budgets. Keep the initial compatibility claims narrow enough that people can reproduce them.
