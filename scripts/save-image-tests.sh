#!/usr/bin/env bash
set -euo pipefail
# save-image-tests.sh — hardware-free regression tests for the saved-image path:
# scripts/flash-image.sh's --save-image mode, scripts/save-image.sh's guard and the
# `just bench-flash` refusals that run before any build.
#
# The saved image is what `just bench-flash` uploads to the remote bench, so these
# cases pin down what goes into it (chip, flash size, IDF bootloader, partition
# table, app) and that saving never probes a board. A fake `espflash` on PATH records
# every call and writes a stub image for `save-image`; a fake `cargo` records the
# build and fails, so the guard cases can tell whether a build was reached.
#
# Quiet on success — one summary line, since this runs on every `just verify`.
# Pass -v (or set VERBOSE=1) to list each case.
#
# Usage: scripts/save-image-tests.sh [-v]

verbose="${VERBOSE:-0}"
if [ "${1:-}" = "-v" ]; then
    verbose=1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

work="$ROOT_DIR/tmp/save-image-tests"
rm -rf "$work"
mkdir -p "$work/bin" "$work/target/riscv32imac-esp-espidf/release" "$work/target/riscv32imc-esp-espidf/release"

cat > "$work/bin/espflash" <<'EOF'
#!/usr/bin/env bash
case "${1:-}" in
    board-info|flash)
        printf 'FAKE-%s %s\n' "$1" "$*" >> "$FAKE_LOG"
        ;;
    save-image)
        printf 'FAKE-save-image %s\n' "$*" >> "$FAKE_LOG"
        out="${!#}"
        if [ "${FAKE_SAVE_EMPTY:-0}" != "1" ]; then
            printf 'stub image\n' > "$out"
        fi
        ;;
    *)
        printf 'fake espflash: unexpected subcommand %s\n' "$*" >&2
        exit 99
        ;;
esac
EOF
cat > "$work/bin/cargo" <<'EOF'
#!/usr/bin/env bash
printf 'FAKE-cargo %s\n' "$*" >> "$FAKE_LOG"
exit 1
EOF
chmod +x "$work/bin/espflash" "$work/bin/cargo"

c6_app="$work/target/riscv32imac-esp-espidf/release/rustyfarian-rgb-clock"
c3_app="$work/target/riscv32imc-esp-espidf/release/rustyfarian-rgb-clock"
bl="$work/bootloader.bin"
: > "$c6_app"
: > "$c3_app"
: > "$bl"
log="$work/espflash.log"
out="$work/out/image.bin"

# Fake device directory for scripts/detect-port.sh: a save must not care how many there are.
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

failures=0
total=0

# run_case <name> <expected status> <output must contain> <log must contain|-> <image: yes|no> -- <command...>
# The log needle is "-" when no fake espflash or cargo call may happen.
# Every case runs with ESPFLASH_PORT and DRY_RUN cleared, so the developer's shell cannot leak in.
# Invariant for every case: no board is probed or flashed.
run_case() {
    local name="$1" want_status="$2" needle="$3" log_needle="$4" want_image="$5"
    shift 6
    total=$((total + 1))
    : > "$log"
    rm -rf "$work/out"
    mkdir -p "$work/out"
    set +e
    output="$(env -u ESPFLASH_PORT -u DRY_RUN PATH="$work/bin:$PATH" CARGO_TARGET_DIR="$work/target" \
        FAKE_LOG="$log" DETECT_PORT_DEV_DIR="$devdir" "$@" 2>&1)"
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
    case "$(cat "$log")" in *FAKE-board-info*|*FAKE-flash*) ok=no ;; esac
    if [ "$want_image" = "yes" ] && [ ! -s "$out" ]; then ok=no; fi
    if [ "$want_image" = "no" ] && [ -s "$out" ]; then ok=no; fi
    if [ "$ok" = "yes" ]; then
        if [ "$verbose" = "1" ]; then printf 'ok       %s\n' "$name"; fi
    else
        failures=$((failures + 1))
        printf 'FAIL     %s — exit %d (wanted %d)\n' "$name" "$status" "$want_status" >&2
        printf '           wanted output substring: %s\n' "$needle" >&2
        printf '           wanted log substring: %s\n' "$log_needle" >&2
        printf '%s\n' "$output" | sed 's/^/           /' >&2
        printf '           log: %s\n' "$(cat "$log")" >&2
    fi
}

# --- flash-image.sh save mode ------------------------------------------------

run_case "C6 image: chip, 4mb flash, partition table, IDF bootloader and app" \
    0 "Saving the merged image of idf_c6_rgb_clock" \
    "save-image --chip esp32c6 --merge --flash-size 4mb --partition-table partitions.csv --bootloader $bl --ignore-app-descriptor $c6_app $out" \
    yes -- "$SCRIPT_DIR/flash-image.sh" --save-image "$out" idf_c6_rgb_clock "$bl"

run_case "C3 image: esp32c3 chip, riscv32imc app, 4mb flash" \
    0 "Saving the merged image of idf_c3_rgb_clock" \
    "save-image --chip esp32c3 --merge --flash-size 4mb --partition-table partitions.csv --bootloader $bl --ignore-app-descriptor $c3_app $out" \
    yes -- "$SCRIPT_DIR/flash-image.sh" --save-image "$out" idf_c3_rgb_clock "$bl"

stage_ports 2
run_case "saving needs no port, even with several boards attached" \
    0 "Saving the merged image" "FAKE-save-image" \
    yes -- "$SCRIPT_DIR/flash-image.sh" --save-image "$out" idf_c3_rgb_clock "$bl"
stage_ports 0

run_case "dry run prints the save command and calls no espflash" \
    0 "[dry-run] would run: espflash save-image --chip esp32c6 --merge --flash-size 4mb" "-" \
    no -- env DRY_RUN=1 "$SCRIPT_DIR/flash-image.sh" --save-image "$out" idf_c6_rgb_clock "$bl"

rm -f "$bl"
run_case "missing bootloader is refused and nothing is saved" \
    1 "expected artifact is missing" "-" \
    no -- "$SCRIPT_DIR/flash-image.sh" --save-image "$out" idf_c6_rgb_clock "$bl"
: > "$bl"

run_case "an empty or missing saved image is refused" \
    1 "espflash save-image left no image" "FAKE-save-image" \
    no -- env FAKE_SAVE_EMPTY=1 "$SCRIPT_DIR/flash-image.sh" --save-image "$out" idf_c6_rgb_clock "$bl"

run_case "a leftover image is removed, so an empty save cannot pass as the new one" \
    1 "espflash save-image left no image" "FAKE-save-image" \
    no -- bash -c 'printf "stale image\n" > "$1"; shift; exec "$@"' _ "$out" \
    env FAKE_SAVE_EMPTY=1 "$SCRIPT_DIR/flash-image.sh" --save-image "$out" idf_c6_rgb_clock "$bl"

run_case "a stray FLASH_SAVE_IMAGE in the environment does not turn a flash into a save" \
    0 "[dry-run] would run: espflash flash --port /dev/fake-port" "-" \
    no -- env DRY_RUN=1 FLASH_SAVE_IMAGE="$out" "$SCRIPT_DIR/flash-image.sh" idf_c6_rgb_clock "$bl" /dev/fake-port

# --- save-image.sh guard -----------------------------------------------------
# Relative paths resolve against the repo root, as under `just`.

cd "$ROOT_DIR"

run_case "a path git would track is refused before any build" \
    1 "would be tracked by git" "-" \
    no -- "$SCRIPT_DIR/save-image.sh" idf_c3_rgb_clock save-image-tests-tracked.bin

run_case "a git-ignored tmp/ path passes the guard and reaches the build" \
    1 "" "FAKE-cargo build --release --target riscv32imc-esp-espidf" \
    no -- "$SCRIPT_DIR/save-image.sh" idf_c3_rgb_clock tmp/save-image-tests/out/guarded.bin

run_case "a tracked path given absolute from outside the repo is refused" \
    1 "would be tracked by git" "-" \
    no -- bash -c 'cd "$1"; shift; exec "$@"' _ "$HOME" \
    "$SCRIPT_DIR/save-image.sh" idf_c3_rgb_clock "$ROOT_DIR/save-image-tests-tracked.bin"

run_case "a tmp/../ path that lands on a tracked location is refused" \
    1 "would be tracked by git" "-" \
    no -- "$SCRIPT_DIR/save-image.sh" idf_c3_rgb_clock tmp/../save-image-tests-tracked.bin

# --- just bench-flash refusals (before any build) ----------------------------

run_case "bench-flash refuses DRY_RUN, which would upload a stale image" \
    1 "DRY_RUN=1 would upload a stale image" "-" \
    no -- env DRY_RUN=1 RUSTYFARIAN_BENCH_DIR="$work/no-bench" just --justfile "$ROOT_DIR/justfile" bench-flash

run_case "bench-flash names a missing rustyfarian-bench checkout" \
    1 "no rustyfarian-bench checkout at $work/no-bench" "-" \
    no -- env RUSTYFARIAN_BENCH_DIR="$work/no-bench" just --justfile "$ROOT_DIR/justfile" bench-flash

if [ -e "$ROOT_DIR/save-image-tests-tracked.bin" ]; then
    failures=$((failures + 1))
    printf 'FAIL     the refused tracked path was created anyway\n' >&2
    rm -f "$ROOT_DIR/save-image-tests-tracked.bin"
fi

if [ "$failures" -ne 0 ]; then
    printf '\n%d of %d save-image cases failed.\n' "$failures" "$total" >&2
    exit 1
fi
printf '\nAll %d save-image cases passed.\n' "$total"
