# Build release archives

harness-to-api ships as the `h2a` executable. The native binary does not require Rust, Node.js, or Python. Install and authenticate the harness CLI separately.

From a source checkout with Rust 1.89 or newer, build an archive for the current host:

```sh
scripts/release.sh v0.1.0
```

The script writes a native archive and SHA-256 checksum to `dist/`, embedding the version in the binary. An optional second argument selects a Rust target triple; its linker and target libraries must be installed. The release workflow builds the six macOS, Linux, and Windows amd64/arm64 archives on matching native runners.

Before publishing an archive:

1. Run the repository checks in [verification](verification.md).
2. Check its checksum and run the binary's help and offline key lifecycle on the target platform.
3. Attach the matching archives and checksums to a GitHub Release.

The release workflow runs on version tags. Creating or pushing a version tag publishes release artifacts, so use it only for an approved release.

Homebrew, npm, and Windows package-manager distribution are tracked in [issue #13](https://github.com/royderks/harness-to-api/issues/13). Use the documented source build until a release or package is available. Do not assume a package name is reserved or an installer is published.
