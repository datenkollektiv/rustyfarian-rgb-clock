#!/usr/bin/env bash
set -euo pipefail
# flash-image-tests.sh — hardware-free regression tests for scripts/flash-image.sh.
#
# flash-image.sh is the last step of the destructive `just flash-baseline` path and
# the only place that guards against writing one chip's image onto another board.
# These cases pin that guard down: a fake `espflash` on PATH answers `board-info`
# with whatever chip $FAKE_CHIP names and records any `flash` invocation instead of
# running it, so the script's decisions are observable without a device.
#
# Quiet on success — one summary line, since this runs on every `just verify`.
# Pass -v (or set VERBOSE=1) to list each case.
#
# Usage: scripts/flash-image-tests.sh [-v]

verbose="${VERBOSE:-0}"
if [ "${1:-}" = "-v" ]; then
    verbose=1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
SUT="$SCRIPT_DIR/flash-image.sh"

work="$ROOT_DIR/tmp/flash-image-tests"
rm -rf "$work"
mkdir -p "$work/bin" "$work/target/riscv32imac-esp-espidf/release"

cat > "$work/bin/espflash" <<'EOF'
#!/usr/bin/env bash
case "${1:-}" in
    board-info)
        printf 'FAKE-BOARD-INFO %s\n' "$*" >> "$FAKE_LOG"
        if [ "${FAKE_CHIP:-esp32c6}" = "none" ]; then
            printf 'Flash size:        4MB\n'
        else
            printf 'Chip type:         %s (revision v0.0)\nFlash size:        4MB\n' "${FAKE_CHIP:-esp32c6}"
        fi
        ;;
    flash)
        printf 'FAKE-FLASH %s\n' "$*" >> "$FAKE_LOG"
        ;;
    *)
        printf 'fake espflash: unexpected subcommand %s\n' "$*" >&2
        exit 99
        ;;
esac
EOF
chmod +x "$work/bin/espflash"

app="$work/target/riscv32imac-esp-espidf/release/rustyfarian-rgb-clock"
bl="$work/bootloader.bin"
: > "$app"
: > "$bl"
log="$work/flash.log"

failures=0
total=0

# Fake device directory scanned by scripts/detect-port.sh (DETECT_PORT_DEV_DIR)
# when no port is given. stage_ports <n> populates it with n fake nodes under
# both the macOS and Linux names, so the scan sees n candidates on either host.
devdir="$work/dev"
stage_ports() {
    rm -rf "$devdir"
    mkdir -p "$devdir"
    local i
    for i in $(seq 1 "$1"); do
        : > "$devdir/cu.usbmodemFAKE$i"
        : > "$devdir/ttyUSB$i"
    done
}
stage_ports 0

# Positional port passed to the script; empty means "let it resolve one".
case_port="/dev/fake-port"

# run_case <name> <expected status> <stdout+stderr must contain> <espflash log must contain|-> <env assignments...>
# The log needle is "-" when NO espflash call (board-info or flash) may happen.
# ESPFLASH_PORT is cleared unless a case sets it, so the developer's shell cannot
# leak into the resolution under test.
run_case() {
    local name="$1" want_status="$2" needle="$3" log_needle="$4"
    shift 4
    total=$((total + 1))
    : > "$log"
    set +e
    output="$(env -u ESPFLASH_PORT PATH="$work/bin:$PATH" CARGO_TARGET_DIR="$work/target" \
        FAKE_LOG="$log" DETECT_PORT_DEV_DIR="$devdir" "$@" \
        "$SUT" idf_c6_rgb_clock "$bl" "$case_port" 2>&1)"
    status=$?
    set -e
    local ok=yes
    if [ "$status" -ne "$want_status" ]; then ok=no; fi
    case "$output" in *"$needle"*) ;; *) ok=no ;; esac
    if [ "$log_needle" = "-" ]; then
        if [ -s "$log" ]; then ok=no; fi
    else
        case "$(cat "$log")" in *"$log_needle"*) ;; *) ok=no ;; esac
    fi
    # Invariant for every refused run: whatever else happened, nothing was flashed.
    if [ "$want_status" -ne 0 ]; then
        case "$(cat "$log")" in *FAKE-FLASH*) ok=no ;; esac
    fi
    if [ "$ok" = "yes" ]; then
        if [ "$verbose" = "1" ]; then printf 'ok       %s\n' "$name"; fi
    else
        failures=$((failures + 1))
        printf 'FAIL     %s — exit %d (wanted %d)\n' "$name" "$status" "$want_status" >&2
        printf '           wanted output substring: %s\n' "$needle" >&2
        printf '           wanted flash-log substring: %s\n' "$log_needle" >&2
        printf '%s\n' "$output" | sed 's/^/           /' >&2
        printf '           flash log: %s\n' "$(cat "$log")" >&2
    fi
}

run_case "matching chip flashes with pinned port, IDF bootloader and app" \
    0 "Chip check OK: esp32c6" "--port /dev/fake-port --partition-table partitions.csv --bootloader $bl --ignore-app-descriptor $app" \
    FAKE_CHIP=esp32c6

run_case "mismatched chip is refused before any flash" \
    1 "is an esp32c3, but idf_c6_rgb_clock targets esp32c6" "FAKE-BOARD-INFO board-info --port /dev/fake-port" \
    FAKE_CHIP=esp32c3

run_case "dry run prints the flash command and never calls espflash" \
    0 "[dry-run] would run: espflash flash --port /dev/fake-port" "-" \
    DRY_RUN=1 FAKE_CHIP=esp32c3

run_case "board-info without a chip type line is refused" \
    1 'could not read "Chip type:"' "FAKE-BOARD-INFO" \
    FAKE_CHIP=none

rm -f "$bl"
run_case "missing bootloader artifact is refused" \
    1 "expected artifact is missing" "-" \
    FAKE_CHIP=esp32c6
: > "$bl"

# --- port resolution when no positional port is given ---------------------
case_port=""

run_case "ESPFLASH_PORT is used when no positional port is given" \
    0 "Chip check OK: esp32c6" "FAKE-BOARD-INFO board-info --port /dev/from-env" \
    FAKE_CHIP=esp32c6 ESPFLASH_PORT=/dev/from-env

run_case "no port available is refused before board-info" \
    1 "no unique serial port detected" "-" \
    FAKE_CHIP=esp32c6

stage_ports 2
run_case "several detected ports are refused rather than one chosen" \
    1 "no unique serial port detected" "-" \
    FAKE_CHIP=esp32c6

run_case "ESPFLASH_PORT wins over several detected ports" \
    0 "Chip check OK: esp32c6" "FAKE-FLASH flash --port /dev/from-env --partition-table" \
    FAKE_CHIP=esp32c6 ESPFLASH_PORT=/dev/from-env

stage_ports 1
run_case "a single detected port is used" \
    0 "Chip check OK: esp32c6" "--port $devdir/" \
    FAKE_CHIP=esp32c6

case_port="/dev/fake-port"

if [ "$failures" -ne 0 ]; then
    printf '\n%d of %d flash-image cases failed.\n' "$failures" "$total" >&2
    exit 1
fi
printf '\nAll %d flash-image cases passed.\n' "$total"
