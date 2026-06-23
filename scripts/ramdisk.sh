#!/usr/bin/env bash
set -euo pipefail
# ramdisk.sh — manage the build RAM disk at /Volumes/RustBuilds
# Usage: scripts/ramdisk.sh attach|detach
#
# Moves cargo's heavy target-dir onto a macOS RAM disk to spare the SSD and speed
# up esp-idf-sys rebuilds. Entirely optional: the justfile falls back to ./target
# when the disk is not mounted, so nothing here is required to build.
#
# Note: while the disk is mounted the justfile sets ESP_IDF_TOOLS_INSTALL_DIR=global,
# so the heavy ESP-IDF tools (~11 GB) install to ~/.espressif — only the per-build
# target dir lives on the RAM disk, so a 6 GB disk is plenty.

if [ "$(uname)" != "Darwin" ]; then
    printf 'error: ramdisk.sh requires macOS (hdiutil/diskutil not available)\n' >&2
    exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./lib.sh
. "$SCRIPT_DIR/lib.sh"

RAMDISK_NAME="RustBuilds"
# Size of the shared RAM disk in GiB.
#
# Set RUSTBUILDS_RAMDISK_SIZE_GB once (e.g. in ~/.zshrc) so every rustyfarian repo
# that shares /Volumes/RustBuilds agrees on the size — the disk is sized by whichever
# repo runs `just ramdisk attach` first after a reboot, so one global value keeps that
# deterministic. (The legacy RAMDISK_SIZE_GB is still honoured as a fallback.)
#
# Each esp-idf-sys target dir is ~1–2.5 GB, so size for the repos you build
# concurrently — e.g. RUSTBUILDS_RAMDISK_SIZE_GB=8 covers ~3 esp-idf repos.
RAMDISK_SIZE_GB="${RUSTBUILDS_RAMDISK_SIZE_GB:-${RAMDISK_SIZE_GB:-6}}"
if ! [[ "$RAMDISK_SIZE_GB" =~ ^[1-9][0-9]*$ ]]; then
    printf 'error: RUSTBUILDS_RAMDISK_SIZE_GB must be a positive integer (got: "%s")\n' "$RAMDISK_SIZE_GB" >&2
    exit 1
fi
RAMDISK_PATH="/Volumes/$RAMDISK_NAME"
BYTES_PER_GIB=$((1024 * 1024 * 1024))
BYTES_PER_SECTOR=512

case "${1:-}" in
    attach)
        if is_ramdisk_mounted "$RAMDISK_PATH"; then
            actual_gb=$(df -g "$RAMDISK_PATH" 2>/dev/null | awk 'NR==2 {print $2}')
            echo "RAM disk already attached at $RAMDISK_PATH (~${actual_gb:-?} GB; detach + re-attach to change size)"
        else
            # hdiutil ram:// expects size in 512-byte sectors.
            SECTORS=$(( RAMDISK_SIZE_GB * BYTES_PER_GIB / BYTES_PER_SECTOR ))
            DEV=$(hdiutil attach -nomount "ram://$SECTORS" | xargs)
            # HFS+ via a single `erasevolume` is the canonical format for an ephemeral
            # RAM device. APFS is deliberately avoided: it would add a container/volume
            # layer (snapshots, space sharing) for no benefit on a throwaway disk.
            diskutil erasevolume HFS+ "$RAMDISK_NAME" "$DEV"
            echo "RAM disk attached at $RAMDISK_PATH (${RAMDISK_SIZE_GB} GB)"
        fi
        # The per-project leaf dir (targets/idf/<project>) is created by cargo;
        # we just ensure the shared parent exists so siblings can coexist.
        mkdir -p "$RAMDISK_PATH/targets/idf"
        ;;
    detach)
        if is_ramdisk_mounted "$RAMDISK_PATH"; then
            hdiutil detach "$RAMDISK_PATH"
            echo "RAM disk detached."
        else
            echo "RAM disk not attached."
        fi
        ;;
    *)
        printf 'Usage: scripts/ramdisk.sh attach|detach\n' >&2
        exit 1
        ;;
esac
