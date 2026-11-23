# AGENTS.md

> Cross-tool operating guide for AI coding agents working on `rustyfarian-rgb-clock`.

## Project Overview

ESP32-C6 and ESP32-C3 (RISC-V) firmware that displays time using 12 WS2812 NeoPixel LEDs arranged as a clock face.
Time is received via MQTT; the real goal is validating a three-tier embedded testing pyramid (host tests → Wokwi simulation → hardware-in-the-loop) for future Rustyfarian projects.
Current stable release: v0.3.0.

## Architecture

Three layers with a strict hardware boundary:

- **`crates/clock-pure/`** — `#![no_std]` pure Rust library, fully testable on the host without any embedded toolchain.
  - Provides `hour_to_index`, `minute_to_index`, `second_to_index`, `scale_color`, `add_colors`.
- **`crates/clock-app/`** — Shared async clock app for both tiers: MQTT tick in, WS2812 frame out, no hardware dependencies.
- **`src/rgb_clock.rs`** — ESP-IDF ring adapter.
  - `IdfRing` wraps `Ws2812Rmt` and shows the frames `clock-app` renders.
  - `run_provisioning_animation` pulses the ring amber (`ferriswheel::PulseEffect`) while the SoftAP portal waits.
- **`src/main.rs`** — Entry point.
  - GPIO18 (C6) / GPIO4 (C3) = 12-LED NeoPixel ring.
  - GPIO8 = onboard status LED.
  - Flow: Wi-Fi init → MQTT connect (`MqttBuilder::subscribe`) → `clock-app` thread; in `ddi-lab` the DDI runtime starts at link-up or the failure deadline, then the health policy runs on the main thread.
  - Default build: the main thread parks after setup; the clock app and MQTT event loop run on background threads.
  - `ddi-lab` build: the clock app, MQTT event loop and DDI worker run on background threads, and the main thread runs the health policy.

External crates (from crates.io, path-patched locally via `[patch.crates-io]` in `.cargo/config.toml`):
- `rustyfarian-ws2812` → `ferriswheel` (effects), `rustyfarian-esp-idf-ws2812` (RMT driver)
- `rustyfarian-network` → `rustyfarian-esp-idf-network` (Wi-Fi, MQTT, provisioning, OTA), `juggler`

## Development Workflow

First-time setup:

```sh
cp .env.example .env   # optional portal prefill: WIFI_SSID, MQTT_HOST, MQTT_PORT, MQTT_USER (MQTT_CLIENT_ID is read by the esp-hal build only); DDI_BASE_URL is required by ddi-lab builds
```

All operations go through `just` — run `just --list` to see all recipes:

Host-safe (no device needed):

```sh
just verify      # host gate: partition and script suites, format check, host clippy and tests (read-only; run before every PR)
just verify-fw   # firmware gate: check and clippy for the ESP-IDF C6 target (no device needed, slow build)
just pre-commit  # format, then verify  (use locally before committing)
just test        # run clock-pure unit tests on host
```

Device-required (ESP32 must be connected):

```sh
just build       # build firmware for ESP32-C6; pass idf_c3_rgb_clock for ESP32-C3
just flash       # build and flash (no monitor)
just run         # build, flash, and open serial monitor
just monitor     # open serial monitor only
```

Remote bench (no local board; touches shared hardware, so leave `bench-flash` to the user):

```sh
just save-image idf_c3_rgb_clock tmp/bench/idf_c3_rgb_clock.bin   # merged whole-flash image, no board
just bench-flash bench-esp32c3-mini1 idf_c3_rgb_clock              # save, then flash a bench part via ../rustyfarian-bench pi-flash
```

Bare-metal esp-hal clock (ESP32-C3 only, no OTA). These recipes use the `esp` toolchain (`cargo +esp`, installed with `espup`) and default to the `c3` chip argument:

```sh
just check-hal c3    # type-check for riscv32imc-unknown-none-elf
just clippy-hal c3   # lint with warnings as errors
just verify-hal c3   # check + clippy; slow on the first run
just build-hal c3    # release build
just flash-hal c3    # build and flash, no monitor
just run-hal c3      # build, flash, and open serial monitor
just clean-hal       # remove the hal target directory
```

The hal crate is `crates/rustyfarian-esp-hal-rgb-clock`; it is not a workspace default member, so bare `cargo` and the IDF recipes stay on the ESP-IDF target.
It provisions Wi-Fi at runtime through SoftAP and reads only the non-secret `.env` prefill values at build time.

Wi-Fi and MQTT settings are entered at runtime on the SoftAP provisioning portal; `build.rs` bakes in only the optional, non-secret `.env` prefill values, plus `DDI_BASE_URL` for `ddi-lab` builds.

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

**Never call the MQTT client from an MQTT event callback.**
The ESP-IDF client deadlocks when a callback subscribes or publishes, because the event loop is blocked (ADR-005).
Topics (`tick`, `clock/config`, and in `ddi-lab` the nudge topic `rustyfarian/ota/<device-id>/nudge`) are subscribed through `MqttBuilder::subscribe`; the network crate resubscribes on every reconnect (see `src/main.rs`).
No `on_message` callback is registered: the clock app drains `receive()`, and IDF delivers each message to the callback or the queue, never both.

**Error handling:** `anyhow::Result` + `.context()` in firmware; no `unwrap()` or `expect()` outside `main` or `#[cfg(test)]`.
In `clock-pure`, return `Option` or a custom error type — `anyhow` must not appear there.

**Embedded timing:** Use `FreeRtos::delay_ms()` inside embedded loops. `std::thread::sleep` does not yield to the FreeRTOS scheduler correctly on ESP-IDF.

**`clock-pure` must stay `no_std`:** no `Vec`, no `String`, no heap allocation. `Copy` types, arrays, and `core::` APIs only.

**Local dev patches:** `.cargo/config.toml` is committed. To develop against sibling working trees, add a `[patch.crates-io]` section to it locally and never commit that section.

**Never:** comment out failing tests, add `#[ignore]` without a documented reason, or declare a task done without running `just verify` (plus `just verify-fw` after firmware changes).

## Project Lore

Non-obvious discoveries are recorded in `docs/project-lore.md`.
Read it before any debugging task; add an entry after any fix that took more than 15 minutes or required knowledge not obvious from the error message.

## Upstream Feature Requests

Deliver a request to another rustyfarian repository by copying its file into the target repo's `review-queue/`.
Request drafts are local working files and are not committed; committed docs refer to the upstream feature doc, roadmap item or release instead.

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
| `src/main.rs` | Pin assignments, MQTT and OTA setup, thread structure |
| `justfile` | All available recipes |
| `docs/ROADMAP.md` | Current priorities |
| `docs/project-lore.md` | Hard-won debugging insights |
