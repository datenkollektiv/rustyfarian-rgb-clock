#!/usr/bin/env bash
set -euo pipefail
# ddi-bundle-tests.sh — build-free regression tests for scripts/ddi-bundle.sh.
#
# ddi-bundle.sh is the last check before an image goes to the fleet's DDI server:
# it must refuse a bad version label, an image of the wrong chip, a truncated or
# oversized image, and it must never leave a refused bundle on disk. These cases pin
# that down with a fake `espflash` on PATH whose `save-image` writes whatever image
# $FAKE_IMAGE names, and a prebuilt-ELF stand-in so nothing is compiled.
#
# Quiet on success — one summary line, since this runs on every `just verify`.
# Pass -v (or set VERBOSE=1) to list each case.
#
# Usage: scripts/ddi-bundle-tests.sh [-v]

verbose="${VERBOSE:-0}"
if [ "${1:-}" = "-v" ]; then
    verbose=1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
SUT="$SCRIPT_DIR/ddi-bundle.sh"

work="$ROOT_DIR/tmp/ddi-bundle-tests"
rm -rf "$work"
mkdir -p "$work/bin" "$work/out"

# save-image --chip <mcu> <elf> <bin>: the image is a 32-byte header-shaped file.
# Byte 0 is the magic, bytes 12..13 the little-endian chip id (5 = C3, 13 = C6).
cat > "$work/bin/espflash" <<'EOF'
#!/usr/bin/env bash
if [ "${1:-}" != "save-image" ]; then
    printf 'fake espflash: unexpected subcommand %s\n' "$*" >&2
    exit 99
fi
bin="${5:?fake espflash: missing output path}"
header() { # <magic octal> <chip id octal>
    printf "\\$1" > "$bin"
    head -c 11 /dev/zero >> "$bin"
    printf "\\$2\\000" >> "$bin"
    head -c 18 /dev/zero >> "$bin"
}
case "${FAKE_IMAGE:-c6}" in
    c6)        header 351 015 ;;
    c3)        header 351 005 ;;
    bad-magic) header 000 015 ;;
    short)     printf '\351\000\000\000' > "$bin" ;;
    big)       header 351 015; head -c 1900000 /dev/zero >> "$bin" ;;
    none)      ;;
    partial)   printf '\351\000' > "$bin"; exit 1 ;;
esac
EOF
chmod +x "$work/bin/espflash"

elf="$work/prebuilt.elf"
: > "$elf"

lock="$work/lock"

failures=0
total=0

# check <name> <command...>: a plain assertion counted like a case.
check() {
    local name="$1"
    shift
    total=$((total + 1))
    if "$@"; then
        if [ "$verbose" = "1" ]; then printf 'ok       %s\n' "$name"; fi
    else
        failures=$((failures + 1))
        printf 'FAIL     %s\n' "$name" >&2
    fi
}

# run_case <name> <expected status> <output must contain> <bundle name expected on disk|-|-stale> <env assignments...> -- <example> <version>
# The bundle name is "-" when NO bundle file may exist afterwards. "-stale" is the same,
# with a bundle of that name left by an earlier build: a refused image must take it away.
run_case() {
    local name="$1" want_status="$2" needle="$3" want_bundle="$4"
    shift 4
    local envs=()
    while [ "$1" != "--" ]; do envs+=("$1"); shift; done
    shift
    total=$((total + 1))
    rm -rf "$work/out"
    mkdir -p "$work/out"
    if [ "$want_bundle" = "-stale" ]; then
        for ext in bin bin.sha256 bin.version bin.chip; do
            printf 'stale\n' > "$work/out/$1-v$2.$ext"
        done
        want_bundle="-"
    fi
    set +e
    # The script's own knobs are cleared, so the developer's shell cannot leak into a case.
    output="$(env -u DDI_VARIANT -u CHECK_IMAGE_BIN -u TICK_TOPIC_OVERRIDE -u DDI_BUNDLE_LOCK_TIMEOUT \
        PATH="$work/bin:$PATH" DDI_BUNDLE_ELF="$elf" DDI_BUNDLE_OUT="$work/out" DDI_BUNDLE_LOCK="$lock" \
        ${envs[@]+"${envs[@]}"} "$SUT" "$@" 2>&1)"
    status=$?
    set -e
    local ok=yes
    if [ "$status" -ne "$want_status" ]; then ok=no; fi
    case "$output" in *"$needle"*) ;; *) ok=no ;; esac
    if [ "$want_bundle" = "-" ]; then
        if [ -n "$(ls -A "$work/out")" ]; then ok=no; fi
    else
        local bin="$work/out/$want_bundle.bin"
        for f in "$bin" "$bin.sha256" "$bin.version" "$bin.chip"; do
            [ -s "$f" ] || ok=no
        done
        # Exactly the four files: in particular never a .sig, which ingest refuses.
        if [ "$(ls -A "$work/out" | wc -l | tr -d ' ')" -ne 4 ]; then ok=no; fi
        if [ "$ok" = "yes" ]; then
            if command -v shasum >/dev/null 2>&1; then
                want_sha=$(shasum -a 256 "$bin" | awk '{print $1}')
            else
                want_sha=$(sha256sum "$bin" | awk '{print $1}')
            fi
            [ "$(cat "$bin.sha256")" = "$want_sha" ] || ok=no
            [ "$(cat "$bin.version")" = "$2" ] || ok=no
            case "$1" in
                idf_c6_*) [ "$(cat "$bin.chip")" = "esp32c6" ] || ok=no ;;
                idf_c3_*) [ "$(cat "$bin.chip")" = "esp32c3" ] || ok=no ;;
            esac
        fi
    fi
    if [ "$ok" = "yes" ]; then
        if [ "$verbose" = "1" ]; then printf 'ok       %s\n' "$name"; fi
    else
        failures=$((failures + 1))
        printf 'FAIL     %s — exit %d (wanted %d)\n' "$name" "$status" "$want_status" >&2
        printf '           wanted output substring: %s\n' "$needle" >&2
        printf '           wanted bundle: %s; out dir holds: %s\n' "$want_bundle" "$(ls -A "$work/out" | tr '\n' ' ')" >&2
        printf '%s\n' "$output" | sed 's/^/           /' >&2
    fi
}

run_case "C6 image yields the four bundle files" \
    0 "Chip:" "idf_c6_rgb_clock-v0.3.1" FAKE_IMAGE=c6 -- idf_c6_rgb_clock 0.3.1

check "a finished run releases its lock" test ! -e "$lock"

run_case "C3 image yields the four bundle files" \
    0 "Chip:" "idf_c3_rgb_clock-v1.20.300" FAKE_IMAGE=c3 -- idf_c3_rgb_clock 1.20.300

run_case "unhealthy variant is named apart from the lab image" \
    0 "Chip:" "idf_c6_rgb_clock-unhealthy-v0.3.2" FAKE_IMAGE=c6 DDI_VARIANT=unhealthy -- idf_c6_rgb_clock 0.3.2

run_case "two-part version is refused" \
    1 "version must be MAJOR.MINOR.PATCH" "-" FAKE_IMAGE=c6 -- idf_c6_rgb_clock 1.0

run_case "leading zero in a version part is refused" \
    1 "version must be MAJOR.MINOR.PATCH" "-" FAKE_IMAGE=c6 -- idf_c6_rgb_clock 01.0.0

run_case "version part above 65535 is refused" \
    1 "is above 65535" "-" FAKE_IMAGE=c6 -- idf_c6_rgb_clock 0.70000.0

run_case "version part of exactly 65535 is accepted" \
    0 "Chip:" "idf_c6_rgb_clock-v65535.65535.65535" FAKE_IMAGE=c6 -- idf_c6_rgb_clock 65535.65535.65535

run_case "version part of 65536 is refused" \
    1 "is above 65535" "-" FAKE_IMAGE=c6 -- idf_c6_rgb_clock 0.0.65536

run_case "six-digit version part is refused without arithmetic" \
    1 "is above 65535" "-" FAKE_IMAGE=c6 -- idf_c6_rgb_clock 123456.0.0

run_case "unknown variant is refused" \
    1 'DDI_VARIANT must be "lab" or "unhealthy"' "-" FAKE_IMAGE=c6 DDI_VARIANT=release -- idf_c6_rgb_clock 0.3.1

run_case "wrong app-image magic is refused and leaves no bundle" \
    1 "expected 233 (0xE9" "-stale" FAKE_IMAGE=bad-magic -- idf_c6_rgb_clock 0.3.1

run_case "C3 image for a C6 bundle is refused and leaves no bundle" \
    1 "image chip id at offset 12 is 5, expected 13 for esp32c6" "-stale" FAKE_IMAGE=c3 -- idf_c6_rgb_clock 0.3.1

run_case "image too short for a header is refused" \
    1 "too short to hold an app header" "-stale" FAKE_IMAGE=short -- idf_c6_rgb_clock 0.3.1

run_case "image larger than the OTA slot is refused" \
    1 "image does not fit the OTA slots" "-stale" FAKE_IMAGE=big -- idf_c6_rgb_clock 0.3.1

run_case "espflash writing no image is refused" \
    1 "produced no file" "-stale" FAKE_IMAGE=none -- idf_c6_rgb_clock 0.3.1

run_case "failing espflash leaves no partial image" \
    1 "espflash save-image failed" "-stale" FAKE_IMAGE=partial -- idf_c6_rgb_clock 0.3.1

# A failed build leaves no ELF; the stale bundle of the same version must be gone before
# the script stops, or a retry could upload the previous image under the new label.
# Only the missing-ELF branch runs here: DDI_BUNDLE_ELF skips build.sh, whose failure
# exits through `set -e` after the same cleanup.
run_case "missing ELF leaves no previous bundle" \
    1 "no release ELF at $work/missing.elf" "-stale" FAKE_IMAGE=c6 DDI_BUNDLE_ELF="$work/missing.elf" -- idf_c6_rgb_clock 0.3.1

# A lock held by a live run: the test shell's own PID stands in for it.
mkdir "$lock"
printf '%s\n' "$$" > "$lock/pid"
run_case "a lock held by a live run is refused at timeout 0 and leaves no bundle" \
    1 "another ddi-bundle run (process $$) holds" "-" FAKE_IMAGE=c6 DDI_BUNDLE_LOCK_TIMEOUT=0 -- idf_c6_rgb_clock 0.3.1
check "a refused run leaves the holder's lock alone" test -f "$lock/pid"
rm -rf "$lock"

# A lock whose holder has exited: a background shell that is gone by now.
sh -c 'exit 0' &
dead_pid=$!
wait "$dead_pid"
mkdir "$lock"
printf '%s\n' "$dead_pid" > "$lock/pid"
run_case "a lock left by an exited run is taken over" \
    0 "taking over the lock left by process $dead_pid" "idf_c6_rgb_clock-v0.3.1" FAKE_IMAGE=c6 -- idf_c6_rgb_clock 0.3.1
check "the taken-over lock is released at the end" test ! -e "$lock"

run_case "a non-numeric lock timeout is refused" \
    1 "DDI_BUNDLE_LOCK_TIMEOUT must be whole seconds" "-" FAKE_IMAGE=c6 DDI_BUNDLE_LOCK_TIMEOUT=soon -- idf_c6_rgb_clock 0.3.1

run_case "unknown chip in the example name is refused" \
    1 'Unknown chip "s3"' "-" FAKE_IMAGE=c6 -- idf_s3_rgb_clock 0.3.1

if [ "$failures" -ne 0 ]; then
    printf 'ddi-bundle-tests: %d of %d cases FAILED\n' "$failures" "$total" >&2
    exit 1
fi
printf 'ddi-bundle-tests: %d cases passed\n' "$total"
