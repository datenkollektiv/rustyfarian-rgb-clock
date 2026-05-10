# AGENTS.md

> Cross-tool operating guide for AI coding agents working on `rustyfarian-rgb-clock`.

## Project Overview

ESP32-C6 (RISC-V) firmware that displays time using 12 WS2812 NeoPixel LEDs arranged as a clock face.
Time is received via MQTT; the real goal is validating a three-tier embedded testing pyramid (host tests → Wokwi simulation → hardware-in-the-loop) for future Rustyfarian projects.

## Architecture

Three layers with a strict hardware boundary:

- **`crates/clock-pure/`** — `#![no_std]` pure Rust library with no hardware dependencies.
  Provides `hour_to_index`, `minute_to_index`, `second_to_index`, `scale_color`, `add_colors`.
  Fully testable on the host machine without any embedded toolchain.
- **`src/rgb_clock.rs`** — Firmware display layer.
  `RGBClock` wraps `WS2812RMT` and maps `LocalTime` to 12 LED positions using `clock-pure`.
  Startup rainbow animation runs via `ferriswheel` until the first MQTT tick arrives.
- **`src/main.rs`** — Entry point.
  GPIO10 = 12-LED NeoPixel clock ring; GPIO8 = onboard status LED.
  Flow: Wi-Fi init → MQTT connect → subscribe `tick` → update clock.
  Main thread parks after setup; MQTT callbacks do all work.

External crates (git dependencies, path-patched locally via `.cargo/config.toml`):
- `rustyfarian-ws2812` → `ferriswheel` (rainbow effects), `rustyfarian-esp-idf-ws2812` (RMT driver)
- `rustyfarian-network` → `rustyfarian-esp-idf-wifi`, `rustyfarian-esp-idf-mqtt`

## Development Workflow

First-time setup:

```sh
cp .cargo/config.toml.dist .cargo/config.toml
cp .env.example .env   # fill in WIFI_SSID, WIFI_PASS, MQTT_HOST, MQTT_PORT, MQTT_CLIENT_ID
```

All operations go through `just` — run `just --list` to see all recipes:

```sh
just verify      # format-check + clippy + clock-pure host tests  (read-only; run before every PR)
just pre-commit  # same but also auto-formats files  (use locally before committing)
just test        # run clock-pure unit tests on host only
just build       # cargo build --release for ESP32-C6 (riscv32imac-esp-espidf)
just flash       # build + flash via espflash + open serial monitor
```

Credentials (`WIFI_SSID`, `WIFI_PASS`, `MQTT_HOST`, `MQTT_PORT`, `MQTT_CLIENT_ID`) are embedded at compile time.
`build.rs` reads `.env` and injects them as `env!()` constants — the binary carries no runtime config loading.

MQTT time format (topic `tick`):

```json
{"hour": 14, "minute": 23, "second": 45}
```

`hour` is 0–23 (24-hour); all three fields are mapped to 12 LED positions.

## Key Conventions

**LED index mapping is off by one from intuition.**
LED 0 = 1 o'clock; LED 11 = 12 o'clock (not zero).
Formula: `(hour + 11) % 12` for hours; `(minute + 55) % 60 / 5` for minutes/seconds.
This is exhaustively tested — any change to the mapping must pass `just test`.

**Hand overlap uses additive color blending.**
`clock-pure`'s `add_colors` (saturating addition) blends hands that share an LED.
Default colors: hour = blue `(0,0,1)`, minute = green `(0,1,0)`, second = red `(1,0,0)`.
All three at 12:00:00 → white `(1,1,1)`. Brightness is applied via `scale_color` at render time.

**Error handling:** `anyhow::Result` + `.context("description")` in firmware.
No `unwrap()` or `expect()` outside `main` or `#[cfg(test)]`.
Log errors with `log::warn!` or `log::error!` rather than panicking.
In `clock-pure`, return `Option` or a custom error type — `anyhow` must not appear there.

**Embedded timing:** Use `FreeRtos::delay_ms()` inside embedded loops.
`std::thread::sleep` does not yield to the FreeRTOS scheduler correctly on ESP-IDF.

**`clock-pure` must stay `no_std`:** no `Vec`, no `String`, no heap allocation.
`Copy` types, arrays, and `core::` APIs only.
Verify with `just test` (host build); any `std` leak breaks the no_std guarantee.

**Local development with sibling repos:**
`.cargo/config.toml` contains `[patch]` sections pointing to sibling directories.
Comment out all patches to build against the published GitHub refs.
The patched file is gitignored — never commit it.

**Never:** comment out failing tests, add `#[ignore]` without a documented reason,
or declare a task done without running `just verify`.

## Project Lore

Non-obvious technical discoveries — facts that caused surprising failures or took significant time to debug — are recorded in `docs/project-lore.md`.
Read it before starting any debugging task.
After resolving a non-obvious failure, add a finding if:
- the fix required knowledge not obvious from the error message, or
- diagnosing it took more than 15 minutes.

## Important Files

| File | Why to read first |
|---|---|
| `crates/clock-pure/src/lib.rs` | All clock math + full test suite — read before touching any index mapping |
| `src/rgb_clock.rs` | LED display logic, hand color constants, startup animation |
| `src/main.rs` | Hardware pin assignments, MQTT subscription, FreeRTOS thread structure |
| `justfile` | All available recipes and their purpose |
| `.env.example` | Required compile-time credentials |
| `docs/ROADMAP.md` | Current project priorities |
| `docs/project-lore.md` | Non-obvious discoveries and hard-won debugging insights |
