#!/usr/bin/env bash
set -euo pipefail
# flash-image.sh — flash an already-built, already-validated image. Nothing else.
# Usage: scripts/flash-image.sh <target> <bootloader.bin> [port]
#        scripts/flash-image.sh --save-image <image> <target> <bootloader.bin>
#   target:     idf_{chip}_{name}  e.g. idf_c6_rgb_clock
#   bootloader: path printed by scripts/preflight.sh
#   port:       serial device; omit to use the single USB serial port detected by
#               scripts/detect-port.sh ($ESPFLASH_PORT wins). Refuses to run
#               without exactly one port, so a chip check and the flash cannot
#               land on two different boards.
#
# DRY_RUN=1 prints the commands that would touch the device and runs none of them.
#
# --save-image <image> flashes nothing: it writes the merged whole-flash image
# (IDF bootloader at 0x0, partition table at 0x8000, app at ota_0, 0xFF-padded to
# FLASH_SIZE from chip-env.sh) to <image>, with the same bootloader, partition table
# and app arguments as the flash. No port is resolved and no board is probed; the
# remote bench reads the chip from the image's bootloader header instead.
# A flag, not an environment variable: a variable left in the shell or `.env` would
# turn `just flash-baseline` into erase-then-save, leaving the board blank.
#
# Deliberately performs no build and no validation: it exists so a caller that has
# already erased the chip can flash artifacts validated BEFORE the erase. Building
# after the erase would mean a compile error leaves the device wiped and unflashed.
# Use scripts/flash.sh for the ordinary (non-destructive) build-then-flash path.
#
# `--ignore-app-descriptor` disables espflash's own chip-model check, so this
# script asks the attached chip what it is (`espflash board-info`) and refuses when
# the answer differs from the requested target: nothing else stops a C6 image
# from being written onto an attached C3.

save=""
if [ "${1:-}" = "--save-image" ]; then
    save="${2:-}"
    if [ -z "$save" ]; then
        printf 'Usage: %s --save-image <image> <idf_{chip}_{name}> <bootloader.bin>\n' "$0" >&2
        exit 2
    fi
    shift 2
fi
example="${1:-}"
bl="${2:-}"
if [ -z "$example" ] || [ -z "$bl" ]; then
    printf 'Usage: %s <idf_{chip}_{name}> <bootloader.bin> [port]\n' "$0" >&2
    exit 2
fi
port="${3:-}"
dry_run="${DRY_RUN:-0}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
eval "$("$SCRIPT_DIR/chip-env.sh" "$example")"

target_dir="${CARGO_TARGET_DIR:-target}"
app="$target_dir/$TARGET/release/rustyfarian-rgb-clock"

for artifact in "$app" "$bl"; do
    if [ ! -f "$artifact" ]; then
        printf 'Error: expected artifact is missing: %s\n' "$artifact" >&2
        printf 'Run scripts/preflight.sh %s first.\n' "$example" >&2
        exit 1
    fi
done

# run <cmd...> — execute, or under DRY_RUN=1 print the exact command instead.
run() {
    if [ "$dry_run" = "1" ]; then
        printf '[dry-run] would run:'
        printf ' %q' "$@"
        printf '\n'
    else
        "$@"
    fi
}

# Shared by the flash and the saved image, so both write the same bootloader and table.
image_args=(--partition-table partitions.csv --bootloader "$bl" --ignore-app-descriptor)

if [ -n "$save" ]; then
    # A leftover image must not pass the check below or reach the bench as the new one.
    if [ "$dry_run" != "1" ]; then
        rm -f "$save"
    fi
    printf 'Saving the merged image of %s (%s, %s flash) with IDF bootloader %s to %s...\n' \
        "$example" "$MCU" "$FLASH_SIZE" "$bl" "$save"
    run espflash save-image --chip "$MCU" --merge --flash-size "$FLASH_SIZE" "${image_args[@]}" "$app" "$save"
    if [ "$dry_run" != "1" ] && [ ! -s "$save" ]; then
        printf 'Error: espflash save-image left no image at %s.\n' "$save" >&2
        exit 1
    fi
    exit 0
fi

if [ -z "$port" ]; then
    port="$("$SCRIPT_DIR/detect-port.sh")"
fi
if [ -z "$port" ]; then
    printf 'Error: no unique serial port detected — refusing to flash.\n' >&2
    printf 'Set ESPFLASH_PORT=/dev/cu.usbmodemXXXX (macOS) or /dev/ttyUSBn (Linux), or detach the other boards.\n' >&2
    exit 1
fi

if [ "$dry_run" = "1" ]; then
    printf '[dry-run] skipping `espflash board-info --port %s` (no device command runs under DRY_RUN); a real run refuses any chip other than %s here\n' "$port" "$MCU"
else
    printf 'Checking the chip on %s is an %s...\n' "$port" "$MCU"
    if ! info="$(espflash board-info --port "$port")"; then
        printf 'Error: espflash board-info failed on %s — is the board attached and not held by a monitor?\n' "$port" >&2
        exit 1
    fi
    chip="$(printf '%s\n' "$info" | sed -n 's/^Chip type:[[:space:]]*\([a-z0-9]*\).*/\1/p' | head -1)"
    if [ -z "$chip" ]; then
        printf 'Error: could not read "Chip type:" from espflash board-info output:\n%s\n' "$info" >&2
        exit 1
    fi
    if [ "$chip" != "$MCU" ]; then
        printf 'Error: the board on %s is an %s, but %s targets %s — refusing to flash.\n' \
            "$port" "$chip" "$example" "$MCU" >&2
        printf 'Pass the matching target (e.g. just flash idf_%s_rgb_clock) or attach the right board.\n' \
            "${chip#esp32}" >&2
        exit 1
    fi
    printf 'Chip check OK: %s\n' "$chip"
fi

printf 'Flashing %s with IDF bootloader %s...\n' "$example" "$bl"
run espflash flash --port "$port" "${image_args[@]}" "$app"
