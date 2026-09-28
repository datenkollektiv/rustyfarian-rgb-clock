# Rustyfarian RGB Clock — development tasks
#
# The workspace defaults to the ESP32-C6 target (riscv32imac-esp-espidf) via
# .cargo/config.toml, so recipes that touch platform-independent crates
# explicitly pass --target to override it.

# Load the optional `.env` so its non-secret portal-prefill values (WIFI_SSID,
# MQTT_HOST, MQTT_PORT, MQTT_USER, MQTT_CLIENT_ID) and OTA server settings
# (OTA_HOST, OTA_PORT) reach the `cargo` build and OTA tooling behind these recipes.
# A missing `.env` is a no-op — the build falls back to hardcoded defaults and
# auto-detection. See `.env.example`.
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
verify: partition-check partition-check-tests flash-image-tests tick-topic-tests
    @cargo fmt --all -- --check || (printf '\nFormatting issues found — run `just pre-commit` to auto-fix.\n' >&2 && exit 1)
    cargo check
    cargo clippy -- -D warnings
    cargo clippy --features unhealthy -- -D warnings
    cargo test -p clock-pure --target {{ host_target }}

# full pre-commit verification: format, check, lint, test (modifies files — local use only)
[group("composite")]
pre-commit: fmt partition-check partition-check-tests flash-image-tests tick-topic-tests check clippy test

# CI-equivalent verification (non-modifying): format check, deny, check, lint, test
[group("composite")]
ci: fmt-check partition-check partition-check-tests flash-image-tests tick-topic-tests deny check clippy test

# ── Build & Check ────────────────────────────────────────────────────────────

# build firmware for a named chip target (e.g. idf_c6_rgb_clock, idf_c3_rgb_clock)
[group("build")]
build example="idf_c6_rgb_clock":
    scripts/build.sh "{{example}}"

# build demo-only unhealthy image for OTA hardware scenario 6 (never marks itself valid, reboots after 15 s so the bootloader rolls back); never enabled by `build`/`flash`
[group("build")]
build-unhealthy example="idf_c6_rgb_clock":
    scripts/build.sh "{{example}}" --features unhealthy

# check the firmware for a named chip target (e.g. idf_c6_rgb_clock, idf_c3_rgb_clock)
[group("build")]
check example="idf_c6_rgb_clock":
    #!/usr/bin/env bash
    set -euo pipefail
    eval "$(scripts/chip-env.sh '{{example}}')"
    printf 'Checking %s (MCU=%s, target=%s)...\n' '{{example}}' "$MCU" "$TARGET"
    MCU="$MCU" cargo check --target "$TARGET"

# validate partitions.csv against ESP-IDF alignment, capacity, and NVS-stability rules
[group("build")]
partition-check:
    @scripts/check-partitions.sh

# run the partition-validator regression suite (scripts/partition-fixtures)
[group("build")]
partition-check-tests:
    @scripts/check-partitions-tests.sh

# run the hardware-free regression suite for scripts/flash-image.sh (fake espflash; chip-mismatch and dry-run guards)
[group("build")]
flash-image-tests:
    @scripts/flash-image-tests.sh

# run the host unit tests for the build-time tick-topic override (build_support/tick_topic.rs)
[group("build")]
tick-topic-tests:
    @scripts/tick-topic-tests.sh

# generate the real flashable app image for a chip target and check it fits both OTA slots (needs a prior `just build`)
[group("build")]
image-check example="idf_c6_rgb_clock":
    scripts/check-image-size.sh "{{example}}"

# ── Flash & Monitor ──────────────────────────────────────────────────────────

# build and flash firmware; does not open the monitor — run `just monitor` after (e.g. idf_c6_rgb_clock, idf_c3_rgb_clock)
[group("flash")]
flash example="idf_c6_rgb_clock":
    scripts/flash.sh "{{example}}"

# one-time first flash of the OTA layout: validate, full-erase, then flash
# Erases NVS too, so the device must be re-provisioned over SoftAP afterwards.
[group("flash")]
flash-baseline example="idf_c6_rgb_clock": partition-check
    #!/usr/bin/env bash
    set -euo pipefail
    # Build and validate BEFORE erasing, and capture the bootloader that passed:
    # the flash after the erase reuses these artifacts and never rebuilds, so a
    # compile error cannot leave a wiped device with nothing to flash onto it.
    bl="$(scripts/preflight.sh "{{ example }}")"
    # Pin one port across the erase and the flash. An ambiguous port must not reach
    # a destructive step: with two boards attached, "let espflash choose" can erase
    # one and flash the other.
    port="${ESPFLASH_PORT:-$(scripts/detect-port.sh)}"
    if [ -z "$port" ]; then
        printf 'Error: no unique serial port detected.\n' >&2
        printf 'Set ESPFLASH_PORT explicitly (e.g. ESPFLASH_PORT=/dev/cu.usbmodem1101 just flash-baseline),\n' >&2
        printf 'or detach the other boards. Refusing to erase against an ambiguous port.\n' >&2
        exit 1
    fi
    printf 'Using serial port %s\n' "$port"
    # DRY_RUN=1 shows the exact device-touching commands, in order, without
    # prompting or running them; scripts/flash-image.sh honours it too.
    if [ "${DRY_RUN:-0}" = "1" ]; then
        printf '[dry-run] would run: espflash erase-flash --port %s\n' "$port"
    else
        printf '\nAbout to ERASE the whole chip, including provisioned Wi-Fi/MQTT credentials.\n'
        printf 'The device must be re-provisioned over SoftAP afterwards.\n'
        read -r -p 'Type ERASE to continue: ' confirm
        [ "$confirm" = "ERASE" ] || { printf 'Aborted — nothing was erased.\n' >&2; exit 1; }
        espflash erase-flash --port "$port"
    fi
    scripts/flash-image.sh "{{ example }}" "$bl" "$port"

# flash firmware and open serial monitor (e.g. idf_c6_rgb_clock, idf_c3_rgb_clock)
[group("flash")]
run example="idf_c6_rgb_clock": (flash example)
    just monitor

# identify the chip on the serial port before flashing (honours ESPFLASH_PORT)
# `--ignore-app-descriptor` disables espflash's own chip-model check, so this is
# the only guard against flashing a C6 image onto an attached C3.
[group("flash")]
board-info:
    #!/usr/bin/env bash
    set -euo pipefail
    port="$(scripts/detect-port.sh)"
    port_args=()
    if [ -n "$port" ]; then
        port_args=(--port "$port")
    fi
    espflash board-info ${port_args[@]+"${port_args[@]}"}

# open serial monitor (no flash)
[group("flash")]
monitor:
    #!/usr/bin/env bash
    set -euo pipefail
    port="$(scripts/detect-port.sh)"
    port_args=()
    if [ -n "$port" ]; then
        port_args=(--port "$port")
    fi
    espflash monitor ${port_args[@]+"${port_args[@]}"}

# erase ESP32 flash (needed after sdkconfig changes)
[confirm]
[group("flash")]
erase-flash:
    espflash erase-flash

# print the esp-idf-sys-built bootloader path that `just flash` will use for a chip target (e.g. idf_c6_rgb_clock, idf_c3_rgb_clock)
[group("flash")]
bootloader-path example="idf_c6_rgb_clock":
    #!/usr/bin/env bash
    set -euo pipefail
    . scripts/lib.sh
    eval "$(scripts/chip-env.sh '{{ example }}')"
    bl="$(find_idf_bootloader "$TARGET" "${CARGO_TARGET_DIR:-target}")"
    if [ -z "$bl" ]; then
        printf 'No IDF-built bootloader found for %s — run `just build {{ example }}` first.\n' "$TARGET" >&2
        exit 1
    fi
    printf '%s\n' "$bl"

# ── OTA ──────────────────────────────────────────────────────────────────────

# stage the OTA image, hash it, and write the manifest under tmp/ota/. Demo knobs are env vars (just arguments are positional): OTA_VARIANT=unhealthy builds the demo image; OTA_VERSION=X rebuilds with X baked in and advertised in the manifest; OTA_TRUNCATE=N stages a `<variant>-truncated` artefact serving only the first N bytes under the full-image hash; OTA_TICK_TOPIC=T subscribes the image to T instead of `tick` so it fails its health check with the publisher still running
[group("ota")]
ota-artifact example="idf_c6_rgb_clock" variant=env_var_or_default("OTA_VARIANT", "release") version=env_var_or_default("OTA_VERSION", "") truncate=env_var_or_default("OTA_TRUNCATE", "0") tick_topic=env_var_or_default("OTA_TICK_TOPIC", ""):
    #!/usr/bin/env bash
    set -euo pipefail
    case "{{variant}}" in
        release)   build_recipe=build ;;
        unhealthy) build_recipe=build-unhealthy ;;
        *) printf 'Error: variant must be "release" or "unhealthy", got "%s"\n' "{{variant}}" >&2; exit 1 ;;
    esac
    # A version override is baked into the image (build.rs reads it), so the
    # binary reports exactly what the manifest advertises. The next plain
    # `just build` rebuilds with the package version again.
    # Trimmed exactly as build.rs trims it, so a blank value means no override
    # and the staging directory names the topic the image really carries.
    tick_topic={{quote(tick_topic)}}
    tick_topic="${tick_topic#"${tick_topic%%[![:space:]]*}"}"
    tick_topic="${tick_topic%"${tick_topic##*[![:space:]]}"}"
    if [ -n "$tick_topic" ]; then
        # Demo-only tick topic, validated and baked in by build.rs; unset again
        # for the next plain build, which then subscribes to `tick`.
        export TICK_TOPIC_OVERRIDE="$tick_topic"
    fi
    if [ -n "{{version}}" ]; then
        FIRMWARE_VERSION_OVERRIDE="{{version}}" just "$build_recipe" "{{example}}"
    else
        just "$build_recipe" "{{example}}"
    fi
    scripts/check-image-size.sh "{{example}}"
    scripts/ota-artifact.sh "{{example}}" "{{variant}}" "{{version}}" "{{truncate}}" "$tick_topic"

# serve the OTA artefacts from tmp/ota/ over plain HTTP on OTA_PORT (default 8000, the port `ota-artifact` bakes into the manifest URL); requires `just ota-artifact` first
[group("ota")]
ota-serve port=env_var_or_default("OTA_PORT", "8000"):
    #!/usr/bin/env bash
    set -euo pipefail
    if [ ! -d tmp/ota ]; then
        printf 'Error: tmp/ota directory does not exist\n' >&2
        printf 'Run `just ota-artifact` first to stage the OTA image and manifest.\n' >&2
        exit 1
    fi
    printf 'Serving OTA artefacts from tmp/ota/ on port %s\n' "{{port}}"
    printf 'Note: http.server sends Content-Length, which the firmware downloader requires.\n' >&2
    printf 'This is plain HTTP for LAN demo only — not for production.\n' >&2
    python3 -m http.server --directory tmp/ota --bind 0.0.0.0 {{port}}

# publish the last artefact staged by `just ota-artifact` to ota/command (broker from .env; DRY_RUN=1 prints the command)
[group("ota")]
ota-push example="idf_c6_rgb_clock":
    #!/usr/bin/env bash
    set -euo pipefail
    command_file="tmp/ota/{{example}}/command.json"
    if [ ! -f "$command_file" ]; then
        printf 'Error: nothing staged for %s; run `just ota-artifact %s` first.\n' "{{example}}" "{{example}}" >&2
        exit 1
    fi
    scripts/ota-mqtt.sh pub ota/command "$(cat "$command_file")"

# ask the device running `from` to roll back to its previous slot (operator rollback; DRY_RUN=1 prints the command)
[group("ota")]
ota-rollback from:
    scripts/ota-mqtt.sh pub ota/command '{"action":"rollback","from":"{{from}}"}'

# publish a raw payload to ota/command, e.g. `just ota-command not-json` (DRY_RUN=1 prints the command)
[group("ota")]
ota-command payload:
    scripts/ota-mqtt.sh pub ota/command {{quote(payload)}}

# follow the device's OTA status messages on ota/status (Ctrl-C to stop)
[group("ota")]
ota-status:
    scripts/ota-mqtt.sh sub ota/status

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
    cargo clippy --features unhealthy -- -D warnings

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
