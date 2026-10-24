#!/usr/bin/env bash
# Build (debug) and run the app. Extra arguments are passed through.
set -euo pipefail
cd "$(dirname "$0")"
exec cargo run -- "$@"
