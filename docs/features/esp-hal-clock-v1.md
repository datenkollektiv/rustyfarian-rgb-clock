---
gate: none
desk-work: available
---

# Feature: esp-hal Clock v1

A bare-metal `esp-hal` build of the clock, next to the unchanged ESP-IDF firmware, without OTA.
Missing upstream pieces become feature requests in `docs/outbox/`, or code contributions where appropriate.

## Milestones

- **M1** — hal build setup and a C3 ring that boots and shows the startup animation and a fixed time.
- **M2** — SoftAP provisioning and Wi-Fi STA via `rustyfarian-esp-hal-network`.
- **M3** — MQTT `tick` subscription drives the hands; no-OTA parity with the IDF clock.
- **M4** — C6 arm (GPIO 18) once a C6 board is available again.
- **M5** — OTA, blocked on an upstream esp-hal port of the OTA consumer runtime.

## Decisions

|                                          Decision | Reason                                                     | Rejected Alternative             |
|--------------------------------------------------:|:-----------------------------------------------------------|:---------------------------------|
|             New crate `crates/rustyfarian-esp-hal-rgb-clock` | Mirrors the knob esp-hal tier; IDF firmware stays at root | Feature flag in the root package |
|                 Not a `default-member` of the workspace | Bare `cargo` and IDF recipes stay on the espidf target     | `exclude` the crate              |
|                            ESP32-C3 first, DIN GPIO 4 | User decision: the C6 board was given away                 | C6 first; both chips at once     |
|      `rustyfarian-esp-hal-network` by git rev pin | esp-hal MQTT is only on unreleased main (`c944a55`)        | Local path patch; wait for 0.7   |
|      Broker is an IPv4 literal; DNS request filed upstream | esp-hal MQTT has no DNS yet (upstream ADR 019 §2)          | Contribute DNS before M3         |
|                          OTA left out of v1 | esp-hal tier has `EspHalOtaManager` only, no runtime        | Hand-roll an OTA runtime locally |
|       `clock-pure` becomes `no_std` outside tests | It uses nothing from `std` already                         | Copy the logic into the hal crate |
|        Follow the knob build pattern (ADR 006 there) | One config, `-Zbuild-std=core,alloc` per recipe, `target/hal` | Second workspace                 |

## Constraints

- The ESP-IDF build must not change behaviour: `just verify`, `just build` and `just flash` keep working.
- Exact pins of the September 2026 wave: `esp-hal =1.2.2`, `esp-rtos =0.4.0`, `rustyfarian-esp-hal-ws2812 0.7`.
- Target `riscv32imc-unknown-none-elf` on the `esp` toolchain (1.95), no `anyhow` in the hal crate; the IDF nightly pin stays.
- Every build, check and flash goes through `just` (`*-hal` recipes); temporary files go in `tmp/`.
- The C3 console is the UART bridge (`esp-println` `uart`), not USB-Serial-JTAG.
- The hal provisioning store lives in raw flash, not NVS, so switching tiers on one board needs re-provisioning.
- Returning a board from the hal image to the IDF A/B firmware needs a full `just flash`, since the hal image uses espflash's default partition table.

## Open Questions

- [x] Bootloader: reuse the IDF v5.3.3 bootloader from `just build idf_c3_rgb_clock`, as the knob does? Yes when built; otherwise espflash falls back to its bundled one (`scripts/flash-hal.sh`).
- [x] Does `esp-hal 1.2.2` build on `nightly-2026-01-26`, or does the hal crate need its own toolchain? No: `esp-sync` needs the stable `cfg_select` (Rust 1.95); the `*-hal` recipes use `cargo +esp` (1.95.0-nightly, April 2026), the IDF pin is untouched.
- [x] Partition table: can the hal image reuse `partitions.csv`, or does it need a hal-specific one? M1 uses espflash's default table (app at 0x10000); the A/B layout waits for M5.
- [ ] When does upstream plan an esp-hal port of the OTA consumer runtime (blocks M5)?

## State

- [x] Design approved
- [x] M1 — build setup and C3 ring (hardware-validated 2026-10-27: rainbow, then the clock hands)
  - C3 flash check expects, for `FIXED_TIME` 12:34:56: blue on LED 11 (12 o'clock), green on LED 5 (6 o'clock), red on LED 10 (11 o'clock), all three separate; LED 0 is the 1 o'clock position.
- [ ] M2 — provisioning and Wi-Fi
- [ ] M3 — MQTT tick
- [ ] M4 — C6 arm
- [ ] M5 — OTA
- [ ] Documentation updated (README, AGENTS.md, CHANGELOG)

## Session Log

- 2026-10-27 — Feature doc created via /feature dialog
- 2026-10-27 — M1 implemented: hal crate, `*-hal` recipes, `clock-pure::render_time`; `just verify`, `verify-hal`, `build-hal`, `deny` green; not yet flashed
- 2026-10-27 — M1 hardware-validated on the C3; `*-hal` recipes take a `chip` argument (default `c3`, `just flash-hal c3`)
