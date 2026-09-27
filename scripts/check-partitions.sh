#!/usr/bin/env bash
set -euo pipefail
# check-partitions.sh — validate partitions.csv against the ESP-IDF rules this
# project depends on, so a misaligned or undersized table fails on the host
# instead of after a full erase and re-provisioning cycle.
#
# Usage: scripts/check-partitions.sh [--flash-size <bytes|0xHEX|4M>] [path-to-csv]
#   default csv: partitions.csv; default flash size: 4 MiB, which both supported
#   boards ship (ESP32-C6-DevKitC-1 N4 and ESP32-C3-DevKitM-1 N4). Pass the flag
#   for any other module so the past-flash-end check matches the real chip.
#
# This validates the table's geometry only. Whether the CURRENT firmware fits a
# slot is checked against the real app image by scripts/check-image-size.sh
# (`just image-check`), which preflight.sh runs before every flash.

if [ "${BASH_VERSINFO[0]:-0}" -lt 4 ]; then
    printf 'Error: this script needs bash 4+ for associative arrays; running under %s.\n' \
        "${BASH_VERSION:-unknown}" >&2
    printf 'macOS ships bash 3.2 as /bin/bash — install a newer one (brew install bash)\n' >&2
    printf 'so that `env bash` resolves to it, or invoke this script with that bash.\n' >&2
    exit 2
fi

flash_size_arg=""
if [ "${1:-}" = "--flash-size" ]; then
    flash_size_arg="${2:-}"
    [ -n "$flash_size_arg" ] || { printf 'Error: --flash-size needs a value (e.g. 0x400000 or 4M)\n' >&2; exit 2; }
    shift 2
fi
CSV="${1:-partitions.csv}"

PART_ALIGN=$((0x1000))     # every partition must sit on a 4 KiB flash-sector boundary
APP_ALIGN=$((0x10000))     # app partitions additionally need 64 KiB (MMU page) boundaries
OTADATA_SIZE=$((0x2000))   # exactly two 4 KiB sectors, one per OTA slot
NVS_OFFSET=$((0x9000))
NVS_SIZE=$((0x6000))
# Historical floor only: the release image measured on 2026-09-26. It catches a slot
# that was obviously never going to fit; it does NOT prove the current build fits.
# That proof comes from scripts/check-image-size.sh against the real .bin.
MIN_SLOT=1452669

fail=0
err() { printf 'FAIL: %s\n' "$1" >&2; fail=1; }
trim() { local s="$1"; s="${s#"${s%%[![:space:]]*}"}"; printf '%s' "${s%"${s##*[![:space:]]}"}"; }

# to_bytes <value> — echo a partition-table size/offset as a plain byte count.
# Accepts decimal, 0x-hex, and the K/M suffixes ESP-IDF's own CSV format allows
# (bash arithmetic cannot evaluate "1M" on its own). Returns 1 on anything else,
# so the caller reports a readable FAIL instead of aborting on a syntax error.
to_bytes() {
    local v="$1"
    case "$v" in
        *[kK]) [[ "$v" =~ ^[0-9]+[kK]$ ]] || return 1; printf '%s' $(( ${v%[kK]} * 1024 ))        ;;
        *[mM]) [[ "$v" =~ ^[0-9]+[mM]$ ]] || return 1; printf '%s' $(( ${v%[mM]} * 1024 * 1024 )) ;;
        0[xX]*) [[ "$v" =~ ^0[xX][0-9a-fA-F]+$ ]] || return 1; printf '%s' $((v)) ;;
        *)      [[ "$v" =~ ^[0-9]+$ ]]            || return 1; printf '%s' $((v)) ;;
    esac
}

if [ -n "$flash_size_arg" ]; then
    if ! FLASH_SIZE=$(to_bytes "$flash_size_arg"); then
        printf 'Error: --flash-size "%s" is not a decimal, 0x-hex, or K/M-suffixed value\n' "$flash_size_arg" >&2
        exit 2
    fi
else
    FLASH_SIZE=$((0x400000))
fi

[ -f "$CSV" ] || { printf 'FAIL: %s not found\n' "$CSV" >&2; exit 1; }

names=""
prev_end=0
prev_off=-1
prev_name=""
declare -A size_of
declare -A type_of
declare -A off_of
declare -A sub_of

while IFS= read -r line || [ -n "$line" ]; do
    line="${line%%#*}"
    [ -n "${line//[[:space:],]/}" ] || continue

    IFS=',' read -r c_name c_type c_sub c_off c_size _ <<< "$line"
    name=$(trim "${c_name:-}"); ptype=$(trim "${c_type:-}")
    psub=$(trim "${c_sub:-}")
    off_s=$(trim "${c_off:-}");  size_s=$(trim "${c_size:-}")

    [ -n "$off_s" ]  || { err "$name: empty offset (this checker requires explicit offsets)"; continue; }
    [ -n "$size_s" ] || { err "$name: empty size"; continue; }

    if ! off=$(to_bytes "$off_s"); then
        err "$name: offset \"$off_s\" is not a decimal, 0x-hex, or K/M-suffixed value"
        continue
    fi
    if ! size=$(to_bytes "$size_s"); then
        err "$name: size \"$size_s\" is not a decimal, 0x-hex, or K/M-suffixed value"
        continue
    fi

    if [ -n "${size_of[$name]+set}" ]; then
        err "duplicate partition name: $name (ESP-IDF requires unique names)"
    fi

    names="$names $name"
    size_of["$name"]=$size
    type_of["$name"]=$ptype
    sub_of["$name"]=$psub
    off_of["$name"]=$off

    # The overlap check below compares only against the previous row, so it is
    # sound only for a table sorted by offset. Reject anything else outright rather
    # than let prev_end walk backwards and hide a real overlap further down.
    if [ "$off" -le "$prev_off" ]; then
        err "$name at $off_s is out of order — rows must have strictly increasing offsets (previous: $prev_name at $(printf '0x%x' "$prev_off"))"
    elif [ "$off" -lt "$prev_end" ]; then
        err "$name at $off_s overlaps $prev_name (which ends at $(printf '0x%x' "$prev_end"))"
    fi
    if [ $((off % PART_ALIGN)) -ne 0 ]; then
        err "$name at $off_s is not 4 KiB aligned — flash erases in 4 KiB sectors"
    fi
    if [ $((off + size)) -gt "$FLASH_SIZE" ]; then
        err "$name ends at $(printf '0x%x' $((off + size))) — past the $(printf '0x%x' "$FLASH_SIZE") flash end"
    fi
    if [ "$ptype" = "app" ]; then
        if [ $((off % APP_ALIGN)) -ne 0 ]; then
            err "$name is an app partition at $off_s — not 64 KiB aligned (ESP-IDF rejects this)"
        fi
        if [ "$size" -lt "$MIN_SLOT" ]; then
            err "$name is $size bytes — smaller than the measured $MIN_SLOT byte image (historical floor; run \`just image-check\` for the current build)"
        fi
    fi

    prev_end=$((off + size))
    prev_off=$off
    prev_name="$name"
done < "$CSV"

# Required partitions, each with the exact type/subtype pair ESP-IDF needs. Checking
# the name alone is not enough: the bootloader selects slots by type/subtype, so a
# row named ota_1 whose subtype says factory is not an OTA slot at all — the name is
# only a label. phy_init is validated when present but is not required here.
declare -A required_pair=(
    [nvs]="data/nvs"
    [otadata]="data/ota"
    [ota_0]="app/ota_0"
    [ota_1]="app/ota_1"
)
declare -A optional_pair=(
    [phy_init]="data/phy"
)

for part in "${!required_pair[@]}"; do
    if [ -z "${type_of[$part]+set}" ]; then
        err "missing required partition: $part"
        continue
    fi
    actual="${type_of[$part]}/${sub_of[$part]}"
    if [ "$actual" != "${required_pair[$part]}" ]; then
        err "$part is declared $actual — must be ${required_pair[$part]}"
    fi
done

for part in "${!optional_pair[@]}"; do
    if [ -n "${type_of[$part]+set}" ]; then
        actual="${type_of[$part]}/${sub_of[$part]}"
        if [ "$actual" != "${optional_pair[$part]}" ]; then
            err "$part is declared $actual — must be ${optional_pair[$part]}"
        fi
    fi
done

if [ -n "${size_of[otadata]:-}" ] && [ "${size_of[otadata]}" -ne "$OTADATA_SIZE" ]; then
    err "otadata is ${size_of[otadata]} bytes — must be exactly $OTADATA_SIZE"
fi
if [ -n "${size_of[ota_0]:-}" ] && [ -n "${size_of[ota_1]:-}" ] \
   && [ "${size_of[ota_0]}" -ne "${size_of[ota_1]}" ]; then
    err "ota_0 (${size_of[ota_0]}) and ota_1 (${size_of[ota_1]}) must be the same size"
fi

# Reuse the values the loop already parsed rather than re-reading the CSV: a second
# parser here silently failed to fire on K/M-suffixed sizes.
if [ -n "${off_of[nvs]:-}" ] && [ "${off_of[nvs]}" -ne "$NVS_OFFSET" ]; then
    err "nvs moved to $(printf '0x%x' "${off_of[nvs]}") from $(printf '0x%x' "$NVS_OFFSET") — provisioned credentials would be lost"
fi
if [ -n "${size_of[nvs]:-}" ] && [ "${size_of[nvs]}" -ne "$NVS_SIZE" ]; then
    err "nvs resized to $(printf '0x%x' "${size_of[nvs]}") from $(printf '0x%x' "$NVS_SIZE") — provisioned credentials would be lost"
fi

if [ "$fail" -ne 0 ]; then
    printf '\n%s is not a valid OTA partition table for this board.\n' "$CSV" >&2
    exit 1
fi

printf 'OK: %s is a valid A/B OTA table (%d KiB free at the tail).\n' \
    "$CSV" $(( (FLASH_SIZE - prev_end) / 1024 ))
