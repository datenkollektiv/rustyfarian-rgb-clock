#!/usr/bin/env bash
set -euo pipefail
# ramdisk-mounted.sh — print "true" if <path> is a live mounted volume, else "false".
# Used by the justfile via `shell(...)` to choose between the RAM disk and ./target.
# Always exits 0 so a non-Darwin host or missing path produces "false" without aborting just.

if [ $# -lt 1 ]; then
    printf 'Usage: %s <path>\n' "$0" >&2
    exit 2
fi

if [ "$(uname)" != "Darwin" ]; then
    echo false
    exit 0
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./lib.sh
. "$SCRIPT_DIR/lib.sh"

if is_ramdisk_mounted "$1"; then
    echo true
else
    echo false
fi
