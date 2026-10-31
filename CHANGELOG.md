# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- OTA application layer in `src/ota/` (spec: `docs/features/ota-mvp-v1.md`):
  - `ota/command` accepts `{"manifest_url": …, "sig": …}` and `{"action": "rollback", "from": "<running version>"}`.
  - The MQTT callback only bounds, parses and enqueues (`try_send` plus an explicit busy flag).
  - A dedicated 16 KiB worker thread fetches the manifest through a bounded `EspHttpConnection` GET and refuses a `target` that is not this chip.
  - The worker applies `decide_update` from `rustyfarian-esp-idf-network 0.5.0`, streams the image via `OtaSession::fetch_and_apply`, and restarts.
  - Statuses go out on `ota/status` with fixed reason codes.
  - The worker logs its stack high-water mark after every job and before a successful restart, so the 16 KiB estimate can be measured on hardware.
  - The OTA threads use `FreeRtos::delay_ms` like the rest of the firmware.
- OTA health policy on the main thread after setup:
  - A `PENDING_VERIFY` slot is marked valid once MQTT is connected, an IPv4 lease is held, a `tick` rendered within the last 10 s, and 30 s have passed since boot.
  - A failed `mark_valid` keeps polling until the deadline.
  - At 120 s it writes a `rolled_back` record to NVS and calls `OtaSession::rollback()`.
  - The deadline rollback does not depend on NVS and retries `OtaSession::rollback()` three times.
  - A slot that is already valid is never touched, so a broker outage cannot reboot a working clock.
  - An update sent to a still-unverified image is refused early as `pending_verify`.
- OTA rollback reports:
  - `rolled_back` carries a `reason`.
  - Reports are two-stage: every reboot path only persists a request (reason plus the slot being left).
  - The next boot confirms a request into a publishable report only if a different slot is running, so a rollback that did not happen is never announced.
  - A report is published with `publish_acked` on the next boot and cleared only on a confirmed ack.
- OTA update identity:
  - `OTA_VERSION=X just ota-artifact` bakes `X` into the image through `FIRMWARE_VERSION_OVERRIDE` (validated in `build.rs`), not only into the manifest.
  - Before activation the worker persists an update-attempt record (promised version, target slot, activated flag) that every boot reconciles.
  - An attempt interrupted mid-download is discarded, and a stale one under a serial-flashed slot is ignored.
  - A booted image that does not report its promised version is rolled back as `rolled_back { version_mismatch }`.
  - A previous image booting after a crash, power loss or bootloader rollback reports `rolled_back { bootloader }`.
  - Activation is noted first thing in `main`, and update admission stays closed until the health policy has finished with the running slot.
- OTA tooling:
  - `just ota-artifact` (`scripts/ota-artifact.sh`) builds, size-checks and stages the image under `tmp/ota/`, hashes the exact `.bin` with SHA-256 and writes the manifest.
  - `OTA_VERSION=X` sets the version, `OTA_VARIANT=unhealthy` selects the demo image, and `OTA_TRUNCATE=N` serves a short body under the full-image hash (checksum-mismatch scenario).
  - `just ota-serve` serves `tmp/ota/` over plain HTTP.
  - `just build-unhealthy` builds the `--features unhealthy` demo image, which fails its health check and restarts after 15 s so the bootloader rolls back.
  - `just clippy` and `just verify` lint the `unhealthy` feature build as well as the default one.
- Firmware version bumped to 0.3.0 so the first OTA push is strictly newer than the flashed 0.2.0 baseline.
- A/B partition layout and rollback bootloader:
  - `ota_0` / `ota_1` / `otadata` with 1.75 MiB slots on 4 MiB flash.
  - `CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE=y`.
  - `scripts/flash.sh` builds first and flashes the `esp-idf-sys`-built bootloader with `--bootloader` / `--ignore-app-descriptor`; `just bootloader-path` resolves it.
  - espflash otherwise writes its own ESP-IDF v5.5.1 bootloader, so no bootloader setting in this repo previously reached the device.
- Flash safety:
  - `just flash-baseline` is the one-time first flash of the OTA layout; re-provisioning is required afterwards.
  - It builds and validates before erasing, pins one serial port across both the erase and the flash, and requires typing `ERASE` to confirm.
  - `scripts/flash-image.sh` is a flash-only step, so `flash-baseline` flashes artifacts validated before its erase.
  - Previously the post-erase step rebuilt, so a compile error left the device erased with nothing to flash onto it.
  - `scripts/preflight.sh` validates everything a flash depends on without touching the device: the app binary exists and exactly one `esp-idf-sys` bootloader resolves.
  - Preflight also requires that bootloader to be compiled with `CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE=y`; a cached older one leaves rollback inert while `mark_valid()` still reports success.
  - `just board-info` prints the attached chip type (honours `ESPFLASH_PORT`), since `--ignore-app-descriptor` disables espflash's own chip-model check.
  - `DRY_RUN=1` for `just flash` and `just flash-baseline` prints every device-touching command in order (erase, chip check, flash) without prompting or running any.
- Partition and image checks:
  - `just partition-check` validates 64 KiB app alignment, slot capacity against the measured image, `otadata` size, overlaps and NVS stability on the host.
  - It enforces exact type/subtype pairs (`app/ota_0`, `app/ota_1`, `data/ota`, `data/nvs`, `data/phy`), rejects duplicate names and requires 4 KiB alignment everywhere.
  - It guards against bash 3.2.
  - `scripts/check-partitions.sh --flash-size <bytes|0xHEX|4M>` overrides the 4 MiB default; both supported boards (C6-DevKitC-1 N4, C3-DevKitM-1 N4) ship 4 MiB.
  - `just partition-check-tests` is a 12-case regression suite over fixtures in `scripts/partition-fixtures/`, asserting both verdict and diagnostic.
  - Both partition recipes run in `just verify`, `just pre-commit`, `just ci` and the `rust.yml` CI job.
  - `just image-check` (`scripts/check-image-size.sh`) generates the exact image with `espflash save-image` and fails if it does not fit both OTA slots.
  - Preflight runs it before every flash and CI on every push, so growth is caught against the real artifact rather than the historical `MIN_SLOT` in `check-partitions.sh`.
- Flash-script tests:
  - `just flash-image-tests` runs ten hardware-free cases for `scripts/flash-image.sh` against a fake `espflash`.
  - Cases: chip-mismatch refusal, unreadable board info, missing artifacts, and a dry run that must never call espflash.
  - Port cases: `ESPFLASH_PORT` precedence, single-port resolution, and refusal with zero or several detected ports.
  - An invariant asserts that no refused run ever reaches `espflash flash`.
  - `scripts/detect-port.sh` honours `DETECT_PORT_DEV_DIR` so the tests can stage fake device nodes.
  - The suite runs in `just verify`, `just pre-commit`, `just ci` and CI.
- `just doctor` reports the bash on `PATH` and flags anything below 4, which the partition validator needs; macOS ships 3.2 as `/bin/bash`.
- CI job `build-c3` builds `idf_c3_rgb_clock` and runs `just image-check` on every push (1,417,120 bytes, 77% of a slot, on 2026-09-27); no Wokwi scenario, no hardware validation.
- SoftAP captive-portal provisioning for the ESP-IDF firmware replaces the `.env`-baked Wi-Fi and MQTT credentials (spec: `docs/features/wifi-softap-provisioning-v1.md`).
- Bare-metal `esp-hal` clock for the ESP32-C3 (spec: `docs/features/esp-hal-clock-v1.md`):
  - Crate `crates/rustyfarian-esp-hal-rgb-clock`, built next to the unchanged ESP-IDF firmware.
  - Drives the WS2812 ring on GPIO 4 with a rainbow at boot, then a fixed time (M1, hardware-validated 2026-10-27).
  - SoftAP captive-portal provisioning and Wi-Fi station mode through `rustyfarian-esp-hal-network`, credentials in raw flash at `0x9000` (M2, hardware-validated 2026-10-30); the network session owns SoftAP bring-up since the `01ff1b9` pin (re-validated 2026-10-31).
  - Boot-time re-entry via the upstream hook re-exported from `rustyfarian_esp_hal_network::provisioning`: a short BOOT (GPIO 9) press after reset reopens the portal and keeps the record (hardware-validated).
  - A 3 s BOOT hold erases the stored record first.
  - `just check-hal`, `clippy-hal`, `build-hal`, `flash-hal`, `verify-hal` and `clean-hal` take a chip argument (`c3` only), run `cargo +esp`, and have no OTA support.

### Validated

- 2026-09-27, both chip arms hardware-validated:
  - ESP32-C6 GPIO 18: the ring lit up on the pogo-pin test rig after flashing.
  - ESP32-C3 GPIO 4: a WS2812 strip (`just flash idf_c3_rgb_clock`, chip check `esp32c3` rev v0.4) showed the rainbow, then the clock with all three hands moving.
- 2026-09-27, the A/B OTA baseline and the September 2026 dependency wave on an ESP32-C6-DevKitC-1, first run (upstream had shipped the wave compile-verified only):
  - Erase, flash with the IDF-built rollback bootloader, SoftAP re-provisioning, MQTT `tick` rendering, and two power cycles all booted `ota_0` at `0x20000`.
  - One regression: the main task overflowed its stack in network `0.5.0`'s `wait_committed` after the portal committed, masked by the following reboot (see `docs/project-lore.md`).
  - Fixed the same day by the 16384-byte main stack (see Fixed), re-verified with a second erase, flash and provisioning cycle that also exercised the new live chip check.

### Changed

- OTA firmware downloads have a total deadline through the `rustyfarian-esp-idf-network 0.6.0` OTA runtime (`with_deadline`), so a trickling server no longer keeps the worker busy.
- The WS2812 ring DIN moved from GPIO 10 to the rustyfarian standard pin: ESP32-C3 GPIO 4, ESP32-C6 GPIO 18 (the `rustyfarian-ws2812` examples and shared pogo-pin rigs).
  - `src/main.rs` selects the pin via the `mcu` cfg from `build.rs`; the Wokwi diagram and docs follow.
  - There is no GPIO 10 fallback, so units wired to GPIO 10 need the data line moved.
- Flash scripts:
  - `scripts/flash-image.sh` (behind `just flash` and `just flash-baseline`) requires exactly one serial port: the single USB device `scripts/detect-port.sh` finds, or `ESPFLASH_PORT`.
  - It asks the chip what it is with `espflash board-info` before writing and refuses a mismatch; `--ignore-app-descriptor` had left that check to the operator.
  - `scripts/preflight.sh` runs `check-partitions.sh` before building, so plain `just flash` validates the table instead of relying on the `flash-baseline` recipe dependency.
  - `check-image-size.sh` requires exactly one `app/ota_0` and one `app/ota_1` row rather than any two OTA-like rows.
  - `just bootloader-path` takes the chip target (`idf_c6_rgb_clock` default, `idf_c3_rgb_clock`) instead of always resolving the C6 bootloader.
  - `DRY_RUN=1` output states that `espflash board-info` is skipped because no device command runs, rather than implying the chip was verified.
  - `just partition-check` rejects rows not in strictly increasing offset order.
  - The previous-row overlap check was only sound for sorted input, so an unsorted table could hide a real overlap behind a misleading "overlaps" message.
- Dependencies and toolchain:
  - `rustyfarian-esp-idf-network` moved from a git pin to crates.io `0.5.0` with the `ota` feature; `deny.toml` `allow-git` is empty and `Cargo.lock` has no git sources.
  - September 2026 rustyfarian wave: `rustyfarian-esp-idf-ws2812` and `ferriswheel` `0.6.0` → `0.7.0` (`pennant 0.7`).
  - Same wave: `esp-idf-hal` `0.46` → `0.47`, `esp-idf-svc` `0.52` → `0.53` (resolves `esp-idf-sys 0.38.1`), network git pin `8fc9f5f` → `fcf536d`.
  - The wave needed no firmware source changes.
  - Nightly `nightly-2025-12-01` → `nightly-2026-01-26` (`1.95.0-nightly`) and `rust-version` `1.77` → `1.95`, required by `rustyfarian-esp-idf-ws2812 0.7`.
  - CI: `actions/checkout` `v4` → `v5`, `extractions/setup-just` `v2` → `v4`, `actions/upload-artifact` `v4` → `v7`, `actions/download-artifact` `v4` → `v8`.
  - The CI bumps follow Node 20's removal from GitHub runners.

### Fixed

- `scripts/build.sh` and `scripts/chip-env.sh` no longer use `${1:?...}` with a `}` in the message, which closed the expansion early and leaked the usage text into the target name.
- `just monitor` builds its `--port` arguments with `${arr[@]+"${arr[@]}"}`, so an empty array no longer trips `set -u` on bash older than 4.4.
- The portal's OTA URL field no longer needs a placeholder:
  - Network `0.5.0` makes `ota_url` optional for `WifiMqttDevice` (ADR 014 amendment) and stores an empty value as "no OTA configured".
  - The `http://ota.invalid/` prefill and its `OTA_URL_PLACEHOLDER` constant are gone.
- `CONFIG_ESP_MAIN_TASK_STACK_SIZE` raised from 8000 to 16384 (the `rustyfarian-ws2812` / `rustyfarian-peripherals` default):
  - Network `0.5.0`'s `wait_committed` cloned `Option<ProvisioningConfig>` on the main stack and overran 8000 by 40 bytes right after the portal committed.
  - Provisioning then ended in a `Stack protection fault` and an extra reboot.
  - Upstream moved the clone off the caller's stack in network 0.6.0.
  - The bump needs `just clean-idf` to reach the generated `sdkconfig`.
  - Verified on the ESP32-C6 on 2026-09-27: after the clean rebuild, erase and re-provisioning logged "Provisioning committed — restarting into normal boot" with no fault.

### Security
- Fresh dependency resolution clears RUSTSEC-2026-0204 (`crossbeam-epoch` 0.9.18 → 0.9.21, via `embuild` 0.33.5) and the unsound-advisory warnings on `anyhow` (1.0.104) and `rand` (0.9.5).

## [0.2.0] - 2026-05-12

### Added
- Expanded `clock-pure` host test suite — color blending, edge cases, and hand-mapping boundary values.
- CI pipeline split into separate jobs with `cargo-deny` dependency audit.

### Changed
- Pinned cross-repo dependencies to release tags — ws2812 v0.5.0, network v0.2.1.
- Stabilised Wokwi simulation harness — merged binary with bootloader, UART console, and Tx/Rx wiring.
- Updated `esp-idf-hal` to `0.46` and migrated MQTT integration to `MqttBuilder` lifecycle-callback API.
- Renamed pure crate `clock-core` → `clock-pure` for consistency with the workspace `*-pure` convention.

### Removed
- Unused `experimental` Cargo feature gate (`esp-idf-svc/experimental`) — it was defined but never referenced in source code. No runtime behavior changed.

## [0.1.0] - 2026-02-13

### Added

- ESP32-C6 firmware that receives time via MQTT and displays it on 12 WS2812 LEDs arranged as a clock face.
- Three configurable clock hands — hour (blue), minute (green), second (red) — with additive color mixing when hands overlap.
- `clock-pure` crate: pure Rust, `no_std`-compatible utilities for mapping hour/minute/second values to 12-LED clock positions.
- Rainbow startup animation that runs until the first MQTT `tick` message is received, then hands off to the clock display.
- MQTT subscriber for the `tick` topic, expecting `{"hour": H, "minute": M, "second": S}` JSON payloads.
- Wi-Fi connection with an onboard LED status indicator during the connection phase.
- Compile-time credential embedding via `.env` and `build.rs` — no secrets in source control.
- Wokwi simulation configuration (`wokwi.toml`, `diagram.json`) for hardware-free testing of the firmware.
- GitHub Actions CI workflow with Wokwi-based automated smoke test and screenshot capture.
- `justfile` with standard recipes: `build`, `flash`, `monitor`, `check`, `clippy`, `test`, `fmt`, `doc`, `verify`, `ci`.
- Custom `partitions.csv` and `sdkconfig.defaults` tuned for the ESP32-C6-DevKitC-1.
- `.cargo/config.toml.dist` template with local path patch stubs for cross-repo development against `rustyfarian-ws2812` and `rustyfarian-network`.

[Unreleased]: https://github.com/datenkollektiv/rustyfarian-rgb-clock/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/datenkollektiv/rustyfarian-rgb-clock/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/datenkollektiv/rustyfarian-rgb-clock/releases/tag/v0.1.0
