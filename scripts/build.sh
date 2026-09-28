#!/usr/bin/env bash
set -euo pipefail
# build.sh — build the firmware for a named chip target
# Usage: scripts/build.sh <target> [extra cargo args...]
#   target: idf_{chip}_{name}  e.g. idf_c6_rgb_clock, idf_c3_rgb_clock
#   extra cargo args: passed through to `cargo build --release --target`
#     e.g. scripts/build.sh idf_c6_rgb_clock --features unhealthy

# Not `${1:?...}`: a `}` inside the message (from `{chip}`) closes the parameter
# expansion early, so the usage text leaks into the variable's value.
example="${1:-}"
if [ -z "$example" ]; then
    printf 'Usage: %s <idf_{chip}_{name}> [extra cargo args...]  e.g. idf_c6_rgb_clock\n' "$0" >&2
    exit 2
fi
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
eval "$("$SCRIPT_DIR/chip-env.sh" "$example")"
shift

printf 'Building %s (MCU=%s, target=%s)...\n' "$example" "$MCU" "$TARGET"
MCU="$MCU" cargo build --release --target "$TARGET" "$@"
