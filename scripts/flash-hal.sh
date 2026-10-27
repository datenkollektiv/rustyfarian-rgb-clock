#!/usr/bin/env bash
# flash-hal.sh — flash the bare-metal esp-hal image for a chip and open the monitor.
# Usage: flash-hal.sh <chip> <hal-target-dir> [idf-target-dir]
# Reuses the IDF-built v5.3.3 bootloader when one exists; otherwise espflash falls
# back to its bundled bootloader. Uses espflash's default partition table (app at
# 0x10000), not partitions.csv: the hal tier has no OTA yet.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./lib.sh
. "$SCRIPT_DIR/lib.sh"

if [ $# -lt 2 ]; then
    printf 'Usage: %s <chip> <hal-target-dir> [idf-target-dir]\n' "$0" >&2
    exit 2
fi
chip="$1"
hal_dir="$2"
idf_dir="${3:-target}"
chip_env="$("$SCRIPT_DIR/hal-chip-env.sh" "$chip")"
eval "$chip_env"
elf="$hal_dir/$HAL_TARGET/release/rustyfarian-esp-hal-rgb-clock"

bootloader_args=()
bl="$(find_idf_bootloader "$IDF_TARGET" "$idf_dir")"
if [ -n "$bl" ]; then
    printf 'Using IDF bootloader %s\n' "$bl"
    bootloader_args=(--bootloader "$bl")
else
    printf 'Warning: no IDF %s bootloader found; espflash uses its bundled one.\n' "$chip" >&2
    printf '(optional: `just build idf_%s_rgb_clock` to reuse the IDF v5.3.3 bootloader)\n' "$chip" >&2
fi

port="$("$SCRIPT_DIR/detect-port.sh")"
port_args=()
if [ -n "$port" ]; then
    port_args=(--port "$port")
fi

exec espflash flash --chip "$ESPFLASH_CHIP" --ignore-app-descriptor --flash-size 4mb \
    ${bootloader_args[@]+"${bootloader_args[@]}"} \
    ${port_args[@]+"${port_args[@]}"} \
    --monitor "$elf"
