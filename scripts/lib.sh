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

# find_idf_bootloader <target-triple> [target-dir]
# Prints the path to the bootloader esp-idf-sys built for <target-triple>.
# Prints nothing (status 0) when no bootloader has been built yet.
# Prints a diagnostic and exits 1 when several candidates exist, because picking
# one arbitrarily can flash a bootloader built from a stale sdkconfig.
find_idf_bootloader() {
    local idf_target="$1"
    local idf_dir="${2:-target}"
    local resolved
    if [[ "$idf_dir" = /* ]]; then
        resolved="$idf_dir"
    else
        resolved="$PWD/$idf_dir"
    fi
    # nullglob so a non-match yields an empty array rather than the literal pattern.
    shopt -s nullglob
    local candidates=( "$resolved/$idf_target/release/build"/esp-idf-sys-*/out/build/bootloader/bootloader.bin )
    shopt -u nullglob
    if [ ${#candidates[@]} -gt 1 ]; then
        printf 'Error: multiple IDF-built bootloaders for target "%s":\n' "$idf_target" >&2
        printf '  %s\n' "${candidates[@]}" >&2
        printf 'Run `just clean-idf` and rebuild so exactly one remains.\n' >&2
        exit 1
    fi
    if [ ${#candidates[@]} -eq 1 ]; then
        printf '%s\n' "${candidates[0]}"
    fi
}
