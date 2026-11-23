#!/usr/bin/env bash
set -euo pipefail
# espflash.sh — run an espflash subcommand on the board scripts/detect-port.sh picks.
#
# Usage:
#   scripts/espflash.sh [--require-port] <subcommand> [args...]
#
# Adds `--port <port>` when detect-port.sh names one port (ESPFLASH_PORT, or the
# only attached USB serial board); otherwise espflash chooses and reports itself.
# --require-port refuses instead of letting espflash choose: a destructive
# subcommand (erase-flash) must not reach an ambiguous port, where espflash could
# pick a different board than the one meant.

require_port=false
if [ "${1:-}" = "--require-port" ]; then
    require_port=true
    shift
fi
if [ $# -eq 0 ]; then
    printf 'usage: scripts/espflash.sh [--require-port] <subcommand> [args...]\n' >&2
    exit 2
fi
sub="$1"
shift

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
port="$("$script_dir/detect-port.sh")"
if [ -n "$port" ]; then
    exec espflash "$sub" --port "$port" "$@"
fi
if [ "$require_port" = true ]; then
    printf 'Error: no unique serial port detected; refusing to run espflash %s.\n' "$sub" >&2
    printf 'Set ESPFLASH_PORT (e.g. ESPFLASH_PORT=/dev/cu.usbmodem1101 just %s), or detach the other boards.\n' "$sub" >&2
    exit 1
fi
exec espflash "$sub" "$@"
