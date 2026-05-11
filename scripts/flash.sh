#!/usr/bin/env bash
set -euo pipefail
# flash.sh — build and flash the firmware for a named chip target
# Usage: scripts/flash.sh <target>
#   target: idf_{chip}_{name}  e.g. idf_c6_rgb_clock, idf_c3_rgb_clock
#
# Does not open the serial monitor — run `just monitor` after flashing.

example="${1:?Usage: scripts/flash.sh <idf_{chip}_{name}>  e.g. idf_c6_rgb_clock}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
eval "$("$SCRIPT_DIR/chip-env.sh" "$example")"

printf 'Building and flashing %s (MCU=%s, target=%s)...\n' "$example" "$MCU" "$TARGET"
MCU="$MCU" cargo espflash flash --release --target "$TARGET" --partition-table partitions.csv
