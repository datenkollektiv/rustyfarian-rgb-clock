#!/usr/bin/env bash
set -euo pipefail
# chip-env.sh — resolve MCU, TARGET and FLASH_SIZE from an idf_{chip}_{name} example name.
#
# Usage:
#   eval "$(scripts/chip-env.sh idf_c6_rgb_clock)"
#   # sets MCU=esp32c6, TARGET=riscv32imac-esp-espidf and FLASH_SIZE=4mb in the calling shell
#
# FLASH_SIZE is the supported module's flash (both are N4, as scripts/check-partitions.sh
# assumes), in espflash's spelling. `espflash flash` reads it from the chip, but
# `espflash save-image` cannot, so a saved image takes it from here. It is not
# sdkconfig.defaults' CONFIG_ESPTOOLPY_FLASHSIZE_2MB: that one stays, because on the C6
# it fixes the MMU page size of bootloaders already in the field, and espflash
# rewrites the header's flash-size byte in both paths anyway.
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
    c3) printf 'MCU=esp32c3\nTARGET=riscv32imc-esp-espidf\nFLASH_SIZE=4mb\n'  ;;
    c6) printf 'MCU=esp32c6\nTARGET=riscv32imac-esp-espidf\nFLASH_SIZE=4mb\n' ;;
    *)  printf 'Unknown chip "%s" in "%s". Supported: c3, c6\n' "$chip" "$example" >&2; exit 1 ;;
esac
