#!/usr/bin/env bash
# Build an optimized binary and copy it to ./dist/launcher-creator.
set -euo pipefail
cd "$(dirname "$0")"
cargo build --release
mkdir -p dist
cp target/release/launcher-creator dist/
echo "Built: $(pwd)/dist/launcher-creator"
