#!/usr/bin/env bash
set -euo pipefail
# check-image-size.sh — generate the exact flashable app image for a chip target
# and check that it fits both OTA slots in partitions.csv.
#
# Usage: scripts/check-image-size.sh <idf_{chip}_{name}> [elf] [partitions.csv]
#   elf defaults to the release binary under $CARGO_TARGET_DIR (or ./target).
#
# scripts/check-partitions.sh only validates the table's geometry against a
# historical size constant. This script measures what an OTA update would really
# download — the .bin espflash assembles from the ELF, headers and padding
# included — so firmware growth fails here, on the host, before a flash or an
# OTA push. preflight.sh runs it before every flash; `just image-check` runs it
# alone. The generated image is left in tmp/ for inspection.

example="${1:-}"
if [ -z "$example" ]; then
    printf 'Usage: %s <idf_{chip}_{name}> [elf] [partitions.csv]\n' "$0" >&2
    exit 2
fi
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
eval "$("$SCRIPT_DIR/chip-env.sh" "$example")"

target_dir="${CARGO_TARGET_DIR:-target}"
elf="${2:-$target_dir/$TARGET/release/rustyfarian-rgb-clock}"
csv="${3:-$ROOT_DIR/partitions.csv}"

if [ ! -f "$elf" ]; then
    printf 'Error: no release ELF at %s — run `just build %s` first.\n' "$elf" "$example" >&2
    exit 1
fi
[ -f "$csv" ] || { printf 'Error: %s not found\n' "$csv" >&2; exit 1; }

trim() { local s="$1"; s="${s#"${s%%[![:space:]]*}"}"; printf '%s' "${s%"${s##*[![:space:]]}"}"; }
to_bytes() {
    local v="$1"
    case "$v" in
        *[kK]) [[ "$v" =~ ^[0-9]+[kK]$ ]] || return 1; printf '%s' $(( ${v%[kK]} * 1024 ))        ;;
        *[mM]) [[ "$v" =~ ^[0-9]+[mM]$ ]] || return 1; printf '%s' $(( ${v%[mM]} * 1024 * 1024 )) ;;
        0[xX]*) [[ "$v" =~ ^0[xX][0-9a-fA-F]+$ ]] || return 1; printf '%s' $((v)) ;;
        *)      [[ "$v" =~ ^[0-9]+$ ]]            || return 1; printf '%s' $((v)) ;;
    esac
}

mkdir -p "$ROOT_DIR/tmp"
bin="$ROOT_DIR/tmp/${example}-app.bin"
rm -f "$bin"

# Without --merge, espflash writes only the application image — the bytes an OTA
# client downloads and writes into a slot — not the bootloader or table.
printf 'Generating app image for %s (MCU=%s)...\n' "$example" "$MCU" >&2
espflash save-image --chip "$MCU" "$elf" "$bin" >&2

if [ ! -f "$bin" ]; then
    printf 'Error: espflash save-image produced no file at %s\n' "$bin" >&2
    exit 1
fi
image_size=$(wc -c < "$bin" | tr -d ' ')

fail=0
seen_ota_0=0
seen_ota_1=0
while IFS= read -r line || [ -n "$line" ]; do
    line="${line%%#*}"
    [ -n "${line//[[:space:],]/}" ] || continue
    IFS=',' read -r c_name c_type c_sub _ c_size _ <<< "$line"
    name=$(trim "${c_name:-}"); ptype=$(trim "${c_type:-}"); psub=$(trim "${c_sub:-}")
    [ "$ptype" = "app" ] || continue
    case "$psub" in
        ota_0) seen_ota_0=$((seen_ota_0 + 1)) ;;
        ota_1) seen_ota_1=$((seen_ota_1 + 1)) ;;
        *) continue ;;
    esac
    if ! slot=$(to_bytes "$(trim "${c_size:-}")"); then
        printf 'FAIL: %s has an unparseable size in %s\n' "$name" "$csv" >&2
        fail=1
        continue
    fi
    if [ "$image_size" -gt "$slot" ]; then
        printf 'FAIL: %s image is %s bytes but slot %s is only %s bytes (%s bytes too big)\n' \
            "$example" "$image_size" "$name" "$slot" $((image_size - slot)) >&2
        fail=1
    else
        printf 'OK: %s image is %s bytes; fits %s (%s bytes, %d%% used, %d KiB headroom)\n' \
            "$example" "$image_size" "$name" "$slot" \
            $(( image_size * 100 / slot )) $(( (slot - image_size) / 1024 )) >&2
    fi
done < "$csv"

# Exactly one of each: two ota_0 rows and no ota_1 would still be "two slots".
if [ "$seen_ota_0" -ne 1 ] || [ "$seen_ota_1" -ne 1 ]; then
    printf 'FAIL: expected exactly one app/ota_0 and one app/ota_1 row in %s, found %d and %d\n' \
        "$csv" "$seen_ota_0" "$seen_ota_1" >&2
    fail=1
fi

if [ "$fail" -ne 0 ]; then
    printf '\n%s does not fit the OTA layout — grow the slots in partitions.csv or shrink the image.\n' "$bin" >&2
    exit 1
fi
printf 'Image left at %s\n' "$bin" >&2
