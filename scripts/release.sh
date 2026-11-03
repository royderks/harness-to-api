#!/bin/sh
set -eu
version=${1:?Usage: scripts/release.sh v0.1.0}
case "$version" in *[!a-zA-Z0-9._-]*|'') echo 'Invalid version' >&2; exit 1;; esac
mkdir -p dist
for platform in darwin/arm64 darwin/amd64 linux/arm64 linux/amd64 windows/arm64 windows/amd64; do
  target_os=${platform%/*}
  target_arch=${platform#*/}
  name="h2a_${version}_${target_os}_${target_arch}"
  folder="dist/$name"
  mkdir -p "$folder"
  executable=h2a
  if [ "$target_os" = windows ]; then executable=h2a.exe; fi
  CGO_ENABLED=0 GOOS="$target_os" GOARCH="$target_arch" go build -trimpath -ldflags "-s -w -X main.version=$version" -o "$folder/$executable" ./cmd/h2a
  cp LICENSE README.md "$folder/"
  cp -R docs examples "$folder/"
  tar -czf "dist/$name.tar.gz" -C "$folder" .
done
cd dist
if command -v sha256sum >/dev/null 2>&1; then
  sha256sum "h2a_${version}_"*.tar.gz > "checksums_${version}.txt"
else
  shasum -a 256 "h2a_${version}_"*.tar.gz > "checksums_${version}.txt"
fi
