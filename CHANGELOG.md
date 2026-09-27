# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- A/B OTA partition layout (`ota_0` / `ota_1` / `otadata`, 1.75 MiB slots on 4 MiB flash) with `just partition-check`, a host-side validator for 64 KiB app alignment, slot capacity against the measured image, `otadata` size, overlaps, and NVS stability
- `CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE=y`, plus `just bootloader-path` and a `scripts/flash.sh` that builds first and flashes the `esp-idf-sys`-built bootloader with `--bootloader` / `--ignore-app-descriptor`. espflash otherwise writes its own ESP-IDF v5.5.1 bootloader, so no bootloader setting in this repo previously reached the device
- `just flash-baseline` — one-time first flash of the OTA layout. Builds and validates *before* erasing, pins one serial port across both the erase and the flash, and requires typing `ERASE` to confirm; re-provisioning is required afterwards
- `scripts/preflight.sh` — builds and validates everything a flash depends on without touching the device: the app binary exists, exactly one `esp-idf-sys` bootloader resolves, and **that bootloader was compiled with `CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE=y`**. A cached bootloader predating the setting otherwise leaves rollback inert while `mark_valid()` still reports success
- `just partition-check` now also enforces exact type/subtype pairs (`app/ota_0`, `app/ota_1`, `data/ota`, `data/nvs`, `data/phy`), rejects duplicate names, requires 4 KiB alignment for every partition, and guards against bash 3.2
- `just partition-check-tests` — a 12-case regression suite over committed fixtures in `scripts/partition-fixtures/`, asserting both the verdict and the diagnostic for each way a table can be wrong. Both partition recipes run in `just verify`, `just pre-commit`, `just ci`, and the `rust.yml` CI job
- `scripts/flash-image.sh` — flash-only step that neither builds nor validates, so `just flash-baseline` can flash artifacts validated *before* its erase. Previously the post-erase step rebuilt, meaning a compile error left the device erased with nothing to flash onto it
- `just board-info` — prints the attached chip type (honours `ESPFLASH_PORT`) so the target can be confirmed before flashing, since `--ignore-app-descriptor` disables espflash's own chip-model check
- `just image-check` / `scripts/check-image-size.sh` — generates the exact app image with `espflash save-image` and fails if it does not fit both OTA slots. `scripts/preflight.sh` runs it before every flash and the CI build job runs it on every push, so firmware growth is caught against the real artifact rather than the historical `MIN_SLOT` constant in `check-partitions.sh`
- `just flash-image-tests` — five hardware-free cases for `scripts/flash-image.sh` driven by a fake `espflash`: chip-mismatch refusal, unreadable board info, missing artifacts, and a dry run that must never call espflash. Runs in `just verify`, `just pre-commit`, `just ci`, and CI
- `DRY_RUN=1` for `just flash` and `just flash-baseline` — prints every device-touching command in order (erase, chip check, flash) without prompting or running any of them
- `scripts/check-partitions.sh --flash-size <bytes|0xHEX|4M>` — overrides the 4 MiB default for modules with a different flash size; both supported boards (C6-DevKitC-1 N4, C3-DevKitM-1 N4) ship 4 MiB
- `just doctor` reports the bash on `PATH` and flags anything below 4, which the partition validator needs; macOS ships 3.2 as `/bin/bash`
- CI job `build-c3` — builds `idf_c3_rgb_clock` and runs `just image-check` on it, so the C3 target is compile- and size-verified on every push (1,417,120 bytes, 77% of a slot, on 2026-09-27); it has no Wokwi scenario and no hardware validation
- `scripts/detect-port.sh` honours `DETECT_PORT_DEV_DIR` so the flash-script tests can stage fake device nodes; `just flash-image-tests` now has ten cases, adding `ESPFLASH_PORT` precedence, single-port resolution, and refusal with zero or several detected ports, plus an invariant that no refused run ever reaches `espflash flash`

### Validated

- 2026-09-27: the A/B OTA baseline and the September 2026 dependency wave ran on an ESP32-C6-DevKitC-1 for the first time — erase, flash with the IDF-built rollback bootloader, SoftAP re-provisioning, MQTT `tick` rendering, and two power cycles all booting `ota_0` at `0x20000`. Upstream had shipped the wave compile-verified only. One regression found: the main task overflowed its stack inside network `0.5.0`'s `wait_committed` right after the portal committed (see `docs/project-lore.md`); the subsequent reboot masked it. Fixed the same day by the 16384-byte main stack below and re-verified with a second erase, flash and provisioning cycle, which also exercised the new live chip check

### Changed
- `scripts/flash-image.sh` (behind `just flash` and `just flash-baseline`) now requires exactly one serial port — the single USB device `scripts/detect-port.sh` finds, or `ESPFLASH_PORT` — and asks the attached chip what it is with `espflash board-info` before writing, refusing when it differs from the requested target. `--ignore-app-descriptor` had left that check to the operator
- `scripts/preflight.sh` runs `check-partitions.sh` before building, so the normal `just flash` path validates the table in the script itself instead of relying on the `flash-baseline` recipe dependency; `check-image-size.sh` now requires exactly one `app/ota_0` and one `app/ota_1` row rather than any two OTA-like rows
- `just bootloader-path` takes the chip target (`idf_c6_rgb_clock` default, `idf_c3_rgb_clock`) instead of always resolving the C6 bootloader
- `DRY_RUN=1` output states explicitly that `espflash board-info` is skipped because no device command runs, rather than implying the chip was verified
- `just partition-check` rejects tables whose rows are not in strictly increasing offset order; the previous-row overlap check was only sound for sorted input, and an unsorted table could hide a real overlap behind a misleading "overlaps" message
- `rustyfarian-esp-idf-network` moved from a git pin to crates.io `0.5.0` with the `ota` feature; `deny.toml` `allow-git` is now empty and `Cargo.lock` has no git sources
- Adopted the September 2026 rustyfarian release wave: `rustyfarian-esp-idf-ws2812` and `ferriswheel` `0.6.0` → `0.7.0` (`pennant 0.7`), `esp-idf-hal` `0.46` → `0.47`, `esp-idf-svc` `0.52` → `0.53` (resolves `esp-idf-sys 0.38.1`), and the pre-release `rustyfarian-esp-idf-network` git pin `8fc9f5f` → `fcf536d`.
  No firmware source changes were needed.
- Pinned nightly `nightly-2025-12-01` → `nightly-2026-01-26` (`1.95.0-nightly`) and `rust-version` `1.77` → `1.95`, required by `rustyfarian-esp-idf-ws2812 0.7`.
- CI: `actions/checkout` `v4` → `v5`, `extractions/setup-just` `v2` → `v4`, `actions/upload-artifact` `v4` → `v7`, and `actions/download-artifact` `v4` → `v8` (Node 20 was removed from GitHub runners).

### Fixed
- `scripts/build.sh` and `scripts/chip-env.sh` no longer use `${1:?...}` with a `}` inside the message, which closed the expansion early and leaked the usage text into the target name
- `just monitor` builds its `--port` arguments with the `${arr[@]+"${arr[@]}"}` idiom, so an empty array no longer trips `set -u` on bash older than 4.4
- The provisioning portal's OTA URL field no longer needs a placeholder. Network `0.5.0` makes `ota_url` optional for `WifiMqttDevice` (ADR 014 amendment) and stores an empty value as "no OTA configured", so the `http://ota.invalid/` prefill and its `OTA_URL_PLACEHOLDER` constant are gone.
- `CONFIG_ESP_MAIN_TASK_STACK_SIZE` raised from 8000 to 16384 (the `rustyfarian-ws2812` / `rustyfarian-peripherals` default).
  Network `0.5.0`'s `wait_committed` clones `Option<ProvisioningConfig>` on the main task's stack and overran 8000 by 40 bytes right after the portal committed, so provisioning ended in a `Stack protection fault` and an extra reboot.
  The clone itself is reported upstream in `docs/outbox/rustyfarian-network-wait-committed-stack-clone.md`; the bump needs `just clean-idf` to reach the generated `sdkconfig`.
  Verified on the ESP32-C6 on 2026-09-27: after the clean rebuild, a full erase and re-provisioning completed with the "Provisioning committed — restarting into normal boot" line and no fault

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
