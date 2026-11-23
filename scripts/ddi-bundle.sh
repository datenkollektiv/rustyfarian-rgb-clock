#!/usr/bin/env bash
set -euo pipefail
# ddi-bundle.sh — build the lab DDI image with a version baked in and write the
# fleet's release bundle for it.
#
# Usage: scripts/ddi-bundle.sh <idf_{chip}_{name}> <version>
#   version: MAJOR.MINOR.PATCH, decimal, no leading zeros, each part at most 65535
#
# Writes four files to tmp/ddi-bundle/ (git-ignored):
#   <name>.bin           the app image, exactly what the device downloads into a slot
#   <name>.bin.sha256    64 lowercase hex characters, the sha256 of <name>.bin
#   <name>.bin.version   the version
#   <name>.bin.chip      the ESP-IDF target name, e.g. esp32c6
# <name> is <example>-v<version>, or <example>-unhealthy-v<version> for the demo
# image. There is no <name>.bin.sig: the fleet's ingest refuses one until image
# signing exists.
#
# Before the sidecars are written the image is checked the way the fleet server
# checks an ingest: it must fit the OTA slot, byte 0 must be 0xE9 (the ESP app-image
# magic) and the little-endian u16 at offset 12 must be the chip id. A refused image
# leaves no bundle file behind, so it cannot be uploaded by mistake.
#
# The image is not device-specific: Wi-Fi, MQTT and the DDI token come from the
# provisioning portal. Only DDI_BASE_URL (from .env or the shell) is compiled in.
#
# Environment:
#   DDI_VARIANT=lab|unhealthy   lab (default) builds `--features ddi-lab`; unhealthy
#                               builds the demo image that never passes its health check
#   TICK_TOPIC_OVERRIDE=<topic> read by build.rs: the image subscribes to that topic
#                               instead of `tick`, so it fails its health check with the
#                               publisher still running (demo; do not leave set)
#   DDI_BUNDLE_ELF=<file>       bundle this prebuilt ELF instead of building (tests)
#   DDI_BUNDLE_OUT=<dir>        write the bundle here instead of tmp/ddi-bundle (tests)
#   DDI_BUNDLE_LOCK=<dir>       the run lock, instead of tmp/ddi-bundle.lock (tests)
#   DDI_BUNDLE_LOCK_TIMEOUT=<s> how long to wait for another run's lock (default 1800)
#
# One run at a time: every run builds into the same release ELF, so a second run
# could replace it between the first run's build and its export, and the first bundle
# would carry the second image under the first version, with a matching checksum.
# A lock (a directory holding the holder's PID; macOS has no flock) spans the cleanup
# of the old bundle, the build and the export. A lock whose holder has exited is taken
# over. A plain `just build` or `build-lab` in another terminal writes the same ELF and
# is not locked out: do not run one while a bundle is being made.

usage() {
    printf 'Usage: %s <idf_{chip}_{name}> <version>\n  version: MAJOR.MINOR.PATCH, no leading zeros, each part at most 65535, e.g. 0.3.1\n' "$0" >&2
}

if [ $# -ne 2 ]; then
    usage
    exit 2
fi
example="$1"
version="$2"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
# Captured first: a failing command substitution inside `eval` would not stop the script.
chip_env="$("$SCRIPT_DIR/chip-env.sh" "$example")" || exit 1
eval "$chip_env"

# The chip id the ESP app-image header carries at offset 12 (esp_chip_id_t).
case "$MCU" in
    esp32c3) chip_id=5 ;;
    esp32c6) chip_id=13 ;;
    *) printf 'Error: no app-image chip id known for %s\n' "$MCU" >&2; exit 1 ;;
esac

# [[ =~ ]] matches the whole string, so a newline cannot smuggle a second line past the check.
version_re='^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$'
if [[ ! "$version" =~ $version_re ]]; then
    printf 'Error: version must be MAJOR.MINOR.PATCH (decimal, no sign, no leading zeros), got "%s".\n' "$version" >&2
    usage
    exit 1
fi
IFS=. read -r major minor patch <<< "$version"
for part in "$major" "$minor" "$patch"; do
    if [ "${#part}" -gt 5 ] || [ "$part" -gt 65535 ]; then
        printf 'Error: version part "%s" in "%s" is above 65535.\n' "$part" "$version" >&2
        exit 1
    fi
done

variant="${DDI_VARIANT:-lab}"
case "$variant" in
    lab)       features="ddi-lab";   name="${example}-v${version}" ;;
    unhealthy) features="unhealthy"; name="${example}-unhealthy-v${version}" ;;
    *) printf 'Error: DDI_VARIANT must be "lab" or "unhealthy", got "%s".\n' "$variant" >&2; exit 1 ;;
esac

# The bundle name is the file name the fleet receives, so it follows the fleet's file-name rule.
name_re='^[A-Za-z0-9_][A-Za-z0-9._-]*$'
if [[ ! "$name" =~ $name_re ]] || [ "${#name}" -gt 64 ]; then
    printf 'Error: bundle name "%s" must match %s and be at most 64 characters.\n' "$name" "$name_re" >&2
    exit 1
fi

out_dir="${DDI_BUNDLE_OUT:-$ROOT_DIR/tmp/ddi-bundle}"
mkdir -p "$out_dir"
bin="$out_dir/$name.bin"

reject() {
    rm -f "$bin" "$bin.sha256" "$bin.version" "$bin.chip"
    printf 'Error: %s\n' "$1" >&2
    exit 1
}

lock_dir="${DDI_BUNDLE_LOCK:-$ROOT_DIR/tmp/ddi-bundle.lock}"
lock_timeout="${DDI_BUNDLE_LOCK_TIMEOUT:-1800}"
if [[ ! "$lock_timeout" =~ ^[0-9]+$ ]]; then
    printf 'Error: DDI_BUNDLE_LOCK_TIMEOUT must be whole seconds, got "%s".\n' "$lock_timeout" >&2
    exit 1
fi
mkdir -p "$(dirname "$lock_dir")"
waited=0
until mkdir "$lock_dir" 2>/dev/null; do
    # An empty pid file is a holder between its mkdir and its pid write: wait for it.
    holder=$(cat "$lock_dir/pid" 2>/dev/null || true)
    if [ -n "$holder" ] && ! kill -0 "$holder" 2>/dev/null; then
        printf 'ddi-bundle: taking over the lock left by process %s, which has exited\n' "$holder" >&2
        rm -rf "$lock_dir"
        continue
    fi
    if [ "$waited" -ge "$lock_timeout" ]; then
        printf 'Error: another ddi-bundle run (process %s) holds %s; gave up after %s s. Remove the directory if no run is active.\n' \
            "${holder:-unknown}" "$lock_dir" "$waited" >&2
        exit 1
    fi
    if [ "$waited" -eq 0 ]; then
        printf 'ddi-bundle: waiting for the run in process %s to finish (up to %s s)\n' "${holder:-unknown}" "$lock_timeout" >&2
    fi
    sleep 1
    waited=$((waited + 1))
done
trap 'rm -rf "$lock_dir"' EXIT
printf '%s\n' "$$" > "$lock_dir/pid"

# The previous bundle goes first, before the build, so a failed build or a run that
# fails part-way never leaves a bundle or sidecars that describe another image. It
# goes inside the lock, so a waiting run never deletes the bundle being written.
rm -f "$bin" "$bin.sha256" "$bin.version" "$bin.chip"

elf="${DDI_BUNDLE_ELF:-}"
if [ -z "$elf" ]; then
    # The version override is baked in by build.rs, so the image reports exactly the
    # version its sidecar names. The next plain build reports the package version again.
    FIRMWARE_VERSION_OVERRIDE="$version" "$SCRIPT_DIR/build.sh" "$example" --features "$features"
    elf="${CARGO_TARGET_DIR:-target}/$TARGET/release/rustyfarian-rgb-clock"
fi
if [ ! -f "$elf" ]; then
    reject "no release ELF at $elf."
fi

# Without --merge, espflash writes only the application image (see check-image-size.sh).
# `|| reject`: a failing espflash may already have written part of the image.
espflash save-image --chip "$MCU" "$elf" "$bin" >&2 || reject "espflash save-image failed"
[ -f "$bin" ] || reject "espflash save-image produced no file at $bin"

size=$(wc -c < "$bin" | tr -d ' ')
if [ "$size" -lt 16 ]; then
    reject "image is $size bytes, too short to hold an app header"
fi
CHECK_IMAGE_BIN="$bin" "$SCRIPT_DIR/check-image-size.sh" "$example" || reject "image does not fit the OTA slots"

# Read the header byte by byte: od -tu2 would follow the host byte order, and the
# header is little-endian.
magic=$(od -An -tu1 -N1 "$bin" | tr -d ' ')
lo=$(od -An -tu1 -j12 -N1 "$bin" | tr -d ' ')
hi=$(od -An -tu1 -j13 -N1 "$bin" | tr -d ' ')
image_chip_id=$((hi * 256 + lo))
if [ "$magic" -ne 233 ]; then
    reject "image byte 0 is $magic, expected 233 (0xE9, the ESP app-image magic)"
fi
if [ "$image_chip_id" -ne "$chip_id" ]; then
    reject "image chip id at offset 12 is $image_chip_id, expected $chip_id for $MCU"
fi

if command -v shasum >/dev/null 2>&1; then
    sha256=$(shasum -a 256 "$bin" | awk '{print $1}')
else
    sha256=$(sha256sum "$bin" | awk '{print $1}')
fi
printf '%s\n' "$sha256" > "$bin.sha256"
printf '%s\n' "$version" > "$bin.version"
printf '%s\n' "$MCU" > "$bin.chip"

printf 'Image:    %s (%s bytes, sha256 %s)\n' "$bin" "$size" "$sha256"
printf 'Sha256:   %s\n' "$bin.sha256"
printf 'Version:  %s\n' "$bin.version"
printf 'Chip:     %s\n' "$bin.chip"
