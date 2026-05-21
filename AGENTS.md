# AGENTS.md

> Cross-tool operating guide for AI coding agents working on `rustyfarian-rgb-clock`.

## Project Overview

ESP32-C6 (RISC-V) firmware that displays time using 12 WS2812 NeoPixel LEDs arranged as a clock face.
Time is received via MQTT; the real goal is validating a three-tier embedded testing pyramid (host tests → Wokwi simulation → hardware-in-the-loop) for future Rustyfarian projects.
Current stable release: v0.2.0.

## Architecture

Three layers with a strict hardware boundary:

- **`crates/clock-pure/`** — `#![no_std]` pure Rust library. Provides `hour_to_index`, `minute_to_index`, `second_to_index`, `scale_color`, `add_colors`. Fully testable on the host without any embedded toolchain.
- **`src/rgb_clock.rs`** — Firmware display layer. `RGBClock` wraps `WS2812RMT` and maps `LocalTime` to 12 LED positions using `clock-pure`. Startup rainbow animation via `ferriswheel` runs until the first MQTT tick arrives.
- **`src/main.rs`** — Entry point. GPIO10 = 12-LED NeoPixel ring; GPIO8 = onboard status LED. Flow: Wi-Fi init → MQTT connect → subscribe-watcher thread → clock updates. Main thread parks after setup; MQTT callbacks do all work.

External crates (git dependencies, path-patched locally via `.cargo/config.toml`):
- `rustyfarian-ws2812` → `ferriswheel` (rainbow effects), `rustyfarian-esp-idf-ws2812` (RMT driver)
- `rustyfarian-network` → `rustyfarian-esp-idf-wifi`, `rustyfarian-esp-idf-mqtt`

## Development Workflow

First-time setup:

```sh
just setup-cargo-config
cp .env.example .env   # fill in WIFI_SSID, WIFI_PASS, MQTT_HOST, MQTT_PORT, MQTT_CLIENT_ID
```

All operations go through `just` — run `just --list` to see all recipes:

Host-safe (no device needed):

```sh
just verify      # format-check + clippy + clock-pure host tests  (read-only; run before every PR)
just pre-commit  # same but also auto-formats  (use locally before committing)
just test        # run clock-pure unit tests on host
```

Device-required (ESP32 must be connected):

```sh
just build       # build firmware for ESP32-C6; pass idf_c3_rgb_clock for ESP32-C3
just flash       # build and flash (no monitor)
just run         # build, flash, and open serial monitor
just monitor     # open serial monitor only
```

Credentials (`WIFI_SSID`, `WIFI_PASS`, `MQTT_HOST`, `MQTT_PORT`, `MQTT_CLIENT_ID`) are embedded at compile time via `build.rs` — no runtime config loading.

MQTT time format (topic `tick`):

```json
{"hour": 14, "minute": 23, "second": 45}
```

`hour` is 0–23 (24-hour); all three fields are mapped to 12 LED positions.

## Key Conventions

**LED index mapping is off by one from intuition.**
LED 0 = 1 o'clock; LED 11 = 12 o'clock (not zero).
Formula: `(hour + 11) % 12` for hours; `(minute + 55) % 60 / 5` for minutes/seconds.
Any change to the mapping must pass `just test`.

**Hand overlap uses additive color blending.**
`add_colors` (saturating addition) blends hands that share an LED.
Default: hour = blue `(0,0,1)`, minute = green `(0,1,0)`, second = red `(1,0,0)`.
Brightness is applied via `scale_color` at render time.

**MQTT subscribe must not be called from the `on_connect` callback.**
`esp-idf-svc 0.52+` deadlocks if `subscribe()` is called inside the MQTT event callback — the event loop is blocked and cannot process the SUBACK.
A dedicated watcher thread polls an `AtomicBool` flag set by `on_connect` and calls `subscribe()` from a normal thread context (see `src/main.rs`).
This constraint extends to reconnect handling: any code that reacts to connection events must signal the watcher thread rather than calling `subscribe()` directly from the callback.

**Error handling:** `anyhow::Result` + `.context()` in firmware; no `unwrap()` or `expect()` outside `main` or `#[cfg(test)]`. In `clock-pure`, return `Option` or a custom error type — `anyhow` must not appear there.

**Embedded timing:** Use `FreeRtos::delay_ms()` inside embedded loops. `std::thread::sleep` does not yield to the FreeRTOS scheduler correctly on ESP-IDF.

**`clock-pure` must stay `no_std`:** no `Vec`, no `String`, no heap allocation. `Copy` types, arrays, and `core::` APIs only.

**Local dev patches:** `.cargo/config.toml` (gitignored) contains `[patch]` sections pointing to sibling directories. Comment out all patches to build against published GitHub refs.

**Never:** comment out failing tests, add `#[ignore]` without a documented reason, or declare a task done without running `just verify`.

## Project Lore

Non-obvious discoveries are recorded in `docs/project-lore.md`.
Read it before any debugging task; add an entry after any fix that took more than 15 minutes or required knowledge not obvious from the error message.

## Coding Principles

- **State assumptions** before starting.
  If a task has multiple valid interpretations, present them rather than picking silently.
- **Simplicity first.**
  Minimum code that solves the problem.
  No features beyond what was asked.
  No abstractions for single-use code.
  No error handling for impossible scenarios.
- **Surgical changes.**
  Touch only what the task requires.
  Do not improve adjacent code, comments, or formatting.
  Every changed line should trace directly to the user's request.
- When your changes create orphans (unused imports, variables, functions), remove them.
  Do not remove pre-existing dead code unless asked.

## Important Files

| File | Why to read first |
|---|---|
| `crates/clock-pure/src/lib.rs` | All clock math and the full test suite |
| `src/rgb_clock.rs` | LED display logic, hand colors, startup animation |
| `src/main.rs` | Pin assignments, MQTT subscribe-watcher thread, FreeRTOS structure |
| `justfile` | All available recipes |
| `docs/ROADMAP.md` | Current priorities |
| `docs/project-lore.md` | Hard-won debugging insights |
