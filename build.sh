#!/usr/bin/env bash
# Build an optimized binary into ./dist/launcher-creator, plus a tarball
# (which keeps the executable bit) for distribution.
set -euo pipefail
cd "$(dirname "$0")"
cargo build --release
rm -rf dist
mkdir -p dist/launcher-creator
cp target/release/launcher-creator assets/launcher-creator.svg dist/launcher-creator/
tar -czf dist/launcher-creator-linux-x86_64.tar.gz -C dist launcher-creator
echo "Built: $(pwd)/dist/launcher-creator/launcher-creator"
echo "Tarball: $(pwd)/dist/launcher-creator-linux-x86_64.tar.gz"
