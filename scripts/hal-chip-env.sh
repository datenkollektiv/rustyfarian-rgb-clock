#!/usr/bin/env bash
set -euo pipefail
# hal-chip-env.sh — resolve the bare-metal (esp-hal) build settings for a chip.
#
# Usage:
#   eval "$(scripts/hal-chip-env.sh c3)"
#   # sets HAL_TARGET, HAL_FEATURE, ESPFLASH_CHIP and IDF_TARGET in the calling shell
#
# IDF_TARGET is the ESP-IDF triple whose built bootloader `flash-hal` reuses.
# To add a new chip, extend the case statement below and the crate's chip features.

chip="${1:-}"
if [ -z "$chip" ]; then
    printf 'Usage: %s <chip>  e.g. c3\n' "$0" >&2
    exit 2
fi

case "$chip" in
    c3) printf 'HAL_TARGET=riscv32imc-unknown-none-elf\nHAL_FEATURE=esp32c3\nESPFLASH_CHIP=esp32c3\nIDF_TARGET=riscv32imc-esp-espidf\n' ;;
    c6) printf 'The esp-hal clock has no C6 arm yet (milestone M4, docs/features/esp-hal-clock-v1.md). Supported: c3\n' >&2; exit 1 ;;
    *)  printf 'Unknown chip "%s". Supported: c3\n' "$chip" >&2; exit 1 ;;
esac
