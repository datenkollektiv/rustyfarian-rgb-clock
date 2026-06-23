#!/usr/bin/env bash
# lib.sh — shared helper functions for scripts/
# Source this file; do not execute it directly.

if [ "${BASH_SOURCE[0]}" = "$0" ]; then
    printf 'Error: lib.sh must be sourced, not executed directly.\n' >&2
    exit 2
fi

# is_ramdisk_mounted <path>
# Returns 0 if <path> is a live mounted volume on macOS, 1 otherwise.
# Uses `diskutil info` rather than parsing `mount`, so a stale /Volumes/<name>
# directory (left over from a failed detach) is correctly reported as unmounted.
is_ramdisk_mounted() {
    local path="$1"
    [ -n "$path" ] || return 1
    diskutil info "$path" >/dev/null 2>&1
}
