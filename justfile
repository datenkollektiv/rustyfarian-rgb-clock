# Rustyfarian RGB Clock — development tasks
#
# The workspace defaults to the ESP32-C6 target (riscv32imac-esp-espidf) via
# .cargo/config.toml, so recipes that touch platform-independent crates
# explicitly pass --target to override it.

# Load the optional `.env` so its non-secret portal-prefill values (WIFI_SSID,
# MQTT_HOST, MQTT_PORT, MQTT_USER) and the lab image's DDI_BASE_URL reach the
# `cargo` build behind these recipes.
# A missing `.env` is a no-op — the build falls back to hardcoded defaults, and
# only the `ddi-lab` builds refuse to start without DDI_BASE_URL. See `.env.example`.
set dotenv-load

host_target := `scripts/host-target.sh`
esp_target := "riscv32imac-esp-espidf"

# The host-testable crates (no hardware dependencies), shared by every host recipe.
host_pkgs := "-p clock-pure -p clock-app"

# `ddi-lab` builds need a compiled-in DDI_BASE_URL (build.rs refuses a build without it).
# Check, lint and CI size builds are never flashed, so they fall back to a placeholder;
# `build-lab`, `flash-lab` and `ddi-bundle` take the real one from `.env` or the shell.
# An empty DDI_BASE_URL counts as unset: `env()` would hand the empty string to build.rs.
ddi_check_url := if env("DDI_BASE_URL", "") == "" { "http://ddi.invalid/DEFAULT" } else { env("DDI_BASE_URL") }

# act: a local rustyfarian-ci checkout if RUSTYFARIAN_CI points at one, otherwise the pushed @v1.
ci_dir := env("RUSTYFARIAN_CI", "")
ci_repo_flag := if ci_dir != "" { '--local-repository "datenkollektiv/rustyfarian-ci@v1=' + absolute_path(ci_dir) + '"' } else { "" }

# bench-flash: the rustyfarian-bench checkout whose `pi-flash` uploads to the remote bench.
bench_dir := env("RUSTYFARIAN_BENCH_DIR", justfile_directory() / "../rustyfarian-bench")

# Build RAM disk (macOS, optional): when `just ramdisk attach` has mounted
# /Volumes/RustBuilds, cargo's target-dir is redirected there via CARGO_TARGET_DIR
# to spare the SSD and speed up esp-idf-sys rebuilds. Falls back to ./target when the
# disk is not mounted, so Linux/CI (which never mount it) are unaffected.
# rust-analyzer keeps using ./target (it doesn't inherit this).
# RUSTBUILDS_RAMDISK (shell or `.env`) overrides the path; a nonexistent one, e.g.
# RUSTBUILDS_RAMDISK=/nonexistent, keeps builds in ./target, and unlike `--set ramdisk`
# it also reaches the nested `just` calls inside recipes (ci-build, setup, run).
ramdisk := env("RUSTBUILDS_RAMDISK", "/Volumes/RustBuilds")
ramdisk_mounted := shell(justfile_directory() + '/scripts/ramdisk-mounted.sh "' + ramdisk + '"')
idf_dir := if ramdisk_mounted == "true" { ramdisk + "/targets/idf/" + file_name(justfile_directory()) } else { "target" }
export CARGO_TARGET_DIR := idf_dir

# Only when the target-dir lives on the RAM disk, install the heavy ESP-IDF tools to
# the shared global ~/.espressif so they don't land on (and fill) the RAM disk. Off
# the RAM disk — including CI, which never mounts it — keep the default per-project
# .embuild ("workspace"), matching `.cargo/config.toml`. This is why "global"
# must NOT live in the committed config: CI would install globally and break
# esp-idf-sys's ninja/compiler discovery.
export ESP_IDF_TOOLS_INSTALL_DIR := if ramdisk_mounted == "true" { "global" } else { "workspace" }

# Bare-metal esp-hal tier (ESP32-C3, `*-hal` recipes): its own target dir, isolated from
# the IDF output, because `-Zbuild-std=core,alloc` artifacts must not mix with the
# IDF `std` ones. The explicit --target-dir beats the exported CARGO_TARGET_DIR.
# The chip's target triple and feature come from scripts/hal-chip-env.sh.
hal_dir := if ramdisk_mounted == "true" { ramdisk + "/targets/hal/" + file_name(justfile_directory()) } else { "target/hal" }
# Link-time floor for the main stack (the portal's POST /save peaks near 20.5 KiB).
# Exported inside the `*-hal` recipes only, so the IDF build never sees it.
hal_stack_min := "24576"
hal_flags := "-Zbuild-std=core,alloc --target-dir " + hal_dir + " -p rustyfarian-esp-hal-rgb-clock"

# list available recipes (default)
_default:
    @just --list

# ── Composite ────────────────────────────────────────────────────────────────
#
# Gates, per the rustyfarian family recipe contract (rustyfarian-ci conventions/just.md):
#   verify     — read-only, offline, host-only: partition and script regression
#                suites (cheapest first), format check, host clippy and tests. No device target, no
#                network. Use before opening a PR.
#   verify-fw  — device gate: check and lint the ESP-IDF firmware for the C6 target
#                (needs the pinned nightly and an ESP-IDF build).
#   pre-commit — modifying: `fmt` (rewrites files), then `verify`. Local use only;
#                it does not run the device gate — run `just verify-fw` for firmware changes.
#   ci         — `verify` + `deny` (network), then the device gate.

# read-only, offline, host-only gate: partition and script suites, format check, host clippy and tests
[group("composite")]
verify: partition-check partition-check-tests flash-image-tests save-image-tests ddi-bundle-tests tick-topic-tests fmt-check clippy-pure test

# device gate: check and lint the ESP-IDF firmware for the C6 target (default, `ddi-lab` and the unhealthy demo feature)
[group("composite")]
verify-fw: check clippy

# format, then verify (modifies files — local use only)
[group("composite")]
pre-commit: fmt verify

# CI-equivalent gate (non-modifying): verify, deny, then the firmware device gate
[group("composite")]
ci: verify deny verify-fw

# ── Build & Check ────────────────────────────────────────────────────────────

# build release firmware for a named chip target (release-only, no debug build; e.g. idf_c6_rgb_clock, idf_c3_rgb_clock)
[group("build")]
build example="idf_c6_rgb_clock":
    scripts/build.sh "{{ example }}"

# build the lab-only DDI image (DDI client over plain HTTP; needs DDI_BASE_URL in `.env` or the shell); never a release image
[group("build")]
build-lab example="idf_c6_rgb_clock":
    scripts/build.sh "{{ example }}" --features ddi-lab

# build demo-only unhealthy lab image (never marks itself valid, reboots after 15 s so the bootloader rolls back; implies `ddi-lab`); never enabled by `build`/`flash`
[group("build")]
build-unhealthy example="idf_c6_rgb_clock":
    scripts/build.sh "{{ example }}" --features unhealthy

# check the firmware for a named chip target (e.g. idf_c6_rgb_clock, idf_c3_rgb_clock)
[group("build")]
check example="idf_c6_rgb_clock":
    #!/usr/bin/env bash
    set -euo pipefail
    eval "$(scripts/chip-env.sh '{{ example }}')"
    printf 'Checking %s (MCU=%s, target=%s)...\n' '{{ example }}' "$MCU" "$TARGET"
    MCU="$MCU" cargo check --target "$TARGET"
    MCU="$MCU" DDI_BASE_URL={{ quote(ddi_check_url) }} cargo check --target "$TARGET" --features ddi-lab

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

# run the hardware-free regression suite for the saved-image path (flash-image.sh save mode, save-image.sh git guard)
[group("build")]
save-image-tests:
    @scripts/save-image-tests.sh

# run the build-free regression suite for scripts/ddi-bundle.sh (fake espflash; version, chip-id, size and leftover-file guards)
[group("build")]
ddi-bundle-tests:
    @scripts/ddi-bundle-tests.sh

# run the host unit tests for the build-time tick-topic override (build_support/tick_topic.rs)
[group("build")]
tick-topic-tests:
    @scripts/tick-topic-tests.sh

# generate the real flashable app image for a chip target and check it fits both OTA slots (needs a prior `just build`)
[group("build")]
image-check example="idf_c6_rgb_clock":
    scripts/check-image-size.sh "{{ example }}"

# ── Flash & Monitor ──────────────────────────────────────────────────────────

# build and flash firmware; does not open the monitor — run `just monitor` after (e.g. idf_c6_rgb_clock, idf_c3_rgb_clock)
[group("flash")]
flash example="idf_c6_rgb_clock":
    scripts/flash.sh "{{ example }}"

# build and flash the lab-only DDI image (needs DDI_BASE_URL in `.env` or the shell); does not open the monitor
[group("flash")]
flash-lab example="idf_c6_rgb_clock":
    FIRMWARE_FEATURES=ddi-lab scripts/flash.sh "{{ example }}"

# Erases NVS too, so the device must be re-provisioned over SoftAP afterwards.
[doc('one-time first flash of the OTA layout: validate, full-erase, then flash')]
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

# `--ignore-app-descriptor` disables espflash's own chip-model check, so this is
# the only guard against flashing a C6 image onto an attached C3.
[doc('identify the chip on the serial port before flashing (honours ESPFLASH_PORT)')]
[group("flash")]
board-info:
    scripts/espflash.sh board-info

# The SoftAP-MAC rule lives in scripts/device-id.sh, which also takes a MAC as its argument.
[doc("print the attached board's device id and DDI controller id, rgb-clock-<xxxx> (resets the board; honours ESPFLASH_PORT)")]
[group("flash")]
device-id:
    scripts/device-id.sh

# open serial monitor (no flash)
[group("flash")]
monitor:
    scripts/espflash.sh monitor

# capture the serial monitor to a file without a TTY, for agents and background logging (e.g. tmp/boot.log)
[group("flash")]
monitor-log file *args:
    scripts/espflash.sh monitor --non-interactive {{ args }} > '{{ file }}' 2>&1

# erase the whole flash, NVS included, so the board must be provisioned again (refuses an ambiguous port)
[confirm]
[group("flash")]
erase-flash:
    scripts/espflash.sh --require-port erase-flash

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

# The image holds the `.env` portal prefill (SSID, broker host, MQTT user), never a password
# or token, so `image` must be a path git ignores (tmp/…) or one outside the repo.
[doc("build a chip target and save its merged whole-flash image (IDF bootloader at 0x0, table at 0x8000) to `image`; touches no board")]
[group("flash")]
save-image example image:
    scripts/save-image.sh "{{ example }}" "{{ image }}"

# ── DDI (lab) ────────────────────────────────────────────────────────────────

# build the lab DDI image with `version` baked in and write the fleet's release bundle (.bin, .bin.sha256, .bin.version, .bin.chip) under tmp/ddi-bundle/; DDI_VARIANT=unhealthy bundles the demo image that fails its health check
[group("ddi")]
ddi-bundle example version:
    scripts/ddi-bundle.sh "{{ example }}" "{{ version }}"

# ── Remote bench ─────────────────────────────────────────────────────────────

# Runs before the build: a missing checkout fails fast, and DRY_RUN=1 is refused because
# the save would only print, while pi-flash (which has no dry run) uploads a stale image.
[private]
_bench-checkout:
    @[ "${DRY_RUN:-0}" != "1" ] || { printf 'bench-flash: DRY_RUN=1 would upload a stale image; use `just --dry-run bench-flash` or `DRY_RUN=1 just save-image …`\n' >&2; exit 1; }
    @[ -f "{{ bench_dir }}/justfile" ] || { printf 'bench-flash: no rustyfarian-bench checkout at %s; clone it there or set RUSTYFARIAN_BENCH_DIR\n' "{{ bench_dir }}" >&2; exit 1; }

# later: once a rustyfarian-bench release ships `flash`, call `rustyfarian-bench flash --part … IMAGE --wait`
# with RUSTYFARIAN_BENCH_URL, RUSTYFARIAN_BENCH_TOKEN and RUSTYFARIAN_BENCH_CA instead of the checkout's pi-flash.
[doc("save `example` to tmp/bench/<example>.bin and flash it onto bench part `part` without SSH; bench-esp32c3-mini1 takes idf_c3_rgb_clock (no C6 part yet; the bench refuses a chip mismatch); extra args after part and example go to pi-flash")]
[group("bench")]
[positional-arguments]
bench-flash part="bench-esp32c3-mini1" example="idf_c3_rgb_clock" *args: _bench-checkout (save-image example "tmp/bench/" + example + ".bin")
    #!/usr/bin/env bash
    set -euo pipefail
    # Absolute: the bench recipe resolves a relative path against its own checkout.
    image="{{ justfile_directory() }}/tmp/bench/$2.bin"
    # This justfile exports its build target dirs; keep them out of the bench's own recipes.
    env -u CARGO_TARGET_DIR -u ESP_IDF_TOOLS_INSTALL_DIR "{{ just_executable() }}" --justfile "{{ bench_dir }}/justfile" pi-flash --part "$1" "$image" --wait "${@:3}"

# ── Bare-metal esp-hal (ESP32-C3; C6 in M4) ────────────────────────────────────────────

# type-check the bare-metal esp-hal crate for a chip (e.g. c3)
[group("hal")]
check-hal chip="c3":
    #!/usr/bin/env bash
    set -euo pipefail
    chip_env="$(scripts/hal-chip-env.sh '{{ chip }}')"
    eval "$chip_env"
    export ESP_HAL_CONFIG_ENSURE_MAIN_STACK_MINIMUM={{ hal_stack_min }}
    cargo +esp check {{ hal_flags }} --target "$HAL_TARGET" --features "$HAL_FEATURE" --bins

# lint the bare-metal esp-hal crate for a chip with warnings-as-errors (e.g. c3)
[group("hal")]
clippy-hal chip="c3":
    #!/usr/bin/env bash
    set -euo pipefail
    chip_env="$(scripts/hal-chip-env.sh '{{ chip }}')"
    eval "$chip_env"
    export ESP_HAL_CONFIG_ENSURE_MAIN_STACK_MINIMUM={{ hal_stack_min }}
    cargo +esp clippy {{ hal_flags }} --target "$HAL_TARGET" --features "$HAL_FEATURE" --bins -- -D warnings

# build the bare-metal firmware for a chip, always --release as esp-hal recommends (e.g. c3)
[group("hal")]
build-hal chip="c3":
    #!/usr/bin/env bash
    set -euo pipefail
    chip_env="$(scripts/hal-chip-env.sh '{{ chip }}')"
    eval "$chip_env"
    export ESP_HAL_CONFIG_ENSURE_MAIN_STACK_MINIMUM={{ hal_stack_min }}
    cargo +esp build --release {{ hal_flags }} --target "$HAL_TARGET" --features "$HAL_FEATURE"

# build and flash the bare-metal firmware for a chip, no monitor; reuses the IDF bootloader when built, else espflash's bundled one (e.g. c3)
[group("hal")]
flash-hal chip="c3": (build-hal chip)
    scripts/flash-hal.sh '{{ chip }}' '{{ hal_dir }}' '{{ idf_dir }}'

# build, flash and monitor the bare-metal firmware for a chip; the monitor decodes backtraces against the ELF (e.g. c3)
[group("hal")]
run-hal chip="c3": (build-hal chip)
    scripts/flash-hal.sh '{{ chip }}' '{{ hal_dir }}' '{{ idf_dir }}' --monitor

# clean the bare-metal build artifacts
[group("hal")]
clean-hal:
    cargo clean --target-dir {{ hal_dir }}

# bare-metal gate: type-check and lint the esp-hal crate (not part of `verify`: needs build-std and a long first compile)
[group("hal")]
verify-hal chip="c3": (check-hal chip) (clippy-hal chip)

# ── Code Quality ─────────────────────────────────────────────────────────────

# format all code
[group("quality")]
fmt:
    cargo fmt --all

# check formatting without modifying files
[group("quality")]
fmt-check:
    cargo fmt --all -- --check

# run clippy on the firmware for a named chip target, default, `ddi-lab` and unhealthy (e.g. idf_c6_rgb_clock, idf_c3_rgb_clock)
[group("quality")]
clippy example="idf_c6_rgb_clock":
    #!/usr/bin/env bash
    set -euo pipefail
    eval "$(scripts/chip-env.sh '{{ example }}')"
    MCU="$MCU" cargo clippy --target "$TARGET" -- -D warnings
    MCU="$MCU" DDI_BASE_URL={{ quote(ddi_check_url) }} cargo clippy --target "$TARGET" --features ddi-lab -- -D warnings
    MCU="$MCU" DDI_BASE_URL={{ quote(ddi_check_url) }} cargo clippy --target "$TARGET" --features unhealthy -- -D warnings

# run clippy on the host crates clock-pure and clock-app (host target — no ESP-IDF toolchain needed)
[group("quality")]
clippy-pure:
    cargo clippy {{ host_pkgs }} --all-targets --target {{ host_target }} -- -D warnings

# run the clock-pure and clock-app unit tests on host
[group("quality")]
test:
    cargo test {{ host_pkgs }} --target {{ host_target }}

# run tests with stdout/stderr visible
[group("quality")]
test-verbose:
    cargo test {{ host_pkgs }} --target {{ host_target }} -- --nocapture

# watch and re-run tests on file changes (requires cargo-watch)
[group("quality")]
watch:
    cargo watch -x "test {{ host_pkgs }} --target {{ host_target }}"

# ── Documentation ────────────────────────────────────────────────────────────

# build rustdoc for clock-pure and clock-app
[group("docs")]
doc:
    cargo doc {{ host_pkgs }} --target {{ host_target }} --no-deps

# build and open the clock-pure docs in the browser (clock-pure only; `just doc` also builds clock-app)
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

# Covers both chips across all build profiles (release from `just build`/`flash`,
# debug from `just check`).
[doc("clean the IDF crate's stale esp-idf-sys artifacts (needed after sdkconfig changes or a chip switch)")]
[group("maintenance")]
clean-idf:
    cargo clean -p rustyfarian-rgb-clock
    rm -rf {{ idf_dir }}/riscv32imac-esp-espidf/*/build/esp-idf-sys-*/
    rm -rf {{ idf_dir }}/riscv32imc-esp-espidf/*/build/esp-idf-sys-*/

# simulate CI dependency resolution locally (CI always generates a fresh lock without path patches)
[group("maintenance")]
lock-ci:
    @git diff --quiet -- .cargo/config.toml || { echo "revert local .cargo/config.toml edits (e.g. [patch] sections) first" >&2; exit 1; }
    cargo update -p pennant -p bunting -p ferriswheel -p rustyfarian-esp-idf-ws2812 -p rustyfarian-esp-idf-network

# install what a fresh clone is missing: component idf (pinned nightly, ldproxy, espflash), hal (espup + esp toolchain) or tools (cargo-deny, cargo-audit, cargo-watch); empty = all; idempotent; ends with doctor
[group("maintenance")]
setup component="":
    #!/usr/bin/env bash
    set -euo pipefail
    ensure() { command -v "$1" >/dev/null 2>&1 || cargo install --locked --target {{ host_target }} "$2"; }
    case "{{ component }}" in
        ""|idf|hal|tools) ;;
        *) echo "setup: unknown component '{{ component }}' (use idf, hal or tools)" >&2; exit 1 ;;
    esac
    if [ -z "{{ component }}" ] || [ "{{ component }}" = idf ]; then
        pinned="$(sed -n 's/^channel *= *"\(.*\)"/\1/p' rust-toolchain.toml)"
        rustup toolchain install "$pinned" --component rust-src --component clippy
        ensure ldproxy ldproxy
        ensure espflash espflash
    fi
    if [ -z "{{ component }}" ] || [ "{{ component }}" = hal ]; then
        ensure espup espup
        if ! rustup toolchain list | awk '/^esp/ { found = 1 } END { exit !found }'; then
            echo "Installing the esp toolchain (cargo +esp)..."
            espup install
        fi
    fi
    if [ -z "{{ component }}" ] || [ "{{ component }}" = tools ]; then
        ensure cargo-deny cargo-deny
        ensure cargo-audit cargo-audit
        ensure cargo-watch cargo-watch
    fi
    # Tools whose install method is your choice: hints only.
    bash -c '[ "${BASH_VERSINFO[0]}" -ge 4 ]' || echo "hint: partition-check needs bash 4+ on PATH (macOS: brew install bash)"
    command -v act >/dev/null 2>&1 || echo "hint: \`just act\` needs act and Docker (macOS: brew install act)"
    just doctor

# report development tooling status (Rust, toolchain, espflash, ldproxy, .env, RAM disk)
[group("maintenance")]
doctor:
    @scripts/doctor.sh "{{ ramdisk }}" "{{ idf_dir }}"

# Verify an attach: `just ramdisk attach && just doctor` — the "ramdisk" row should
# read `ok … → target-dir: …/RustBuilds/…`; then `just check` builds into it.
[doc('manage the build RAM disk (macOS): just ramdisk attach | detach | prune [days]')]
[group("maintenance")]
ramdisk action *args:
    @scripts/ramdisk.sh "{{ action }}" {{ args }}

# ── CI jobs ──────────────────────────────────────────────────────────────────
#
# Each GitHub Actions job runs exactly one of these (rustyfarian-ci conventions).
# Host recipes pass --target {{ host_target }}; CI exports RUSTUP_TOOLCHAIN=stable,
# overriding the pinned nightly in rust-toolchain.toml.

# CI job: check formatting
[group("ci")]
ci-fmt: fmt-check

# CI job: lint the host crates
[group("ci")]
ci-clippy: clippy-pure

# CI job: partition table, partition-validator, flash-script and tick-topic suites, then host tests
[group("ci")]
ci-test: partition-check partition-check-tests flash-image-tests save-image-tests ddi-bundle-tests tick-topic-tests test

# CI job: cargo-deny licenses, advisories, bans and sources
[group("ci")]
ci-deny: deny

# CI job: cargo-audit advisories
[group("ci")]
ci-audit: audit

# Wokwi reads the console from UART0, so CI appends it to sdkconfig.defaults before
# building; outside CI (`CI` unset) the tracked file is left alone. The merged image
# is the firmware the wokwi job downloads.
[doc('CI job: build, lint and size-check the C6 firmware, then write the merged Wokwi image')]
[group("ci")]
ci-build:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ "${CI:-}" = "true" ]; then
        echo "CONFIG_ESP_CONSOLE_UART_DEFAULT=y" >> sdkconfig.defaults
    else
        echo "ci-build: not in CI, skipping the UART console append to sdkconfig.defaults" >&2
    fi
    # The lab image is the larger build, so it is the one that outgrows the OTA slots first.
    DDI_BASE_URL={{ quote(ddi_check_url) }} cargo build --release --features ddi-lab
    just image-check
    cargo build --release
    just clippy
    just image-check
    out="{{ idf_dir }}/{{ esp_target }}/release"
    bootloader="$(ls -t "$out"/build/esp-idf-sys-*/out/build/bootloader/bootloader.bin | head -1)"
    espflash save-image --chip esp32c6 --flash-size 4mb --merge \
        --bootloader "$bootloader" \
        --partition-table partitions.csv \
        "$out/rustyfarian-rgb-clock" \
        "$out/rustyfarian-rgb-clock.bin"

# CI job: build the C3 firmware, lab image first, and check both images fit the OTA slots
[group("ci")]
ci-build-c3:
    DDI_BASE_URL={{ quote(ddi_check_url) }} just build-lab idf_c3_rgb_clock
    just image-check idf_c3_rgb_clock
    just build idf_c3_rgb_clock
    just image-check idf_c3_rgb_clock

# The Wokwi scenarios themselves run in wokwi/wokwi-ci-action; this recipe holds the
# job's shell steps: `prepare` before the action, `check <startup> <frames>` after it
# with the two scenario step outcomes.
[doc('CI job: Wokwi shell steps — just ci-wokwi prepare | check <startup-outcome> <frames-outcome>')]
[group("ci")]
ci-wokwi stage="prepare" startup="" frames="":
    #!/usr/bin/env bash
    set -euo pipefail
    case "{{ stage }}" in
        prepare)
            chmod +x target/{{ esp_target }}/release/rustyfarian-rgb-clock
            mkdir -p screenshots
            ;;
        check)
            if [[ "{{ startup }}" == "failure" ]]; then
                echo "::warning::Wokwi startup test failed - check screenshots"
            fi
            if [[ "{{ frames }}" == "failure" ]]; then
                echo "::warning::Wokwi frames test failed - check screenshots"
            fi
            if grep -qi "panic\|abort\|exception" screenshots/*.log 2>/dev/null; then
                echo "::error::Critical errors found in Wokwi serial logs"
            fi
            ;;
        *) echo "ci-wokwi: unknown stage '{{ stage }}' (use prepare or check)" >&2; exit 1 ;;
    esac

# ── Local CI (act) ───────────────────────────────────────────────────────────

# Runs natively (arm64 on Apple Silicon): amd64 emulation breaks GNU tar on 26.04.
# The family act-latest image ships no runner Clang set, so it does not reproduce the
# libclang 22 / ESP-IDF v6.1 break (lore "Toolchain & Dependencies").
# Tokens are dropped: the actions are public and act fails on a stale GH_TOKEN.
# No cache server: a restored target/ would hide toolchain changes, as a cache miss on CI would not.
# `build` ends in a failed "Upload firmware artifact": act 0.2.97's artifact server rejects upload-artifact@v7 (`mime_type`).
# Empty job runs the host jobs; name a firmware job explicitly (`just act build-c3`). Requires Docker + act.
[doc('run CI jobs via act in the family runner image: all host jobs, or one (`just act build-c3`)')]
[group("local-ci")]
act job="":
    #!/usr/bin/env bash
    set -euo pipefail
    for j in {{ if job == "" { "test deny fmt clippy audit" } else { job } }}; do
        env -u GH_TOKEN -u GITHUB_TOKEN act -j "$j" -P ubuntu-latest=catthehacker/ubuntu:act-latest --pull=false --no-cache-server --artifact-server-path tmp/act-artifacts {{ ci_repo_flag }}
    done
