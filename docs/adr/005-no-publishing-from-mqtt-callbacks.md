# ADR-005: Never Publish from an MQTT Callback

## Status

Accepted — 2026-09-28.

## Context

Rejecting a malformed `ota/command` used to publish a `failed` status directly from the `on_message` callback via `MqttHandle::try_publish`.

`try_publish` is non-blocking only against the Rust-side client mutex; the block happens inside `esp_mqtt_client_enqueue`, which takes the MQTT task's `MQTT_API_LOCK`.

ESP-IDF holds that lock across the whole event-loop iteration, including the synchronous dispatch of `on_message`, so the callback's enqueue waits for the very lock the MQTT task is holding while it waits for the callback to return.

The client deadlocks until a reset (bug 001, `docs/bugs/archive/001-rejected-ota-command-deadlocks-mqtt-2026-09-28.md`).

## Decision

Nothing in this firmware publishes, subscribes, or otherwise calls into the MQTT client from `on_message`, `on_connect`, or `on_disconnect`.

Callbacks hand work to a dedicated thread over a bounded channel using a non-blocking `try_send`; a full queue drops and counts the item.

OTA rejections flow through the `ota-reporter` thread, which publishes `failed { reason }` (`src/ota/mod.rs`).

The upstream `rustyfarian-esp-idf-network` 0.5.1 adds a `WrongThread` guard that turns any misuse into a fail-fast error (upstream ADR 017); this firmware adopts it once published (ROADMAP, Near term).

## Consequences

### Positive

- Callbacks return quickly, so the MQTT event loop never blocks on firmware code.
- Dropped rejections are counted and logged instead of being lost silently.

### Negative

- A permanent reporter thread (6 KiB stack) and a bounded rejection queue; rejection statuses are best-effort.
- Until the 0.5.1 dependency bump lands, the contract rests on discipline: a callback misuse on 0.5.0 still hangs the client until a reset.

## References

- `docs/bugs/archive/001-rejected-ota-command-deadlocks-mqtt-2026-09-28.md` — the bug and its mechanism.
- Upstream `rustyfarian-network` ADR 017 — the runtime `WrongThread` guard that backstops this contract (guard in `511b37f`, ADR in `4ac3d42`, part of 0.5.1).