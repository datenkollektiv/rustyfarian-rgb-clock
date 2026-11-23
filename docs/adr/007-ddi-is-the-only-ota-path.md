# ADR-007: DDI Is the Only OTA Path

## Status

Accepted — 2026-11-23.
Adopts `rustyfarian-network` ADR 027 (DDI transport scope) and the fleet DDI profile 0.4.0.
Amends item 7 of [ADR-006](006-one-clock-app-on-both-tiers.md): the tier hook no longer takes OTA commands.
[ADR-005](005-no-publishing-from-mqtt-callbacks.md) still governs the IDF callbacks.

## Context

The IDF firmware installs images on an MQTT command: a message on `ota/command` names a manifest URL, and the upstream MQTT OTA runtime downloads, verifies and swaps.
Whoever can publish on that topic and answer the manifest URL controls the image.

The fleet's DDI server replaces that flow.
The device polls the server with a per-device token, and the server offers a deployment.
Under the fleet profile no MQTT message carries install or rollback authority.

Upstream ships the DDI client on the ESP-IDF tier behind `ota-ddi`.
It makes `ota-ddi` next to the MQTT wire (`ota-wire`) a compile error, so one image has one path.
The client has no TLS yet: an `https://` base URL is refused, and `http://` needs the lab opt-in `ota-ddi-insecure-http`.

## Decision

1. The MQTT OTA command intake, its topics and its laptop recipes leave the repository.
   DDI is the only network path that installs an image.
2. The DDI client is compiled in by the firmware feature `ddi-lab`, which enables upstream `ota-ddi` and `ota-ddi-insecure-http`.
   The DDI base URL is compiled in; no runtime input turns the plain-HTTP opt-in on.
3. A `ddi-lab` image is for lab boards only and is never a release image.
4. The default build carries no OTA code until upstream's TLS round lands.
   It is updated over serial.
5. The device id is `rgb-clock-<mac4>` in every build.
   In a `ddi-lab` image it is also the DDI controller id, the MQTT username and the MQTT client id.
6. The token reaches the device through the provisioning portal and is passed only to the upstream transport.
   The firmware builds no DDI request itself.
7. The only OTA use of MQTT is the nudge topic `rustyfarian/ota/<device-id>/nudge`, which asks for an early poll and carries no authority.

## Consequences

### Positive

- One OTA path, and a publisher on the broker can no longer install or roll back an image.
- Every clock has its own identity and its own credential.
- The `MQTT_CLIENT_ID` prefill that leaked one chip's id into another chip's portal is gone.

### Negative

- A default build cannot update over the network until TLS fits the image slot.
- The hardware-validated MQTT OTA path and its drills are removed before the DDI path has its own bench record.
- Boards on MQTT OTA images move over by serial flash, erase and re-provisioning.
- The token sits in plaintext NVS on lab boards, a recorded lab-only risk.

## References

- `rustyfarian-network` ADR 027 and `docs/features/ddi-client-v1.md` — the upstream client
- [`docs/features/ddi-client-v1.md`](../features/ddi-client-v1.md) — scope, open questions, state
- `docs/features/archive/ota-mvp-v1.md` — the MQTT OTA contract this replaces
