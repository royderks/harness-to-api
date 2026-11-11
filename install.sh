#!/bin/sh
# Keep the entry point last so a truncated download cannot start installation.
h2a_install() (
    set -eu
    version=v0.1.0
    fail() { printf 'h2a: %s\n' "$*" >&2; exit 1; }
    case "$(uname -s)" in Darwin) os=darwin;; Linux) os=linux;; *) fail 'Use install.ps1 on Windows; this OS is unsupported.';; esac
    case "$(uname -m)" in arm64|aarch64) arch=arm64;; x86_64|amd64) arch=amd64;; *) fail 'Supported architectures: x86-64 and ARM64.';; esac
    for command in curl tar install mktemp; do
        command -v "$command" >/dev/null 2>&1 || fail "Required command missing: $command"
    done
    if command -v sha256sum >/dev/null 2>&1; then checksum=sha256sum
    elif command -v shasum >/dev/null 2>&1; then checksum=shasum
    else fail 'Install sha256sum or shasum to verify the download.'; fi
    [ -n "${HOME:-}" ] || fail 'HOME must be set.'
    destination=${H2A_INSTALL_DIR:-"$HOME/.local/bin"}
    case "$destination" in /*) ;; *) fail 'Install directory must be an absolute path.';; esac
    temp=$(mktemp -d)
    staged=''
    trap 'rm -rf -- "$temp"; if [ -n "$staged" ]; then rm -f -- "$staged"; fi' EXIT
    trap 'exit 1' HUP INT TERM
    archive="h2a_${version}_${os}_${arch}.tar.gz"
    base="https://github.com/royderks/harness-to-api/releases/download/$version"
    curl --proto '=https' --tlsv1.2 -fsSL "$base/$archive" -o "$temp/$archive"
    curl --proto '=https' --tlsv1.2 -fsSL "$base/checksums_${version}_${os}_${arch}.txt" -o "$temp/checksum"
    expected=$(awk -v name="$archive" 'NF == 2 && $2 == name && length($1) == 64 && $1 !~ /[^0-9a-fA-F]/ {print $1}' "$temp/checksum")
    [ "${#expected}" -eq 64 ] || fail 'Invalid release checksum.'
    (
        cd "$temp"
        if [ "$checksum" = sha256sum ]; then
            printf '%s  %s\n' "$expected" "$archive" | sha256sum -c -
        else
            printf '%s  %s\n' "$expected" "$archive" | shasum -a 256 -c -
        fi
    ) || fail 'Checksum mismatch; installation stopped.'
    tar -xzf "$temp/$archive" -C "$temp" ./h2a
    [ -f "$temp/h2a" ] && [ ! -L "$temp/h2a" ] || fail 'Archive does not contain a regular h2a executable.'
    mkdir -p "$destination"
    [ ! -d "$destination/h2a" ] || fail 'Install target is a directory.'
    staged=$(mktemp "$destination/.h2a-install.XXXXXX")
    install -m 755 "$temp/h2a" "$staged"
    mv -f -- "$staged" "$destination/h2a"
    staged=''
    printf 'Installed h2a %s to %s/h2a\n' "$version" "$destination"
    if [ "${H2A_INSTALL_ONLY:-0}" != 1 ]; then
        rm -rf -- "$temp"
        trap - EXIT HUP INT TERM
        PATH="$destination:$PATH"
        export PATH
        exec "$destination/h2a" init </dev/tty
    fi
)
h2a_install
