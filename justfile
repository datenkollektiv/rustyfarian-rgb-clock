# Rustyfarian RGB Clock — development tasks
#
# The workspace defaults to the ESP32-C6 target (riscv32imac-esp-espidf) via
# .cargo/config.toml, so recipes that touch platform-independent crates
# explicitly pass --target to override it.

# Load the optional `.env` so its non-secret portal-prefill values (WIFI_SSID,
# MQTT_HOST, MQTT_PORT, MQTT_USER, MQTT_CLIENT_ID) reach the `cargo` build behind
# these recipes. A missing `.env` is a no-op — the build falls back to the
# hardcoded defaults. See `.env.example`.
set dotenv-load := true

host_target := `scripts/host-target.sh`
esp_target  := "riscv32imac-esp-espidf"

# Build RAM disk (macOS, optional): when `just ramdisk attach` has mounted
# /Volumes/RustBuilds, cargo's target-dir is redirected there via CARGO_TARGET_DIR
# to spare the SSD and speed up esp-idf-sys rebuilds. Falls back to ./target when the
# disk is not mounted, so Linux/CI (which never mount it) are unaffected.
# rust-analyzer keeps using ./target (it doesn't inherit this).
ramdisk         := "/Volumes/RustBuilds"
ramdisk_mounted := shell(justfile_directory() + '/scripts/ramdisk-mounted.sh "' + ramdisk + '"')
idf_dir         := if ramdisk_mounted == "true" { ramdisk + "/targets/idf/" + file_name(justfile_directory()) } else { "target" }
export CARGO_TARGET_DIR := idf_dir

# Only when the target-dir lives on the RAM disk, install the heavy ESP-IDF tools to
# the shared global ~/.espressif so they don't land on (and fill) the RAM disk. Off
# the RAM disk — including CI, which never mounts it — keep the default per-project
# .embuild ("workspace"), matching `.cargo/config.toml.dist`. This is why "global"
# must NOT live in the committed config: CI would install globally and break
# esp-idf-sys's ninja/compiler discovery.
export ESP_IDF_TOOLS_INSTALL_DIR := if ramdisk_mounted == "true" { "global" } else { "workspace" }

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

# clean the IDF crate's stale esp-idf-sys artifacts for both chips, across all build
# profiles (release from `just build`/`flash`, debug from `just check`) — needed
# after sdkconfig changes or a chip switch
[group("maintenance")]
clean-idf:
    cargo clean -p rustyfarian-rgb-clock
    rm -rf {{ idf_dir }}/riscv32imac-esp-espidf/*/build/esp-idf-sys-*/
    rm -rf {{ idf_dir }}/riscv32imc-esp-espidf/*/build/esp-idf-sys-*/

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

# report development tooling status (Rust, toolchain, espflash, ldproxy, .env, RAM disk)
[group("maintenance")]
doctor:
    @scripts/doctor.sh "{{ ramdisk }}" "{{ idf_dir }}"

# manage the build RAM disk (macOS): just ramdisk attach | detach
# Verify an attach: `just ramdisk attach && just doctor` — the "ramdisk" row should
# read `ok … → target-dir: …/RustBuilds/…`; then `just check` builds into it.
[group("maintenance")]
ramdisk action:
    @scripts/ramdisk.sh "{{ action }}"

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
