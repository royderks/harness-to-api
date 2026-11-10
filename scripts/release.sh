#!/bin/sh
set -eu
version=${1:?Usage: scripts/release.sh v0.1.0 [target-triple]}
case "$version" in *[!a-zA-Z0-9._-]*|'') echo 'Invalid version' >&2; exit 1;; esac
if ! git diff --quiet HEAD --; then
  echo 'Commit tracked changes before building a release' >&2
  exit 1
fi
source_commit=$(git rev-parse HEAD)
repo=$(pwd)
snapshot=$(mktemp -d)
trap 'rm -rf -- "$snapshot"' EXIT HUP INT TERM
# Build and package only committed inputs, excluding ignored files and local extras.
git archive --output="$snapshot/source.tar" "$source_commit"
tar -xf "$snapshot/source.tar" -C "$snapshot"
target=${2:-$(cd "$snapshot" && rustc -vV | sed -n 's/^host: //p')}
case "$target" in
  aarch64-apple-darwin) platform=darwin_arm64;;
  x86_64-apple-darwin) platform=darwin_amd64;;
  aarch64-unknown-linux-gnu) platform=linux_arm64;;
  x86_64-unknown-linux-gnu) platform=linux_amd64;;
  aarch64-pc-windows-msvc) platform=windows_arm64;;
  x86_64-pc-windows-msvc) platform=windows_amd64;;
  *) echo 'Unsupported release target' >&2; exit 1;;
esac
executable=h2a
case "$platform" in windows_*) executable=h2a.exe;; esac
name="h2a_${version}_${platform}"
folder="dist/$name"
# Refuse stale packaging directories so old files cannot leak into a new archive.
if [ -e "$folder" ]; then
  echo "Archive staging directory already exists: $folder; remove it before rebuilding" >&2
  exit 1
fi
(
  cd "$snapshot"
  H2A_VERSION="$version" cargo build --target-dir "$repo/target" --locked --release --target "$target" --bin h2a
)
mkdir -p "$folder"
cp "target/$target/release/$executable" "$folder/"
cp "$snapshot/LICENSE" "$snapshot/README.md" "$snapshot/CONTRIBUTING.md" "$folder/"
cp -R "$snapshot/docs" "$snapshot/examples" "$folder/"
mkdir -p "$folder/demo"
cp "$snapshot"/demo/*.py "$snapshot"/demo/*.sh "$folder/demo/"
{
  printf 'version: %s\ntarget: %s\nsource_commit: %s\n' "$version" "$target" "$source_commit"
  (cd "$snapshot" && rustc -vV && cargo --version)
  printf 'build_environment: ambient toolchain, environment and user Cargo configuration; not hermetic\n'
  printf 'signature: unsigned\n'
} > "dist/build_${version}_${platform}.txt"
cp "dist/build_${version}_${platform}.txt" "$folder/BUILD.txt"
tar -czf "dist/$name.tar.gz" -C "$folder" .
cd dist
if command -v sha256sum >/dev/null 2>&1; then
  sha256sum "$name.tar.gz" > "checksums_${version}_${platform}.txt"
else
  shasum -a 256 "$name.tar.gz" > "checksums_${version}_${platform}.txt"
fi
