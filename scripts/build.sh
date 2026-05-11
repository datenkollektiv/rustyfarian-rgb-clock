#!/usr/bin/env bash
set -euo pipefail
# build.sh — build the firmware for a named chip target
# Usage: scripts/build.sh <target>
#   target: idf_{chip}_{name}  e.g. idf_c6_rgb_clock, idf_c3_rgb_clock

example="${1:?Usage: scripts/build.sh <idf_{chip}_{name}>  e.g. idf_c6_rgb_clock}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
eval "$("$SCRIPT_DIR/chip-env.sh" "$example")"

printf 'Building %s (MCU=%s, target=%s)...\n' "$example" "$MCU" "$TARGET"
MCU="$MCU" cargo build --release --target "$TARGET"
