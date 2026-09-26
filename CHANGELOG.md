# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed
- Adopted the September 2026 rustyfarian release wave: `rustyfarian-esp-idf-ws2812` and `ferriswheel` `0.6.0` → `0.7.0` (`pennant 0.7`), `esp-idf-hal` `0.46` → `0.47`, `esp-idf-svc` `0.52` → `0.53` (resolves `esp-idf-sys 0.38.1`), and the pre-release `rustyfarian-esp-idf-network` git pin `8fc9f5f` → `fcf536d`.
  No firmware source changes were needed.
- Pinned nightly `nightly-2025-12-01` → `nightly-2026-01-26` (`1.95.0-nightly`) and `rust-version` `1.77` → `1.95`, required by `rustyfarian-esp-idf-ws2812 0.7`.
- CI: `actions/checkout` `v4` → `v5`, `extractions/setup-just` `v2` → `v4`, `actions/upload-artifact` `v4` → `v7`, and `actions/download-artifact` `v4` → `v8` (Node 20 was removed from GitHub runners).

### Fixed
- Provisioning portal prefills the required OTA URL with the placeholder `http://ota.invalid/` on a fresh device, so a submission no longer fails on it; the firmware implements no OTA and never uses the value.

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
