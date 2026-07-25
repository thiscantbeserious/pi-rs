#!/usr/bin/env bash
# E2E smoke + chaos gate (AGENTS.md). Two parts:
#   1. Release binary smoke: the pi-rs binary builds and reports its version.
#   2. kill -9 survival: the host supervisor survives a killed host and
#      respawns. This is Phase 1 exit gate 2. The test runs the Rust
#      integration case `supervisor_survives_kill9_and_respawns`, which builds
#      and spawns the mock-host binary (crates/pi-core/src/bin/mock_host.rs)
#      via the CARGO_BIN_EXE_mock-host harness env var.
#
# The host chaos path is exercised against the mock-host binary because the
# real Deno-compiled host (host/main.ts) is not wired into the pi-rs binary
# until Phase 3. See docs/plans/step-6-ci-deno-conformance.md.
set -euo pipefail

BIN="${BIN:-target/release/pi-rs}"

echo "e2e: build release binary"
cargo build --release -p pi-rs

echo "e2e: binary reports version"
OUT="$("$BIN")"
echo "  $OUT"
[[ "$OUT" == pi-rs\ * ]] || {
	echo "ERROR: unexpected output"
	exit 1
}

echo "e2e: host supervisor survives kill -9 and respawns"
cargo test -p pi-core --test supervisor_integration supervisor_survives_kill9_and_respawns -- --nocapture

echo "All e2e tests passed!"
