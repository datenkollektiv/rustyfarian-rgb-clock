#!/usr/bin/env bash
set -euo pipefail
# ramdisk.sh — manage the shared build RAM disk at /Volumes/RustBuilds.
# Usage: scripts/ramdisk.sh attach|detach|prune [days]
# Canonical copy: rustyfarian-ci/scripts/ (conventions/just.md); repos copy it byte for byte.
#
# The RAM disk is shared across all rustyfarian repos that build for device targets;
# each repo namespaces its artifacts under targets/{hal,idf,avr}/<repo-name>.
# Attaching is optional — without it, builds fall back to target/ on disk.
#
# `prune [days]` removes every targets/{hal,idf,avr}/<repo> tree that holds no file
# newer than <days> days (default 7, minimum 1), so idle trees from other repos
# stop filling the shared disk. To wipe the whole disk, detach and re-attach.
# Trees a running cargo build holds open are skipped; a build that starts mid-prune is not protected.

if [ "$(uname)" != "Darwin" ]; then
    printf 'error: ramdisk.sh requires macOS (hdiutil/diskutil not available)\n' >&2
    exit 1
fi

RAMDISK_NAME="RustBuilds"
# Interpreted as GiB (1024^3 bytes). Set RUSTBUILDS_RAMDISK_SIZE_GB to share one
# size across all rustyfarian repos; the legacy RAMDISK_SIZE_GB is still honoured.
RAMDISK_SIZE_GB="${RUSTBUILDS_RAMDISK_SIZE_GB:-${RAMDISK_SIZE_GB:-6}}"
RAMDISK_PATH="/Volumes/$RAMDISK_NAME"
BYTES_PER_GIB=$((1024 * 1024 * 1024))
BYTES_PER_SECTOR=512

# A live mounted volume, not a stale /Volumes/<name> directory left by an earlier detach.
is_mounted() {
    diskutil info "$RAMDISK_PATH" >/dev/null 2>&1
}

case "${1:-}" in
    attach)
        if is_mounted; then
            echo "RAM disk already attached at $RAMDISK_PATH"
        else
            # Only attach uses the size; detach and prune must work with a bad setting.
            if ! [[ "$RAMDISK_SIZE_GB" =~ ^[1-9][0-9]*$ ]]; then
                printf 'error: RAM disk size must be a positive integer — set RUSTBUILDS_RAMDISK_SIZE_GB (got: "%s")\n' "$RAMDISK_SIZE_GB" >&2
                exit 1
            fi
            # hdiutil ram:// expects size in 512-byte sectors.
            SECTORS=$(( RAMDISK_SIZE_GB * BYTES_PER_GIB / BYTES_PER_SECTOR ))
            DEV=$(hdiutil attach -nomount "ram://$SECTORS" | xargs)
            # HFS+ is used deliberately: `diskutil erasevolume HFS+` is the one-step
            # formatter for hdiutil RAM devices on every macOS version in use; APFS
            # needs separate container steps and gains nothing for a throwaway cache.
            diskutil erasevolume HFS+ "$RAMDISK_NAME" "$DEV"
            echo "RAM disk attached at $RAMDISK_PATH (${RAMDISK_SIZE_GB} GB)"
        fi
        mkdir -p "$RAMDISK_PATH/targets/hal" "$RAMDISK_PATH/targets/idf" "$RAMDISK_PATH/targets/avr"
        ;;
    detach)
        if is_mounted; then
            hdiutil detach "$RAMDISK_PATH"
            echo "RAM disk detached."
        else
            echo "RAM disk not attached."
        fi
        ;;
    prune)
        DAYS="${2:-7}"
        if ! [[ "$DAYS" =~ ^[1-9][0-9]*$ ]]; then
            printf 'error: prune expects a number of days >= 1 (got: "%s")\n' "$DAYS" >&2
            printf 'to wipe the whole RAM disk use: just ramdisk detach && just ramdisk attach\n' >&2
            exit 1
        fi
        if ! is_mounted; then
            echo "RAM disk not attached."
            exit 0
        fi
        PRUNED=0
        # rm -rf follows a symlinked parent: never prune through targets/ or a namespace that points elsewhere.
        for dir in "$RAMDISK_PATH/targets" "$RAMDISK_PATH"/targets/{hal,idf,avr}; do
            if [ -L "$dir" ]; then
                printf 'error: %s is a symlink; refusing to prune outside the RAM disk\n' "${dir#"$RAMDISK_PATH"/}" >&2
                exit 1
            fi
        done
        for tree in "$RAMDISK_PATH"/targets/hal/* "$RAMDISK_PATH"/targets/idf/* "$RAMDISK_PATH"/targets/avr/*; do
            [ -d "$tree" ] && [ ! -L "$tree" ] || continue
            # A failed scan must not read as "stale": skip the tree instead.
            if ! recent=$(find "$tree" -type f -mtime "-${DAYS}" -print -quit); then
                printf 'skipped %s (scan failed)\n' "${tree#"$RAMDISK_PATH"/}" >&2
                continue
            fi
            [ -z "$recent" ] || continue
            # Cargo holds .cargo-lock open for a whole build; a no-op build writes nothing,
            # so an old tree can still be in use.
            holders=$(find "$tree" -maxdepth 3 -name .cargo-lock -exec lsof -t {} + 2>/dev/null || true)
            if [ -n "$holders" ]; then
                printf 'skipped %s (build in progress)\n' "${tree#"$RAMDISK_PATH"/}" >&2
                continue
            fi
            size=$(du -sh "$tree" 2>/dev/null | awk '{ print $1 }') || size='?'
            rm -rf "$tree"
            echo "pruned ${tree#"$RAMDISK_PATH"/} ($size, no file newer than ${DAYS} days)"
            PRUNED=$((PRUNED + 1))
        done
        if [ "$PRUNED" -eq 0 ]; then
            echo "nothing to prune: every target tree has files newer than ${DAYS} days"
        fi
        df -h "$RAMDISK_PATH" | tail -1 | awk '{ printf "RAM disk %s used of %s, %s free\n", $3, $2, $4 }'
        ;;
    *)
        printf 'Usage: scripts/ramdisk.sh attach|detach|prune [days]\n' >&2
        exit 1
        ;;
esac
