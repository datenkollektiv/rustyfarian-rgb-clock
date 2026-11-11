---
id: 004
title: The esp-hal clock's last log line before a provisioning reset is cut off
captured-on: 2026-10-30
doc-version: 2
status: closed
kind: observation
---

# Bug 004: The esp-hal clock's last log line before a provisioning reset is cut off

## Symptom
After `Committed`, the console shows `INF�` instead of `Provisioning committed — restarting into normal boot`, followed directly by the ROM banner.

## Suspected Cause
`esp_hal::system::software_reset()` runs right after `log::info!`, before UART0 has drained its TX FIFO.
A short busy-wait or a UART flush before the reset would keep the line.

## Linked Artefact
- `crates/rustyfarian-esp-hal-rgb-clock/src/main.rs` — end of the provisioning path (`COMMIT_FLUSH_MS`, `software_reset`)

## Reproduction Confidence
high

## Session Log
- 2026-10-30 — Observation captured during the M2 hardware test; cosmetic
- 2026-11-11 — Fixed: `run_portal` waits `LOG_DRAIN_MS` (50 ms) before `software_reset`, so UART0 drains the last line
- 2026-11-11 — Closed via /bug dialog; not yet hardware-verified, check the line on the next C3 run (M3)
