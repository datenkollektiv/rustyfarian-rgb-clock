---
gate: hardware
gate-note: Tick-driven hands on the C3 with both the IDF and the esp-hal image, OTA still passing on IDF
desk-work: available
---

# Feature: One Clock App, Two Tiers v1

One async clock app, generic over the upstream parity traits, drives the hands on both firmwares ([ADR-006](../adr/006-one-clock-app-on-both-tiers.md)).
Delivers esp-hal clock M3 by construction.

## Decisions

|                                      Decision | Reason                                                           | Rejected Alternative              |
|----------------------------------------------:|:-----------------------------------------------------------------|:----------------------------------|
|     Both tiers git-pin network main `a53c434` | Find API gaps on both tiers before the upstream release          | esp-hal only; IDF stays on 0.6.0  |
| App generic over `NetworkLink` + `MqttClient` | Upstream ADR 022 surface; mirrors upstream `lwt_app.rs`          | Per-tier tick code                |
|                     App in `crates/clock-app` | Host-tested against juggler mocks; one manifest                  | `#[path]` include like upstream   |
|                  Tick parsing in `clock-pure` | Flat logic, lowest rung (upstream ADR 023); strict ranges        | serde in the app                  |
|     IDF OTA intake via `Hooks::other_message` | IDF `receive()` and `on_message` are exclusive; no callback left | IDF wrapper filtering `receive()` |
|           IDF Wi-Fi init becomes non-blocking | The app owns the ring; retries replace the 30 s reboot loop      | Keep the blocking LED path        |

## Constraints

- `just verify`, `just build`, `just verify-hal` and `just build-hal` stay green.
- IDF OTA, provisioning and the `MQTT_TOPIC` override keep their behaviour.
- `clock-app` compiles under `no_std`: `core`, `heapless`, `embassy-time`, `embassy-futures`.
- Nothing publishes or subscribes from an IDF MQTT callback or from the receive loop (ADR-005).
- One `juggler` source in the graph; a second source splits the traits (like the `pennant` trap).
- The `a53c434` pin is a temporary local `file://` git source; repin to GitHub before committing.

## Open Questions

- [x] IDF `receive()` next to `on_message`? No: a message reaches one or the other, so `on_message` goes.
- [x] App location? A `no_std` crate, `crates/clock-app`.
- [ ] IDF heap with the app thread and the OTA worker: gate is min-free ≥ 40 KB on the C3 after one outage.

## Gaps and Discrepancies (upstream `a53c434`)

- IDF: `MqttBuilder::on_message` disables `receive()`, undocumented; forces OTA routing through the app.
- Both: `NetworkLink`, `MqttClient`, `MqttMessage` not re-exported; consumers need a direct `juggler` pin.
- IDF: inherent `WiFiManager::is_connected`/`wait_connected` shadow the trait methods with other signatures.
- IDF: `OtaHandle::run_health_policy` blocks its thread and `WiFiManager` is not `Sync`.
- IDF: `NetworkLink` has no IP query; health uses a `subscribe_ready` flag instead.
- IDF: `InboundMessage::retained()`/`qos()` are not reported; the app must not branch on them.
- hal: `MqttConfig<'static>` forces copying provisioned strings into static cells.
- hal: broker must be an IPv4 literal; a hostname provisions fine, then MQTT never starts.
- hal: subscriptions at build time only, 64-byte topics, 256-byte payloads, inbound queue of 4.
- hal: no OTA runtime (blocks M5); factory reset erases nothing.
- Both: no shared provisioning boot outcome, no Wi-Fi failure signal, no status-LED hook, no graceful shutdown.
- Both: startup message shapes differ (fixed IDF topic vs hal arguments).
- Breaking on IDF: `QoS` path, `build(&wifi)`, `PortalConfig::device_name` is `DeviceName`, stricter `is_connected`.
- IDF: an LED passed to `WiFiManager` forces blocking init, so non-blocking init loses the GPIO8 status LED.
- IDF: `receive()` assembles up to 16 KiB per message, 8 deep, with no consumer limit; a flood can pin ~128 KiB.
- Both: `NetworkLink` has no `wait_disconnected`; the app polls `is_connected()` every second.
- Both: `juggler/mock` always pulls `lora` and `espnow`; host tests build them.
- Both: upstream `a53c434` still reports version 0.6.0; only the `?rev=` source tells it from crates.io.

## Behaviour Changes

- Ticks need exactly `hour`, `minute`, `second` in range (0..=23, 0..=59); out-of-range or extra keys keep the rainbow or last frame.
- IDF leaves the rainbow on the first valid tick, not on any message on the tick topic.
- IDF Wi-Fi retries forever instead of rebooting after 30 s; the onboard status LED stays dark.
- esp-hal shows live ticks instead of the fixed 12:34:56 face.

## State

- [x] Design approved
- [x] Tick parsing in `clock-pure` with host tests
- [x] Pins moved to `a53c434` on both tiers
- [x] `clock-app` crate with host tests
- [x] Shared app on IDF (tick via `receive`)
- [x] Shared app on esp-hal (M3)
- [ ] Hardware run on the C3, both images
- [ ] Documentation updated

## Session Log

- 2026-11-04 — Feature doc created via /feature dialog; ADR-006 accepted
- 2026-11-05 — Analysis workflow answered Q1 and Q2 and recorded the upstream gaps; plan approved
- 2026-11-05 — Steps 1–6 implemented; `verify`, `verify-hal`, IDF C3/C6, unhealthy and hal builds, `deny` green; not yet flashed
