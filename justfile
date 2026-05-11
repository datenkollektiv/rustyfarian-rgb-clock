# Rustyfarian RGB Clock — development tasks
#
# The workspace defaults to the ESP32-C6 target (riscv32imac-esp-espidf) via
# .cargo/config.toml, so recipes that touch platform-independent crates
# explicitly pass --target to override it.

host_target := `scripts/host-target.sh`
esp_target  := "riscv32imac-esp-espidf"

# list available recipes (default)
_default:
    @just --list

# --- Build & Check -----------------------------------------------------------

# build firmware for a named chip target (e.g. idf_c6_rgb_clock, idf_c3_rgb_clock)
build example="idf_c6_rgb_clock":
    scripts/build.sh "{{example}}"

# check the firmware for a named chip target (e.g. idf_c6_rgb_clock, idf_c3_rgb_clock)
check example="idf_c6_rgb_clock":
    #!/usr/bin/env bash
    set -euo pipefail
    eval "$(scripts/chip-env.sh '{{example}}')"
    printf 'Checking %s (MCU=%s, target=%s)...\n' '{{example}}' "$MCU" "$TARGET"
    MCU="$MCU" cargo check --target "$TARGET"

# --- Flash & Monitor ---------------------------------------------------------

# build and flash firmware; does not open the monitor — run `just monitor` after (e.g. idf_c6_rgb_clock, idf_c3_rgb_clock)
flash example="idf_c6_rgb_clock":
    scripts/flash.sh "{{example}}"

# flash firmware and open serial monitor (e.g. idf_c6_rgb_clock, idf_c3_rgb_clock)
run example="idf_c6_rgb_clock": (flash example)
    just monitor

# open serial monitor (no flash)
monitor:
    #!/usr/bin/env bash
    set -euo pipefail
    port="$(scripts/detect-port.sh)"
    port_args=()
    [ -n "$port" ] && port_args=(--port "$port")
    espflash monitor "${port_args[@]}"

# erase ESP32 flash (needed after sdkconfig changes)
[confirm]
erase-flash:
    espflash erase-flash

# --- Code Quality ------------------------------------------------------------

# format all code
fmt:
    cargo fmt

# check formatting without modifying files
fmt-check:
    cargo fmt -- --check

# run clippy on the entire workspace
clippy:
    cargo clippy -- -D warnings

# run clock-pure unit tests on host
test:
    cargo test -p clock-pure --target {{ host_target }}

# run tests with stdout/stderr visible
test-verbose:
    cargo test -p clock-pure --target {{ host_target }} -- --nocapture

# --- Documentation -----------------------------------------------------------

# build rustdoc for clock-pure
doc:
    cargo doc -p clock-pure --target {{ host_target }} --no-deps

# build and open docs in browser
doc-open:
    cargo doc -p clock-pure --target {{ host_target }} --no-deps --open

# --- Maintenance -------------------------------------------------------------

# install required development tooling (cargo-deny, cargo-audit, cargo-watch)
setup:
    cargo install cargo-deny cargo-audit cargo-watch

# check dependency licenses, advisories, and bans
deny:
    cargo deny check

# check dependencies for known security vulnerabilities (requires cargo-audit)
audit:
    cargo audit

# update dependencies
update:
    cargo update

# clean build artifacts
clean:
    cargo clean

# clean only the IDF crate's build artifacts for both chips (needed after sdkconfig changes or chip switch)
clean-idf:
    cargo clean -p rustyfarian-rgb-clock
    rm -rf target/riscv32imac-esp-espidf/release/build/esp-idf-sys-*/
    rm -rf target/riscv32imc-esp-espidf/release/build/esp-idf-sys-*/

# watch and re-run tests on file changes (requires cargo-watch)
watch:
    cargo watch -x "test -p clock-pure --target {{ host_target }}"

# set up local cargo config from the template
setup-cargo-config:
    cp .cargo/config.toml.dist .cargo/config.toml

# simulate CI dependency resolution locally (CI always generates a fresh lock without path patches)
# useful to verify cross-repo crate unification before pushing
lock-ci: setup-cargo-config
    cargo update -p pennant -p bunting -p ferriswheel -p rustyfarian-esp-idf-ws2812 -p rustyfarian-esp-idf-wifi -p rustyfarian-esp-idf-mqtt -p rustyfarian-network-pure -p wifi-pure
    @echo "Done. Run 'just setup-cargo-config' and restore your dev patches when finished."

# --- Local CI (act) ----------------------------------------------------------

# run CI workflow locally via act (requires Docker + act)
act-ci:
    act -j host-tests

# run format-check workflow locally via act (requires Docker + act)
act-fmt:
    act -j fmt

# run audit workflow locally via act (requires Docker + act)
act-audit:
    act -j audit

# run all CI workflows locally via act (requires Docker + act)
act-all: act-fmt act-ci act-audit

# --- Composite ---------------------------------------------------------------

# full pre-commit verification: format, check, lint, test (modifies files — local use only)
pre-commit: fmt check clippy test

# verify code quality without modifying files; suggests 'just pre-commit' on formatting issues
verify:
    @cargo fmt -- --check || (printf '\nFormatting issues found — run `just pre-commit` to auto-fix.\n' >&2 && exit 1)
    cargo check
    cargo clippy -- -D warnings
    cargo test -p clock-pure --target {{ host_target }}

# CI-equivalent verification (non-modifying): format check, deny, check, lint, test
ci: fmt-check deny check clippy test
