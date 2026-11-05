---
gate: none
desk-work: available
---

# Feature: esp-hal Clock v1

A bare-metal `esp-hal` build of the clock, next to the unchanged ESP-IDF firmware, without OTA.
Missing upstream pieces become feature requests to the upstream repository, or code contributions where appropriate.

## Milestones

- **M1** — hal build setup and a C3 ring that boots and shows the startup animation and a fixed time.
- **M2** — SoftAP provisioning and Wi-Fi STA via `rustyfarian-esp-hal-network`.
- **M3** — MQTT `tick` subscription drives the hands; no-OTA parity with the IDF clock.
- **M4** — C6 arm (GPIO 18) once a C6 board is available again.
- **M5** — OTA, blocked on an upstream esp-hal port of the OTA consumer runtime.

## Decisions

|                                                                            Decision | Reason                                                                                 | Rejected Alternative                 |
|------------------------------------------------------------------------------------:|:---------------------------------------------------------------------------------------|:-------------------------------------|
|                                    New crate `crates/rustyfarian-esp-hal-rgb-clock` | Mirrors the knob esp-hal tier; IDF firmware stays at root                              | Feature flag in the root package     |
|                                             Not a `default-member` of the workspace | Bare `cargo` and IDF recipes stay on the espidf target                                 | `exclude` the crate                  |
|                                                          ESP32-C3 first, DIN GPIO 4 | User decision: the C6 board was given away                                             | C6 first; both chips at once         |
|                                        `rustyfarian-esp-hal-network` by git rev pin | esp-hal provisioning is only on unreleased main (`01ff1b9`); the `juggler` pin is gone | Local path patch; wait for 0.7       |
| BOOT (GPIO9) re-entry via `rustyfarian_esp_hal_network::provisioning::gate_reentry` | Upstream hook (part 6 of portal fallback); recovery without a cable                    | Wait for an esp-hal boot entry point |
|                     BOOT polled for 2 s after the prompt (bug 003); no ring cue yet | The hal app reaches the hook ~250 ms after reset, too early for one read               | One read like upstream; a 1 s window |
|                               Broker is an IPv4 literal; DNS request filed upstream | esp-hal MQTT has no DNS yet (upstream ADR 019 §2)                                      | Contribute DNS before M3             |
|                                                                  OTA left out of v1 | esp-hal tier has `EspHalOtaManager` only, no runtime                                   | Hand-roll an OTA runtime locally     |
|                                         `clock-pure` becomes `no_std` outside tests | It uses nothing from `std` already                                                     | Copy the logic into the hal crate    |
|                                       Follow the knob build pattern (ADR 006 there) | One config, `-Zbuild-std=core,alloc` per recipe, `target/hal`                          | Second workspace                     |
|                                  Store at `0x9000`/`0x6000` in the default NVS area | Default table has no free flash; survives reflash (ADR 015 §4 deviation)               | Dedicated partition (M5)             |
|                            SoftAP bring-up owned by the network session (`start()`) | Upstream since `6f6286f`; no local `ApConfig` workaround; log SSID from `ap_mac()`       | Local SoftAP bring-up in `main.rs`   |
|                                    72 KiB heap, 24 KiB link-time main-stack minimum | Portal `POST /save` peaks near 20.5 KiB on the shared stack                            | Smaller heap; unchecked stack        |
|                            Blocking WS2812 transmit inside `critical_section::with` | esp-hal 1.2.2 refills async RMT in `poll`; misses the ~30 µs deadline                  | Async driver; RMT driver in RAM      |

## Constraints

- The ESP-IDF build must not change behaviour: `just verify`, `just build` and `just flash` keep working.
- Exact pins of the September 2026 wave: `esp-hal =1.2.2`, `esp-rtos =0.4.0`, `rustyfarian-esp-hal-ws2812 0.7`.
- Target `riscv32imc-unknown-none-elf` on the `esp` toolchain (1.95), no `anyhow` in the hal crate; the IDF nightly pin stays.
- Every build, check and flash goes through `just` (`*-hal` recipes); temporary files go in `tmp/`.
- The C3 console is the UART bridge (`esp-println` `uart`), not USB-Serial-JTAG.
- Wi-Fi init starts the scheduler, so there is no second `esp_rtos::start`: the ring starts after init and is dark for a few hundred ms.
- `ESP_HAL_CONFIG_ENSURE_MAIN_STACK_MINIMUM=24576` is exported by the `*-hal` recipes only; the link fails if the main stack is smaller.
- The hal provisioning store writes raw flash in the NVS area, not the NVS format, so switching tiers on one board needs re-provisioning.
- Accepted limitation, as on the IDF tier: the open SoftAP portal exposes the build-time prefill and lets anyone in range claim the device.
- Returning a board from the hal image to the IDF A/B firmware needs a full `just flash`, since the hal image uses espflash's default partition table.

## Open Questions

- [x] Bootloader: reuse the IDF v5.3.3 bootloader from `just build idf_c3_rgb_clock`, as the knob does?
  - Yes when built; otherwise espflash falls back to its bundled one (`scripts/flash-hal.sh`).
- [x] Does `esp-hal 1.2.2` build on `nightly-2026-01-26`, or does the hal crate need its own toolchain?
  - No: `esp-sync` needs the stable `cfg_select` (Rust 1.95).
  - The `*-hal` recipes use `cargo +esp` (1.95.0-nightly, April 2026).
  - The IDF pin is untouched.
- [x] Partition table: can the hal image reuse `partitions.csv`, or does it need a hal-specific one?
  - M1 uses espflash's default table (app at 0x10000).
  - The A/B layout waits for M5.
- [x] Partition table, further: M2 keeps the default table and puts the store in the NVS area (`0x9000`, 24 KiB), which the hal image does not otherwise use.
- [x] Does the hal tier need the Wi-Fi failure fallback to the portal?
  - Not for M2: log the reason, retry every 5 s, recover with `just erase-flash`.
- [x] Does `FactoryResetRequested` need an erase path?
  - Upstream gap: M2 only resets.
- [ ] When does upstream plan an esp-hal port of the OTA consumer runtime (blocks M5)?

## State

- [x] Design approved
- [x] M1 — build setup and C3 ring (hardware-validated 2026-10-27: rainbow, then the clock hands)
  - 2026-10-30 rerun: the same async image garbles the first frames, then settles; the 2026-10-27 pass was likely luck (bug 002).
  - C3 flash check expects, for `FIXED_TIME` 12:34:56: blue on LED 11 (12 o'clock), green on LED 5 (6 o'clock), red on LED 10 (11 o'clock), all three separate; LED 0 is the 1 o'clock position.
- [x] M2 — provisioning and Wi-Fi (hardware-validated on the C3 2026-10-30)
  - Passed: unprovisioned boot → amber pulse, portal `Rustyfarian-XXXX`, `POST /save` → `Committed`, software reset, STA join, `Got IP`.
  - Passed: clean rainbow and steady hands (blue LED 11, green LED 5, red LED 10), Wi-Fi unaffected by the blocking ring transmit.
  - Passed: BOOT short press → portal with the record kept, then STA rejoin; 3 s hold → record erased, re-provisioned, STA join.
  - Fixed on the way: garbled ring (`docs/bugs/archive/002-esp-hal-async-rmt-garbles-ring-2026-10-30.md`) and BOOT window (`docs/bugs/archive/003-esp-hal-boot-reentry-window-too-short-2026-10-30.md`).
  - Open: the "restarting into normal boot" line is cut off by the reset; cosmetic (`docs/bugs/004-commit-restart-log-line-cut-2026-10-30.md`).
  - Deferred: a static ring cue during the BOOT window; it needs the ring built before `gate_reentry`.
  - Known gap: a wrong Wi-Fi password retries forever with the ring on the rainbow; recover with `just erase-flash`.
  - 2026-10-30 pin move to `01ff1b9`: re-entry types come from `rustyfarian-esp-hal-network`, the session owns SoftAP bring-up, store read-back fix `7727443` included.
  - Passed on the C3 2026-10-31: AP `Rustyfarian-C4F4` with portal name `rgb-clock`, pairing ring, BOOT short press keeps the record, 3 s hold erases it, provision/commit/reboot into the face; boot now logs `Showing fixed time 12:34:56`.
  - Observed: two or three hitches in the rainbow while Wi-Fi connects (radio bring-up shares the one executor); cosmetic, recheck when M3 lands.
- [ ] M3 — MQTT tick
  - Implemented 2026-11-05 through the shared `clock-app` (ADR-006, `one-clock-app-two-tiers-v1.md`); awaits the C3 hardware run.
- [ ] M4 — C6 arm
- [ ] M5 — OTA
- [ ] Documentation updated (README, AGENTS.md, CHANGELOG)

## Session Log

- 2026-10-27 — Feature doc created via /feature dialog
- 2026-10-27 — M1 implemented: hal crate, `*-hal` recipes, `clock-pure::render_time`; `just verify`, `verify-hal`, `build-hal`, `deny` green; not yet flashed
- 2026-10-27 — M1 hardware-validated on the C3; `*-hal` recipes take a `chip` argument (default `c3`, `just flash-hal c3`)
- 2026-10-27 — M2 implemented (SoftAP portal, STA, store at `0x9000`, 24 KiB stack floor, 7 outbox requests)
  - `just verify`, `verify-hal`, `build-hal`, `deny` green
  - Not yet flashed
  - Broker prefill must be an IPv4 literal
- 2026-10-28 — M2 committed (`39c86a4`); adopted the upstream re-entry hook: pin `bae2dc4`, direct `juggler` pin, BOOT short press → portal, 3 s → erase; awaiting hardware test
- 2026-10-30 — M2 hardware test on the C3 (`just flash-hal c3`, log via the new `just monitor-log`)
  - Provisioning to STA join passed; DHCP took over 10 s once, retried fine
  - Ring garbled; control images pin it on the async RMT refill (blocking + critical section is clean); fix needs its own plan
  - BOOT re-entry not caught (~250 ms sample window); erase untested
  - M3 not started
- 2026-10-30 — Filed bugs 002 (ring), 003 (BOOT window), 004 (cut log line) and an upstream ws2812 request; next session starts with bug 002
- 2026-10-30 — Bug 003 fixed: 2 s BOOT window; short press, STA rejoin, 3 s erase and re-provisioning verified on the C3; next is the full M2 rerun
- 2026-10-30 — M2 closed on the bug 003 hardware log (portal, commit, STA, BOOT short press, 3 s erase); next is M3
- 2026-10-30 — Pin moved from `bae2dc4` to `01ff1b9`; the second `juggler` git pin is dropped; `check-hal`, `clippy-hal`, `build-hal`, `verify` and `build` pass
- 2026-10-31 — Pin move hardware-validated on the C3 (portal, short press, 3 s erase, commit/reboot); boot logs the fixed time; README states the static face
- 2026-11-05 — M3 implemented via the shared clock app; pin moved to `a53c434` (temporary local source); `verify-hal`, `build-hal` green; not yet flashed
