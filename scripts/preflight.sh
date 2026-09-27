#!/usr/bin/env bash
set -euo pipefail
# preflight.sh — build the firmware and validate everything a flash depends on,
# without touching the device. Prints the resolved bootloader path on stdout; all
# progress and diagnostics go to stderr, so callers can capture just the path.
#
# Usage: scripts/preflight.sh <idf_{chip}_{name}>   e.g. idf_c6_rgb_clock
#
# This exists so `just flash-baseline` can build and validate BEFORE erasing the
# chip. A compile error, a missing or ambiguous bootloader, or a bootloader built
# without rollback must never leave a working device wiped of its credentials.

# Not `${1:?...}`: a `}` inside the message (from `{chip}`) closes the parameter
# expansion early, so the usage text leaks into the variable's value.
example="${1:-}"
if [ -z "$example" ]; then
    printf 'Usage: %s <idf_{chip}_{name}>  e.g. idf_c6_rgb_clock\n' "$0" >&2
    exit 2
fi
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./lib.sh
. "$SCRIPT_DIR/lib.sh"
eval "$("$SCRIPT_DIR/chip-env.sh" "$example")"

target_dir="${CARGO_TARGET_DIR:-target}"

# Validate the table's geometry here, in the script path, so `just flash` cannot
# skip it: only `just flash-baseline` declares the recipe dependency, and espflash
# itself checks neither the type/subtype pairs nor NVS stability.
"$SCRIPT_DIR/check-partitions.sh" >&2

printf 'Building %s (MCU=%s, target=%s)...\n' "$example" "$MCU" "$TARGET" >&2
MCU="$MCU" cargo build --release --target "$TARGET" >&2

app="$target_dir/$TARGET/release/rustyfarian-rgb-clock"
if [ ! -f "$app" ]; then
    printf 'Error: app binary not found at %s\n' "$app" >&2
    exit 1
fi

# Measure the real flashable image against both OTA slots. partition-check's size
# constant is only a historical floor; this is the check that tracks growth.
"$SCRIPT_DIR/check-image-size.sh" "$example" "$app" >&2

# A multiple-candidates error exits inside the command substitution; the `if` keeps
# that from being swallowed by `set -e` without a second, redundant message.
if ! bl="$(find_idf_bootloader "$TARGET" "$target_dir")"; then
    exit 1
fi
if [ -z "$bl" ]; then
    printf 'Error: no esp-idf-sys-built bootloader under %s.\n' "$target_dir" >&2
    printf 'Run `just clean-idf && just build`; flashing espflash-bundled v5.5.1 would disable rollback.\n' >&2
    exit 1
fi

# The bootloader sits at <out>/build/bootloader/bootloader.bin and the sdkconfig it
# was compiled from at <out>/sdkconfig. Checking that generated file — not
# sdkconfig.defaults — is what proves rollback is compiled into THIS bootloader.
# A cached build predating the setting still lets mark_valid() report success while
# rollback is inert, which is the failure this guard exists to prevent.
sdkconfig="${bl%/build/bootloader/bootloader.bin}/sdkconfig"
if [ ! -f "$sdkconfig" ]; then
    printf 'Error: no generated sdkconfig beside the bootloader (looked for %s).\n' "$sdkconfig" >&2
    printf 'Run `just clean-idf && just build` to regenerate the ESP-IDF build tree.\n' >&2
    exit 1
fi
if ! grep -qx 'CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE=y' "$sdkconfig"; then
    printf 'Error: the selected bootloader was built WITHOUT rollback enabled.\n' >&2
    printf '  bootloader: %s\n' "$bl" >&2
    printf '  sdkconfig:  %s\n' "$sdkconfig" >&2
    printf 'OTA rollback would be silently inert — mark_valid() would still succeed.\n' >&2
    printf 'Run `just clean-idf && just build`, then retry.\n' >&2
    exit 1
fi

printf 'Preflight OK — app: %s\n' "$app" >&2
printf 'Preflight OK — bootloader (rollback enabled): %s\n' "$bl" >&2
printf '%s\n' "$bl"
