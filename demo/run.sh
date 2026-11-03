#!/bin/sh
# Run from any directory. Set H2A_BIN to skip the Cargo build.
set -eu
cd "$(dirname "$0")/.."
exec python3 demo/run.py "$@"
