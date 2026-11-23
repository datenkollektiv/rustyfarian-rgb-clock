#!/usr/bin/env bash
set -euo pipefail
# device-id.sh — print a board's device id, which is also its DDI controller id.
#
# Usage:
#   scripts/device-id.sh                     # reads the base MAC from the attached board (resets it)
#   scripts/device-id.sh 40:4c:ca:47:97:28   # computes the id from a given base MAC
#
# The id is `rgb-clock-` plus the SoftAP MAC's last two bytes, and the SoftAP MAC
# is the base MAC (as `espflash board-info` prints it) + 1 in the last byte.
# The portal SSID `Rustyfarian-XXXX` carries the same suffix.

if [ $# -gt 0 ]; then
    mac="$1"
else
    script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    info="$("$script_dir/espflash.sh" board-info)"
    mac="$(sed -n 's/^MAC address:[[:space:]]*//p' <<< "$info")"
fi
mac="$(printf '%s' "$mac" | tr 'A-F' 'a-f')"
if ! [[ "$mac" =~ ^([0-9a-f]{2}:){5}[0-9a-f]{2}$ ]]; then
    printf 'device-id: not a MAC address: %s\n' "${mac:-<none in the espflash board-info output>}" >&2
    exit 1
fi
IFS=: read -r _ _ _ _ b4 b5 <<< "$mac"
printf 'rgb-clock-%s%02x\n' "$b4" $(((16#$b5 + 1) % 256))
