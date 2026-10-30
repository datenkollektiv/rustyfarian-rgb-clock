---
id: 003
title: BOOT re-entry on the esp-hal clock cannot be triggered by hand
captured-on: 2026-10-30
doc-version: 1
status: open-defect
kind: defect
---

# Bug 003: BOOT re-entry on the esp-hal clock cannot be triggered by hand

## Symptom
Five RESET-then-BOOT attempts on the C3 never logged `BOOT held`; every boot went straight to `Provisioned (wifi_mqtt) — booting clock`.

## Suspected Cause
`boot_button_reentry` reads GPIO9 once and returns at once when it is high.
The hal image reaches that read ~250 ms after reset release (bootloader `Loaded app` at 251 ms), while the IDF boot that upstream's example targets leaves ~500 ms.
BOOT cannot be held earlier: GPIO9 is a strapping pin, and holding it through reset enters ROM download mode.

## Linked Artefact
- `crates/rustyfarian-esp-hal-rgb-clock/src/main.rs` — `boot_button_reentry`, `BOOT_HOLD_MS`, `BOOT_DEBOUNCE_MS`
- `rustyfarian-esp-idf-network` `examples/idf_c3_provision_mqtt.rs` at `bae2dc4` — the single-sample design this mirrors
- `docs/features/esp-hal-clock-v1.md` — Decisions row "BOOT (GPIO9) re-entry via `juggler::gate_reentry`, sampled once after reset"

## Reproduction Confidence
high

## Severity
medium

## Environment
ESP32-C3 rev v0.3, espflash-bundled bootloader (ESP-IDF v6.1-beta1), branch `scouting-2026-10-30` at `fcaa69a`, provisioned record in the store.

## Expected Behaviour
A short BOOT press right after RESET reopens the portal with the record kept; a 3 s hold erases the record and opens the portal.

## Actual Behaviour
The press is never seen, so neither path runs; the 3 s erase is untested for the same reason.

## Reproduction Steps
1. Provision the board, then start `just monitor-log tmp/<name>.log`.
2. Tap RESET and press BOOT as quickly as possible after releasing it; hold ~1 s.
3. The log shows a normal provisioned boot and no `BOOT held` line.

## Suggested Fix Area
- Poll GPIO9 for a short window (e.g. 1–2 s) after the "press BOOT now" log line, instead of one read.
- Cost: every boot is delayed by the window; the ring is dark then, so a brief indicator would help.
- The window length is a Decisions-table change in the feature doc; mention it to upstream, whose example has the same single read.
- After the fix, run both checks: short press → `Re-entry: portal requested — the stored record is kept`; 3 s hold → `Re-entry: stored record erased`.

## Owner

## Links
- `docs/bugs/archive/002-esp-hal-async-rmt-garbles-ring-2026-10-30.md` — found in the same hardware test

## Session Log
- 2026-10-30 — Captured as a defect during the M2 hardware test
