#!/usr/bin/env bash
set -euo pipefail
# save-image.sh — build a chip target and save its merged whole-flash image to a file.
#
# Usage: scripts/save-image.sh <idf_{chip}_{name}> <image>
#   e.g. scripts/save-image.sh idf_c3_rgb_clock tmp/bench/idf_c3_rgb_clock.bin
#   FIRMWARE_FEATURES=<cargo features> saves a feature image, e.g. `ddi-lab` (see preflight.sh).
#
# Builds and validates through scripts/preflight.sh (partition check, image size,
# rollback-enabled IDF bootloader), then writes the image with scripts/flash-image.sh's
# save mode, so it carries the same bootloader and partition table a `just flash` writes.
# Touches no board.
#
# Inside this repo the image path must be one git ignores (tmp/…); outside the repo
# any path is accepted. The image carries the `.env` portal prefill baked in at build
# time (Wi-Fi SSID, broker host and port, MQTT user, and the DDI URL in lab images).
# No password or token is in it; those live in NVS.

example="${1:-}"
image="${2:-}"
if [ -z "$example" ] || [ -z "$image" ]; then
    printf 'Usage: %s <idf_{chip}_{name}> <image>  e.g. idf_c3_rgb_clock tmp/bench/idf_c3_rgb_clock.bin\n' "$0" >&2
    exit 2
fi
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd -P)"

# Resolve the image to an absolute, physical path before judging it, so the verdict
# does not depend on the working directory or on `..` segments. The file and its
# parents may not exist yet: resolve the nearest existing ancestor, keep the rest.
case "$image" in
    /*) abs="$image" ;;
    *) abs="$PWD/$image" ;;
esac
dir="$(dirname "$abs")"
rest="$(basename "$abs")"
while [ ! -d "$dir" ]; do
    rest="$(basename "$dir")/$rest"
    dir="$(dirname "$dir")"
done
case "/$rest/" in
    */../*|*/./*)
        printf 'save-image: %s has a ./ or ../ segment below a directory that does not exist yet; give a plain path\n' "$image" >&2
        exit 1
        ;;
esac
abs="$(cd "$dir" && pwd -P)/$rest"

# Inside this repo only a path git positively ignores is accepted: 1 (tracked or
# trackable), 128 (git error, e.g. dubious ownership) and 127 (no git) all refuse.
case "$abs" in
    "$ROOT_DIR"/*)
        rc=0
        git -C "$ROOT_DIR" check-ignore -q -- "$abs" 2>/dev/null || rc=$?
        if [ "$rc" -ne 0 ]; then
            if [ "$rc" -eq 1 ]; then
                reason="would be tracked by git"
            else
                reason="cannot be confirmed as git-ignored (git exit $rc)"
            fi
            printf 'save-image: %s %s, and the image holds the .env portal prefill (SSID, broker host, MQTT user); save it under tmp/ or outside the repo\n' "$image" "$reason" >&2
            exit 1
        fi
        ;;
esac

# The scripts below read partitions.csv and target/ relative to the repo root.
cd "$ROOT_DIR"
mkdir -p "$(dirname "$abs")"
bl="$("$SCRIPT_DIR/preflight.sh" "$example")"
exec "$SCRIPT_DIR/flash-image.sh" --save-image "$abs" "$example" "$bl"
