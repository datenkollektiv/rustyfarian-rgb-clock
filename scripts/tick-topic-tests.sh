#!/usr/bin/env bash
set -euo pipefail
# tick-topic-tests.sh — host unit tests for build_support/tick_topic.rs.
#
# The tick-topic override is resolved in build.rs, which cargo never tests, and
# the firmware crate cannot `cargo test` on the host. The logic therefore lives
# in one self-contained file that this script compiles with plain `rustc --test`.
#
# Quiet on success — one summary line, since this runs on every `just verify`.
# Pass -v (or set VERBOSE=1) to list each case.
#
# Usage: scripts/tick-topic-tests.sh [-v]

verbose="${VERBOSE:-0}"
if [ "${1:-}" = "-v" ]; then
    verbose=1
fi

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
bin="$ROOT_DIR/tmp/tick-topic-tests"
mkdir -p "$ROOT_DIR/tmp"

rustc --edition 2021 --test "$ROOT_DIR/build_support/tick_topic.rs" -o "$bin"
if [ "$verbose" = "1" ]; then
    "$bin"
else
    output="$("$bin" 2>&1)" || { printf '%s\n' "$output" >&2; exit 1; }
    summary="$(printf '%s\n' "$output" | grep -E '^test result:' || true)"
    if [ -z "$summary" ]; then
        printf 'tick-topic: no "test result:" line in the test output:\n%s\n' "$output" >&2
        exit 1
    fi
    printf 'tick-topic: %s\n' "$summary"
fi
