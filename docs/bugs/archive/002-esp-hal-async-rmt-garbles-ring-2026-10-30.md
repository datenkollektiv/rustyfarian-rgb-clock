---
id: 002
title: The esp-hal clock's async WS2812 transmit garbles the C3 ring
captured-on: 2026-10-30
doc-version: 2
status: closed
kind: defect
---

# Bug 002: The esp-hal clock's async WS2812 transmit garbles the C3 ring

## Symptom
On the esp-hal C3 clock, the rainbow and provisioning pulse flicker, and the fixed time shows one or two stray green LEDs instead of the three hands.

## Suspected Cause
Confirmed by control images on the same board, ring and wiring.
A 12-LED frame is 289 pulse codes; a C3 RMT channel holds 48, so the transmit refills half-buffers while it runs.
`esp-hal 1.2.2` does that refill in `TxFuture::poll` (`src/rmt.rs:1986`), not in the ISR: the interrupt only wakes the task.
Each refill has a ~30 µs deadline (24 codes × 1.25 µs), and executor wake-up latency misses it at random; Wi-Fi makes it worse.
When the deadline is missed, the RMT re-sends stale codes and the LEDs latch shifted bits.

## Linked Artefact
- `crates/rustyfarian-esp-hal-rgb-clock/src/main.rs` — `Ring` type (`Ws2812Rmt<'static, esp_hal::Async, RMT_BUFFER>`), `show`, `.into_async()`
- `rustyfarian-esp-hal-ws2812 0.7.0`, feature `async`
- `esp-hal =1.2.2`, `src/rmt.rs` — `Channel<Async, Tx>::transmit`, `TxFuture::poll`
- `docs/features/esp-hal-clock-v1.md` — M2 State
- `docs/project-lore.md` — "esp-hal Tier"

## Reproduction Confidence
high

## Severity
high

## Environment
ESP32-C3 rev v0.3 (UART bridge `/dev/cu.usbserial-210`), 12-LED ring on GPIO4, no level shifter, ring powered from the board's 5 V pin.
Branch `scouting-2026-10-30` at `fcaa69a`; control images built from `ebf792b` (M1) in a throwaway worktree.

## Expected Behaviour
Every frame renders exactly: a smooth rainbow, then blue on LED 11, green on LED 5 and red on LED 10 for `FIXED_TIME` 12:34:56.

## Actual Behaviour

| Image | Result |
|:--|:--|
| M2, async, Wi-Fi up | Garbled; never seen to settle |
| M1, async, no radio | Garbled, then correct after a few frames |
| M1, async, `ESP_HAL_CONFIG_PLACE_RMT_DRIVER_IN_RAM=true` | Garbled, settles on one green LED |
| M1, blocking driver inside `critical_section::with` | Clean from the first frame |

The last row rules out the unshifted 3.3 V data line, the cable and the supply.

## Reproduction Steps
1. `just flash-hal c3` on the current branch, then `just monitor-log tmp/<name>.log`.
2. Watch the rainbow during boot and the fixed time after `Got IP address`.
3. For the control, check out `ebf792b` and flash it the same way; the rainbow flickers without any radio.

## Suggested Fix Area
- Switch `Ring` to `Ws2812RmtBlocking` and wrap `set_pixels_slice` in `critical_section::with` (the validated control edit: 5 lines plus `critical-section = "1"`, `async` feature dropped).
- Cost: interrupts off for ~0.4 ms per frame (12 × 30 µs); the rerun must show Wi-Fi, DHCP and the portal tolerate that at 30 ms rainbow frames.
- `show` stays `async fn` for its callers or becomes sync; `ring_task` keeps its structure either way.
- Rejected: `place-rmt-driver-in-ram` (tested, no help); `with_memsize(2)` only doubles the slack and still depends on executor latency.
- Upstream: worth a request to `rustyfarian-ws2812` (the async driver silently depends on executor latency).

## Owner

## Links
- `docs/issues/led-display-signal-debugging.md` — same symptom class on the IDF C6 tier
- `docs/bugs/archive/003-esp-hal-boot-reentry-window-too-short-2026-10-30.md` — found in the same hardware test

## Session Log
- 2026-10-30 — Captured as a defect during the M2 hardware test; root cause pinned with four control images
- 2026-10-30 — Fixed: `Ring` is `Ws2812RmtBlocking`, `show` wraps `set_pixels_slice` in `critical_section::with`, no `.into_async()`, ws2812 `async` feature dropped
- 2026-10-30 — Verified on the C3 in STA mode over three boots: clean rainbow, steady blue LED 11 / green LED 5 / red LED 10, Wi-Fi associated and `Got IP`, no disconnects
- 2026-10-30 — Amber portal pulse not yet seen with the new driver; checked in the M2 rerun; closed via /bug
