# Install h2a without Rust

Download a native archive and matching `checksums_*.txt` from
[GitHub Releases](https://github.com/royderks/harness-to-api/releases).
The executable needs no Rust, Cargo, Node.js, or Python. Your selected harness
still needs its own installation and login. Python is only needed for Python
client examples.

Release downloads follow repository visibility. For a private repository, sign
in with an account that has access, or use authenticated `gh release download`.
An unauthenticated 404 does not establish that an asset is missing.

## Select your archive

For version `v0.1.0`:

| Computer | Archive |
| --- | --- |
| macOS Apple Silicon | `h2a_v0.1.0_darwin_arm64.tar.gz` |
| macOS Intel | `h2a_v0.1.0_darwin_amd64.tar.gz` |
| Linux x86-64 | `h2a_v0.1.0_linux_amd64.tar.gz` |
| Linux ARM64 | `h2a_v0.1.0_linux_arm64.tar.gz` |
| Windows Intel/AMD 64-bit | `h2a_v0.1.0_windows_amd64.tar.gz` |
| Windows ARM64 | `h2a_v0.1.0_windows_arm64.tar.gz` |

Use only assets actually attached to the release. Linux builds use glibc and are
tested on the Ubuntu runners listed in `release.yml`; Alpine/musl and older glibc
distributions are not claimed. `build_*.txt` records the exact compiler, source
commit and target. `verification_*.json` records the archive checksum and native
smoke checks. These binaries are unsigned, without macOS notarization or Windows
Authenticode signing. Checksums detect altered bytes; they do not independently
authenticate the publisher.

## macOS or Linux

Download both files into an empty directory. For example, Apple Silicon with the
GitHub CLI (replace `darwin_arm64` for another platform):

```sh
gh release download v0.1.0 --repo royderks/harness-to-api \
  --pattern 'h2a_v0.1.0_darwin_arm64.tar.gz' \
  --pattern 'checksums_v0.1.0_darwin_arm64.txt'
shasum -a 256 -c checksums_v0.1.0_darwin_arm64.txt
mkdir h2a-v0.1.0
tar -xzf h2a_v0.1.0_darwin_arm64.tar.gz -C h2a-v0.1.0
./h2a-v0.1.0/h2a version
```

On Linux use `sha256sum -c CHECKSUM_FILE` instead. Stop if verification fails.
Browser downloads work too; `gh` is optional for public releases. To put the binary
on your PATH:

```sh
mkdir -p "$HOME/.local/bin"
install -m 755 ./h2a-v0.1.0/h2a "$HOME/.local/bin/h2a"
export PATH="$HOME/.local/bin:$PATH"
h2a init --provider codex
```

Add the PATH line to your shell configuration if needed. Keep the terminal running;
Ctrl+C stops h2a. macOS may require approving the unsigned downloaded app under
Privacy & Security. Do not disable Gatekeeper globally.

## Windows PowerShell

Download the Windows archive and matching checksum. For x86-64:

```powershell
$archive = 'h2a_v0.1.0_windows_amd64.tar.gz'
$checksum = 'checksums_v0.1.0_windows_amd64.txt'
$expected = ((Get-Content -Raw $checksum).Trim() -split '\s+')[0]
if ((Get-FileHash $archive -Algorithm SHA256).Hash.ToLower() -ne $expected) {
    throw 'Checksum mismatch; do not run this archive'
}
New-Item -ItemType Directory h2a-v0.1.0
tar -xzf $archive -C h2a-v0.1.0
.\h2a-v0.1.0\h2a.exe version
.\h2a-v0.1.0\h2a.exe init --provider codex
```

The version must match the selected release. ARM64 users substitute `windows_arm64`
in both filenames. Windows may show an unsigned-app warning. For repeat use, move
the binary to a user-owned directory on PATH. Updates replace the executable after
stopping h2a; they do not require deleting gateway keys or harness logins.

## Build and verify a release

From a source checkout with Rust 1.89+ and Python 3.13:

```sh
cargo build --locked --example harness-fixture
scripts/release.sh v0.1.0
python3 scripts/check-release.py --version v0.1.0 \
  --target "$(rustc -vV | sed -n 's/^host: //p')" \
  --fixture "$PWD/target/debug/examples/harness-fixture"
```

The script writes an archive, SHA-256 checksum and build information to `dist/`.
Commit source and documentation changes first: tracked edits are rejected. Source
and packaged files come from a snapshot of that commit; untracked and ignored
checkout files are excluded. Cargo runs from that snapshot, so checkout-local
Cargo configuration is not discovered. This is not a hermetic or bit-for-bit
reproducible build: the installed toolchain, environment variables, user Cargo
configuration and system libraries remain trusted build inputs. Published builds
use fresh hosted runners. Build metadata identifies source and compiler, not every
ambient machine setting.
It refuses to reuse an existing staging directory. Remove only that task's staging
directory before an intentional rebuild. An optional second argument selects a
target triple; its toolchain/linker must exist. Native verification rejects a
runner with the wrong OS or architecture.

The release workflow tests all six targets on matching native runners, extracts
each final archive, checks its version/help, and exercises an offline authenticated
HTTP request, quota denial, durable count and revocation. No provider account is
used. PRs touching release files run the same checks without publishing. An approved
version tag creates a **draft** release only after all targets pass. Inspect the
evidence, download and check the attached bytes, then publish the draft explicitly.
Do not replace assets under an already published version; publish a new patch version.

Homebrew, npm and Windows package managers remain separate work under
[issue #13](https://github.com/royderks/harness-to-api/issues/13). No registry package
or automatic installer is implied by a GitHub release.
