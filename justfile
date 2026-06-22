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

# ── Composite ────────────────────────────────────────────────────────────────
#
# Three intentionally distinct gates — keep their differences deliberate:
#   verify     — quick, read-only local gate. Does NOT modify files and does NOT
#                run `deny` (network access / slower). Use before opening a PR.
#   pre-commit — modifying: runs `fmt` (rewrites files), then the same checks.
#                Use locally right before committing.
#   ci         — read-only mirror of the GitHub workflows. Adds `deny` on top of
#                `verify`'s checks. This is the canonical "what CI runs" target.

# verify code quality without modifying files; suggests 'just pre-commit' on formatting issues
[group("composite")]
verify:
    @cargo fmt --all -- --check || (printf '\nFormatting issues found — run `just pre-commit` to auto-fix.\n' >&2 && exit 1)
    cargo check
    cargo clippy -- -D warnings
    cargo test -p clock-pure --target {{ host_target }}

# full pre-commit verification: format, check, lint, test (modifies files — local use only)
[group("composite")]
pre-commit: fmt check clippy test

# CI-equivalent verification (non-modifying): format check, deny, check, lint, test
[group("composite")]
ci: fmt-check deny check clippy test

# ── Build & Check ────────────────────────────────────────────────────────────

# build firmware for a named chip target (e.g. idf_c6_rgb_clock, idf_c3_rgb_clock)
[group("build")]
build example="idf_c6_rgb_clock":
    scripts/build.sh "{{example}}"

# check the firmware for a named chip target (e.g. idf_c6_rgb_clock, idf_c3_rgb_clock)
[group("build")]
check example="idf_c6_rgb_clock":
    #!/usr/bin/env bash
    set -euo pipefail
    eval "$(scripts/chip-env.sh '{{example}}')"
    printf 'Checking %s (MCU=%s, target=%s)...\n' '{{example}}' "$MCU" "$TARGET"
    MCU="$MCU" cargo check --target "$TARGET"

# ── Flash & Monitor ──────────────────────────────────────────────────────────

# build and flash firmware; does not open the monitor — run `just monitor` after (e.g. idf_c6_rgb_clock, idf_c3_rgb_clock)
[group("flash")]
flash example="idf_c6_rgb_clock":
    scripts/flash.sh "{{example}}"

# flash firmware and open serial monitor (e.g. idf_c6_rgb_clock, idf_c3_rgb_clock)
[group("flash")]
run example="idf_c6_rgb_clock": (flash example)
    just monitor

# open serial monitor (no flash)
[group("flash")]
monitor:
    #!/usr/bin/env bash
    set -euo pipefail
    port="$(scripts/detect-port.sh)"
    port_args=()
    [ -n "$port" ] && port_args=(--port "$port")
    espflash monitor "${port_args[@]}"

# erase ESP32 flash (needed after sdkconfig changes)
[confirm]
[group("flash")]
erase-flash:
    espflash erase-flash

# ── Code Quality ─────────────────────────────────────────────────────────────

# format all code
[group("quality")]
fmt:
    cargo fmt --all

# check formatting without modifying files
[group("quality")]
fmt-check:
    cargo fmt --all -- --check

# run clippy on the entire workspace (firmware, ESP-IDF target)
[group("quality")]
clippy:
    cargo clippy -- -D warnings

# run clippy on the pure clock-pure crate only (host target — no ESP-IDF toolchain needed)
[group("quality")]
clippy-pure:
    cargo clippy -p clock-pure --all-targets --target {{ host_target }} -- -D warnings

# run clock-pure unit tests on host
[group("quality")]
test:
    cargo test -p clock-pure --target {{ host_target }}

# run tests with stdout/stderr visible
[group("quality")]
test-verbose:
    cargo test -p clock-pure --target {{ host_target }} -- --nocapture

# watch and re-run tests on file changes (requires cargo-watch)
[group("quality")]
watch:
    cargo watch -x "test -p clock-pure --target {{ host_target }}"

# ── Documentation ────────────────────────────────────────────────────────────

# build rustdoc for clock-pure
[group("docs")]
doc:
    cargo doc -p clock-pure --target {{ host_target }} --no-deps

# build and open docs in browser
[group("docs")]
doc-open:
    cargo doc -p clock-pure --target {{ host_target }} --no-deps --open

# ── Maintenance ──────────────────────────────────────────────────────────────

# check dependency licenses, advisories, and bans
[group("maintenance")]
deny:
    cargo deny check

# check dependencies for known security vulnerabilities (requires cargo-audit)
[group("maintenance")]
audit:
    cargo audit

# update dependencies
[group("maintenance")]
update:
    cargo update

# clean build artifacts
[group("maintenance")]
clean:
    cargo clean

# clean only the IDF crate's build artifacts for both chips (needed after sdkconfig changes or chip switch)
[group("maintenance")]
clean-idf:
    cargo clean -p rustyfarian-rgb-clock
    rm -rf target/riscv32imac-esp-espidf/release/build/esp-idf-sys-*/
    rm -rf target/riscv32imc-esp-espidf/release/build/esp-idf-sys-*/

# set up local cargo config from the template
[group("maintenance")]
setup-cargo-config:
    cp .cargo/config.toml.dist .cargo/config.toml

# simulate CI dependency resolution locally (CI always generates a fresh lock without path patches)
[group("maintenance")]
lock-ci: setup-cargo-config
    cargo update -p pennant -p bunting -p ferriswheel -p rustyfarian-esp-idf-ws2812 -p rustyfarian-esp-idf-network
    @echo "Done. Run 'just setup-cargo-config' and restore your dev patches when finished."

# install required development tooling (cargo-deny, cargo-audit, cargo-watch)
[group("maintenance")]
setup:
    cargo install cargo-deny cargo-audit cargo-watch

# report development tooling status (Rust, nightly toolchain, espflash, ldproxy, .env)
[group("maintenance")]
doctor:
    @scripts/doctor.sh

# ── Local CI (act) ───────────────────────────────────────────────────────────

# run CI workflow locally via act (requires Docker + act)
[group("local-ci")]
act-ci:
    act -j host-tests

# run format-check workflow locally via act (requires Docker + act)
[group("local-ci")]
act-fmt:
    act -j fmt

# run audit workflow locally via act (requires Docker + act)
[group("local-ci")]
act-audit:
    act -j audit

# run all CI workflows locally via act (requires Docker + act)
[group("local-ci")]
act-all: act-fmt act-ci act-audit
