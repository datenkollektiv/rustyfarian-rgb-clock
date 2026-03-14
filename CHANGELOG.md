# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

[Unreleased]: https://github.com/datenkollektiv/rustyfarian-rgb-clock/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/datenkollektiv/rustyfarian-rgb-clock/releases/tag/v0.1.0
