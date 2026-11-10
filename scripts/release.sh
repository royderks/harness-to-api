#!/bin/sh
set -eu
version=${1:?Usage: scripts/release.sh v0.1.0 [target-triple]}
case "$version" in *[!a-zA-Z0-9._-]*|'') echo 'Invalid version' >&2; exit 1;; esac
target=${2:-$(rustc -vV | sed -n 's/^host: //p')}
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
H2A_VERSION="$version" cargo build --locked --release --target "$target" --bin h2a
name="h2a_${version}_${platform}"
folder="dist/$name"
# Refuse stale packaging directories so old files cannot leak into a new archive.
if [ -e "$folder" ]; then
  echo "Archive staging directory already exists: $folder; remove it before rebuilding" >&2
  exit 1
fi
mkdir -p "$folder"
cp "target/$target/release/$executable" "$folder/"
cp LICENSE README.md CONTRIBUTING.md "$folder/"
cp -R docs examples "$folder/"
mkdir -p "$folder/demo"
cp demo/*.py demo/*.sh "$folder/demo/"
{
  printf 'version: %s\ntarget: %s\nsource_commit: %s\n' "$version" "$target" "$(git rev-parse HEAD)"
  rustc -vV
  cargo --version
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
