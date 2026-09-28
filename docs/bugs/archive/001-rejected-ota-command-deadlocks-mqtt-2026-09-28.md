---
id: 001
title: A rejected ota/command deadlocks the MQTT client until reset
captured-on: 2026-09-28
doc-version: 2
status: closed
kind: defect
---

# Bug 001: A rejected ota/command deadlocks the MQTT client until reset

## Symptom
One malformed `ota/command` payload freezes the clock: no `failed` status is published, and no further `tick` or command is ever processed until the board is reset.

## Suspected Cause
`OtaSubmitter::reject` calls `publish_status` → `MqttHandle::try_publish` from inside the `on_message` callback.
`rustyfarian-esp-idf-network 0.5.0` runs `on_message` while the received event from `connection.next()` is still borrowed.
The ESP-IDF MQTT task waits for that event to be released while holding its API lock, so `enqueue` on the callback thread never returns.
The mutex `try_lock` in `try_publish` does not help: the block is inside `esp_mqtt_client_enqueue`, not on the Rust mutex.
The network crate already subscribes from a spawned thread (`spawn_subscriber_thread`), which suggests the same constraint.

## Linked Artefact
- `src/ota/mod.rs` — `OtaSubmitter::submit` and `OtaSubmitter::reject`
- `rustyfarian-esp-idf-network 0.5.0` — `src/mqtt/mod.rs`, builder event loop dispatching `EventPayload::Received`
- `docs/features/ota-mvp-v1.md` — hardware item 8 add-on check (malformed command)
- `docs/project-lore.md` — "OTA & Firmware Update"

## Reproduction Confidence
high

## Severity
high

A single bad message on the unauthenticated `ota/command` topic takes the clock offline with no automatic recovery.
An already-valid slot never rolls back, so nothing reboots the board.

## Environment
- ESP32-C3 (revision v0.4), firmware 0.3.0 on `ota_0`, branch `ota-application-layer`
- `rustyfarian-esp-idf-network 0.5.0`, ESP-IDF v5.3.3
- Found 2026-09-28 during the C3 OTA onboarding demo; the C6 shares the code path

## Expected Behaviour
`failed { "reason": "command_invalid" }` arrives on `ota/status` and ticks keep flowing, per the feature doc.

## Actual Behaviour
- Serial logs `OTA command rejected (command_invalid): trailing characters at line 1 column 87` (at 450534 ms), then nothing further from MQTT.
- No `failed` status reaches the broker, and no `OTA status publish dropped` warning is logged.
- The ring freezes on the last applied tick.
- A later `not-json` probe on `ota/command` never reaches the firmware.
- RST recovers the board; the broker does not redeliver the QoS 0 command.

## Reproduction Steps
1. Flash and provision the board, confirm ticks move the ring.
2. Publish a malformed payload, e.g. `mqttx pub -h $MQTT_HOST -p $MQTT_PORT -t ota/command -m 'not-json'`.
3. Watch `ota/status`: no `failed` message arrives.
4. Watch the ring: it stops following ticks.
5. Publish any further `ota/command`: the serial log shows nothing.

## Suggested Fix Area
- Never publish from `on_message`: route rejections to a thread that is not the MQTT event loop, e.g. a rejection message on the OTA worker's channel or a small dedicated reporter.
- The same `reject()` path serves `busy` and `worker_unavailable`, so a second command during an update likely hangs the client too — cover all three.
- Consider an upstream note to `rustyfarian-esp-idf-network`: `try_publish` is documented as non-blocking but blocks forever from inside a callback; a `WrongThread` guard like `publish_acked`'s would turn the hang into an error.

## Owner

## Links
- `docs/features/ota-mvp-v1.md` — hardware sequence item 8 add-on check marked failed
- `docs/project-lore.md` — entry "`try_publish` from inside an `on_message` callback deadlocks the MQTT client for good"

## Session Log
- 2026-09-28 — Observed on the ESP32-C3 during the OTA onboarding demo; reset recovered the board
- 2026-09-28 — Captured directly as a defect via /bug (full triage known from the live session)
- 2026-09-28 — Root cause confirmed in sources: `MQTT_API_LOCK` held by `esp_mqtt_task` across the `Channel::share` handoff (`mqtt_client.c:1580`, `:1040`, `:2207`)
- 2026-09-28 — Fixed: `reject()` `try_send`s into a bounded queue (depth 4) drained by a new `ota-reporter` thread; nothing publishes from `on_message`
- 2026-09-28 — Verified on the ESP32-C3 (fix delivered over OTA as 0.3.5): `command_invalid` published, 15-message burst gave 11 published + 4 counted drops and a live probe, `busy` during a 0.3.6 update published while the update completed
- 2026-09-28 — Upstream request filed in `docs/outbox/rustyfarian-network-try-publish-from-callback.md`; closed via /bug
