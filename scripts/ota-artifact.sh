#!/usr/bin/env bash
set -euo pipefail
# ota-artifact.sh — stage the OTA firmware image, compute its SHA-256, and write the manifest
#
# Usage: scripts/ota-artifact.sh <example> <variant> [version] [truncate] [tick_topic]
#   example:    idf_{chip}_{name}  e.g. idf_c6_rgb_clock
#   variant:    release or unhealthy; a truncated artefact is staged under <variant>-truncated
#   version:    override manifest version (e.g. "1.0.0"); default: parse from Cargo.toml
#   truncate:   serve only the first N bytes of the image (for checksum-mismatch demo);
#               0 (default) means serve the full image
#   tick_topic: the demo tick-topic override the image was built with (already trimmed);
#               stages it under <variant>[-truncated]-topic-<name> so it never replaces
#               the normal image

example="${1:-}"
variant="${2:-}"
version="${3:-}"
truncate="${4:-0}"
tick_topic="${5:-}"

if [ -z "$example" ] || [ -z "$variant" ]; then
    printf 'Usage: %s <idf_{chip}_{name}> <variant> [version] [truncate] [tick_topic]\n' "$0" >&2
    printf '  variant: release or unhealthy (demo-only)\n' >&2
    printf '  version: override manifest version; default: parse from Cargo.toml\n' >&2
    printf '  truncate: serve only first N bytes (0 = full image, default)\n' >&2
    exit 2
fi

# The variant names a directory under tmp/ota/ that is removed and recreated
# below, so it must be one of the known names — never a path fragment.
case "$variant" in
    release|unhealthy) ;;
    *)
        printf 'Error: variant must be "release" or "unhealthy", got "%s"\n' "$variant" >&2
        exit 1
        ;;
esac

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
eval "$("$SCRIPT_DIR/chip-env.sh" "$example")"

# Resolve the source image: env override for testing, else standard location
image="${OTA_IMAGE:-$ROOT_DIR/tmp/${example}-app.bin}"
if [ ! -f "$image" ]; then
    printf 'Error: image not found at %s\n' "$image" >&2
    printf 'Run `just build %s` first (or set OTA_IMAGE to override).\n' "$example" >&2
    exit 1
fi

image_size=$(wc -c < "$image" | tr -d ' ')

# Validate truncate is a non-negative integer and does not exceed image size
if ! [[ "$truncate" =~ ^[0-9]+$ ]]; then
    printf 'Error: truncate must be a non-negative integer, got "%s"\n' "$truncate" >&2
    exit 1
fi
if [ "$truncate" -gt 0 ] && [ "$truncate" -ge "$image_size" ]; then
    printf 'Error: truncate (%d) must be smaller than the image (%d bytes), or nothing is cut\n' "$truncate" "$image_size" >&2
    exit 1
fi

# Compute SHA-256 over the FULL source image
if command -v shasum >/dev/null 2>&1; then
    sha256=$(shasum -a 256 "$image" | awk '{print $1}')
else
    sha256=$(sha256sum "$image" | awk '{print $1}')
fi

# Parse version from Cargo.toml if not provided
if [ -z "$version" ]; then
    version=$(grep -m1 '^version = "' "$ROOT_DIR/Cargo.toml" | sed 's/^version = "\([^"]*\)".*/\1/')
    if [ -z "$version" ]; then
        printf 'Error: could not parse version from %s\n' "$ROOT_DIR/Cargo.toml" >&2
        exit 1
    fi
fi

# Validate version looks like MAJOR.MINOR.PATCH
if ! [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    printf 'Error: version "%s" does not match MAJOR.MINOR.PATCH format\n' "$version" >&2
    exit 1
fi

# Resolve OTA_HOST: from env (dotenv-loaded), else auto-detect
if [ -z "${OTA_HOST:-}" ]; then
    if [ "$(uname)" = "Darwin" ]; then
        OTA_HOST=$(ipconfig getifaddr en0 2>/dev/null || true)
    else
        OTA_HOST=$(hostname -I 2>/dev/null | awk '{print $1}' || true)
    fi
    if [ -z "$OTA_HOST" ]; then
        printf 'Error: could not auto-detect OTA_HOST\n' >&2
        printf 'Set OTA_HOST in .env or as an environment variable (LAN IP of this machine).\n' >&2
        exit 1
    fi
fi

OTA_PORT="${OTA_PORT:-8000}"

# Create output directory (recreate it per run); a truncated artefact gets its
# own directory so it never overwrites the good image of the same variant, and so
# does a tick-topic override build. The topic is reduced to [A-Za-z0-9._-] so it
# stays a single directory name and never becomes a path fragment.
# The topic suffix always comes last, so a topic ending in `-truncated` cannot
# collide with a truncated build; distinct topics that sanitise alike (`a/b`,
# `a_b`) still share a directory, so pick names that differ in [A-Za-z0-9._-].
staged="$variant"
if [ "$truncate" -gt 0 ]; then
    staged="${staged}-truncated"
fi
if [ -n "$tick_topic" ]; then
    staged="${staged}-topic-$(printf '%s' "$tick_topic" | tr -c 'A-Za-z0-9._-' '_')"
fi
output_dir="$ROOT_DIR/tmp/ota/${example}/${staged}"
rm -rf "$output_dir"
mkdir -p "$output_dir"

# Copy the image to firmware.bin; if truncate is set, truncate it on write
if [ "$truncate" -gt 0 ]; then
    head -c "$truncate" "$image" > "$output_dir/firmware.bin"
else
    cp "$image" "$output_dir/firmware.bin"
fi

# Generate the manifest.json with exactly the four required keys
manifest="$output_dir/manifest.json"
printf '{\n' > "$manifest"
printf '  "version": "%s",\n' "$version" >> "$manifest"
printf '  "sha256": "%s",\n' "$sha256" >> "$manifest"
printf '  "url": "http://%s:%s/%s/%s/firmware.bin",\n' "$OTA_HOST" "$OTA_PORT" "$example" "$staged" >> "$manifest"
printf '  "target": "%s"\n' "$MCU" >> "$manifest"
printf '}\n' >> "$manifest"

# The ready-to-publish command for this artefact, per target, so `just ota-push`
# sends exactly the URL resolved here; the last staged artefact wins.
command_file="$ROOT_DIR/tmp/ota/${example}/command.json"
printf '{"manifest_url":"http://%s:%s/%s/%s/manifest.json"}\n' "$OTA_HOST" "$OTA_PORT" "$example" "$staged" > "$command_file"

# Print diagnostics to stderr
{
    printf '\n'
    printf 'OTA artefact staged:\n'
    printf '  image:       %s\n' "$image"
    printf '  size:        %d bytes\n' "$image_size"
    printf '  sha256:      %s\n' "$sha256"
    printf '  output dir:  %s\n' "$output_dir"
    if [ "$truncate" -gt 0 ]; then
        printf '\n'
        printf 'WARNING: firmware.bin truncated to %d bytes (serves checksum-mismatch scenario).\n' "$truncate"
        printf 'The manifest hash (%s) is from the FULL image and will NOT match.\n' "$sha256"
    fi
    printf '\n'
    printf 'To push this update over MQTT (requires broker connectivity and `just ota-serve` running):\n'
    printf '\n'
    printf '  just ota-push %s\n' "$example"
    printf '\n'
    printf 'It publishes %s:\n' "$command_file"
    printf '  %s\n' "$(cat "$command_file")"
    printf '\n'
    printf 'Ensure `just ota-serve` is running on this machine.\n'
    printf '\n'
} >&2
