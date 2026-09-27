#!/usr/bin/env bash
set -euo pipefail
# flash.sh — build and flash the firmware for a named chip target
# Usage: scripts/flash.sh <target> [port]
#   target: idf_{chip}_{name}  e.g. idf_c6_rgb_clock, idf_c3_rgb_clock
#   port:   optional serial device; omit to let espflash choose
#
# Does not open the serial monitor — run `just monitor` after flashing.
#
# Building and validating happens in scripts/preflight.sh, which also refuses a
# bootloader built without CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE=y. The IDF-built
# bootloader is then passed explicitly: espflash 4.x bundles an ESP-IDF v5.5.1
# bootloader whose 32 KB MMU page size mismatches this project's v5.3.3 app, and
# only the IDF-built one carries this repo's sdkconfig.
# See docs/project-lore.md "OTA & Firmware Update".
#
# CAUTION: `--ignore-app-descriptor` also disables espflash's chip-model check, so
# pass `port` (or attach only the intended board) when more than one ESP32 is
# connected. Nothing here will refuse a chip mismatch for you.

# Not `${1:?...}`: a `}` inside the message (from `{chip}`) closes the parameter
# expansion early, so the usage text leaks into the variable's value.
example="${1:-}"
if [ -z "$example" ]; then
    printf 'Usage: %s <idf_{chip}_{name}> [port]  e.g. idf_c6_rgb_clock\n' "$0" >&2
    exit 2
fi
port="${2:-}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
eval "$("$SCRIPT_DIR/chip-env.sh" "$example")"

# Builds, checks the app exists, resolves exactly one IDF bootloader, and verifies
# rollback is compiled into it. Exits non-zero with its own diagnostics on failure.
bl="$("$SCRIPT_DIR/preflight.sh" "$example")"

# Flash-only step, shared with `just flash-baseline` so the destructive path can
# reuse artifacts validated before its erase instead of rebuilding after it.
exec "$SCRIPT_DIR/flash-image.sh" "$example" "$bl" "$port"
