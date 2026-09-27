#!/usr/bin/env bash
set -euo pipefail
# check-partitions-tests.sh — regression suite for scripts/check-partitions.sh.
#
# Every fixture in scripts/partition-fixtures/ is run through the validator and
# asserted against an expected verdict and, for the failing ones, an expected
# message. The validator guards a layout whose mistakes are only discoverable after
# erasing a device, so it needs its own tests: a guard that silently stops guarding
# is worse than no guard, and each case below is one that previously slipped through.
#
# The fixtures are committed and live beside this script, so CI enforces every case
# and a fresh clone can run the suite. They are test inputs, not scratch — keeping
# them under tmp/ would make the guard real only on the machine that created them.
#
# Quiet on success — one summary line, since this runs on every `just verify`.
# Pass -v (or set VERBOSE=1) to list each case.
#
# Usage: scripts/check-partitions-tests.sh [-v]

verbose="${VERBOSE:-0}"
if [ "${1:-}" = "-v" ]; then
    verbose=1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VALIDATOR="$SCRIPT_DIR/check-partitions.sh"
FIXTURES="$SCRIPT_DIR/partition-fixtures"

if [ ! -d "$FIXTURES" ]; then
    printf 'Error: fixture directory missing: %s\n' "$FIXTURES" >&2
    exit 1
fi

# fixture | expected verdict | substring the output must contain | extra validator args
CASES=(
    "valid-hex.csv|pass|is a valid A/B OTA table"
    "valid-suffix-sizes.csv|pass|is a valid A/B OTA table"
    "valid-hex.csv|fail|past the|--flash-size 0x200000"
    "bad-out-of-order.csv|fail|is out of order"
    "bad-wrong-slot-subtype.csv|fail|ota_1 is declared app/factory — must be app/ota_1"
    "bad-otadata-subtype.csv|fail|otadata is declared data/nvs — must be data/ota"
    "bad-slot-typed-data.csv|fail|ota_0 is declared data/ota_0 — must be app/ota_0"
    "bad-app-misaligned.csv|fail|not 64 KiB aligned"
    "bad-slot-too-small.csv|fail|smaller than the measured"
    "bad-nvs-moved.csv|fail|provisioned credentials would be lost"
    "bad-duplicate-name.csv|fail|duplicate partition name"
    "bad-malformed-hex.csv|fail|is not a decimal, 0x-hex, or K/M-suffixed value"
    "bad-overlap.csv|fail|overlaps"
    "bad-past-flash-end.csv|fail|past the"
)

failures=0
for case in "${CASES[@]}"; do
    IFS='|' read -r fixture expect needle extra <<< "$case"
    path="$FIXTURES/$fixture"

    if [ ! -f "$path" ]; then
        printf 'MISSING  %s (no such fixture)\n' "$fixture" >&2
        failures=$((failures + 1))
        continue
    fi

    # $extra is deliberately unquoted: it is a space-separated option list for the
    # validator (e.g. "--flash-size 0x200000"), empty for most cases.
    set +e
    # shellcheck disable=SC2086
    output="$("$VALIDATOR" $extra "$path" 2>&1)"
    status=$?
    set -e

    if [ "$expect" = "pass" ]; then
        actual_ok=$([ "$status" -eq 0 ] && echo yes || echo no)
    else
        actual_ok=$([ "$status" -ne 0 ] && echo yes || echo no)
    fi

    if [ "$actual_ok" != "yes" ]; then
        printf 'FAIL     %s — expected to %s, exited %d\n' "$fixture" "$expect" "$status" >&2
        printf '%s\n' "$output" | sed 's/^/           /' >&2
        failures=$((failures + 1))
        continue
    fi

    case "$output" in
        *"$needle"*)
            if [ "$verbose" = "1" ]; then
                printf 'ok       %s (%s)\n' "$fixture" "$expect"
            fi
            ;;
        *)
            printf 'FAIL     %s — %s as expected, but the message changed\n' "$fixture" "$expect" >&2
            printf '           wanted substring: %s\n' "$needle" >&2
            printf '%s\n' "$output" | sed 's/^/           /' >&2
            failures=$((failures + 1))
            ;;
    esac
done

if [ "$failures" -ne 0 ]; then
    printf '\n%d of %d partition-validator cases failed.\n' "$failures" "${#CASES[@]}" >&2
    exit 1
fi

printf '\nAll %d partition-validator cases passed.\n' "${#CASES[@]}"
