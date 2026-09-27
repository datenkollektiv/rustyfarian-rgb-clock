#!/usr/bin/env bash
set -euo pipefail
# chip-env.sh — resolve MCU and TARGET from an idf_{chip}_{name} example name.
#
# Usage:
#   eval "$(scripts/chip-env.sh idf_c6_rgb_clock)"
#   # sets MCU=esp32c6 and TARGET=riscv32imac-esp-espidf in the calling shell
#
# To add a new chip, extend the case statement below.

# Not `${1:?...}`: a `}` inside the message (from `{chip}`) closes the parameter
# expansion early, so the usage text leaks into the variable's value.
example="${1:-}"
if [ -z "$example" ]; then
    printf 'Usage: %s <idf_{chip}_{name}>  e.g. idf_c6_rgb_clock\n' "$0" >&2
    exit 2
fi
chip=$(printf '%s' "$example" | cut -d_ -f2)

case "$chip" in
    c3) printf 'MCU=esp32c3\nTARGET=riscv32imc-esp-espidf\n'  ;;
    c6) printf 'MCU=esp32c6\nTARGET=riscv32imac-esp-espidf\n' ;;
    *)  printf 'Unknown chip "%s" in "%s". Supported: c3, c6\n' "$chip" "$example" >&2; exit 1 ;;
esac
