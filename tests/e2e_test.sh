#!/usr/bin/env bash
# E2E smoke tests: run the release binary end-to-end.
set -euo pipefail

BIN="${BIN:-target/release/pi-rs}"

echo "e2e: binary reports version"
OUT="$("$BIN")"
echo "  $OUT"
[[ "$OUT" == pi-rs\ * ]] || { echo "ERROR: unexpected output"; exit 1; }

echo "All e2e tests passed!"
