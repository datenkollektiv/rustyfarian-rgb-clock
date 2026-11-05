# ADR-006: One Clock App on Both Tiers, Shared Logic on the Lowest Rung

## Status

Accepted — 2026-11-04.
Adopts `rustyfarian-network` ADR 022 (tier parity) and the three-rung ladder of `rustyfarian-network` ADR 023 and `rustyfarian-ws2812` ADR 012.
Applies `rustyfarian-network` ADR 021 (binary-owned providers) to both firmwares.
Supersedes nothing; [ADR-005](005-no-publishing-from-mqtt-callbacks.md) still governs the IDF callbacks.

## Context

The clock has two firmwares: the ESP-IDF one at the repo root and the bare-metal esp-hal one in `crates/rustyfarian-esp-hal-rgb-clock`.
Each sequences the same flow on its own: parse a `tick`, render the hands, show the ring.
The IDF copy lives in an `on_message` closure, and the hal copy does not exist yet (esp-hal clock M3).
Two copies of one flow drift, which is the failure mode the upstream ADRs address.

Upstream main (`a53c434`) now ships one async surface on both tiers.
`juggler::wifi::NetworkLink` and `juggler::mqtt::MqttClient` are implemented by the IDF `WiFiManager` and `MqttHandle` and by the esp-hal `WifiLink` and `MqttHandle`.
`examples-shared/lwt_app.rs` runs one application on both tiers to prove it.

The clock is the integration fixture of the workspace, so adopting the unreleased surface early is how upstream gaps are found before a release.

## Decision

1. The clock application is one async module, generic over `NetworkLink`, `MqttClient` and a narrow ring port, compiled into both firmwares.
   Only the setup code (peripherals, provisioning, Wi-Fi and MQTT builders, executor) differs per tier.
2. The IDF firmware runs the app with `block_on`, and the hal firmware runs it on its embassy executor; each firmware owns its executor (upstream ADR 021).
3. Shared logic takes the lowest rung that fits.
   - Flat logic (tick payload parsing and validation, time-to-pixels rendering) lives in `clock-pure`.
   - The app lives in `crates/clock-app`, a `no_std` crate host-tested against the `juggler` mocks; it only sequences: receive, call `clock-pure`, hand pixels to the ring port.
   - No rung-3 action machine is introduced unless a flow meets a trigger of upstream ADR 023 decision 2 and flat helpers still leave duplicated steps.
4. Review rule: a branch in a tier adapter that is not an error or event mapping is logic and moves into `clock-pure`.
5. A ring port is narrow: one method that writes a frame; the tier keeps the RMT driver, the critical section and the transmit mode.
6. Both firmwares pin `rustyfarian-esp-idf-network` and `rustyfarian-esp-hal-network` to the same upstream git revision until a release ships the async surface.
7. IDF OTA stays on the blocking runtime, since the esp-hal tier has no OTA runtime yet.
   Its intake moves from `on_message` into the app's receive loop: IDF delivers a message to `on_message` or to `receive()`, never both.
   The loop hands non-tick topics to a tier hook, which only enqueues (ADR-005).
8. Every API gap or tier discrepancy found while adopting is recorded in the feature doc and filed upstream; it is never patched over by a tier-specific branch in the app.

## Consequences

### Positive

- The tick path is written once, and the esp-hal clock gains MQTT (M3) by construction.
- Tick parsing becomes host-testable in `clock-pure`.
- Upstream gaps surface while the API is still unreleased.

### Negative

- Both firmwares depend on an unreleased git revision, which `cargo deny` and the release flow must accept until upstream publishes.
- The IDF firmware carries both the async surface (tick) and the blocking surface (OTA) during the transition.
- The IDF firmware gains an embassy-time driver and `block_on`, with the heap cost upstream measured at 15 to 25 KB (upstream bug 013).

## References

- `rustyfarian-network` ADR 021, ADR 022, ADR 023 — providers, tier parity, the ladder
- `rustyfarian-ws2812` ADR 012 — the ladder applied to pure crates
- `docs/features/one-clock-app-two-tiers-v1.md` — scope, gaps found, state
- `docs/features/esp-hal-clock-v1.md` — milestone M3
