# Feature: OTA MVP Demo — MQTT-triggered A/B Update with Rollback

*Status: Draft*

*Delivered so far on branch `ota-mvp`: the A/B partition layout, the rollback-capable IDF-built bootloader, the flash and validation tooling, and hardware sequence items 1 to 3. The OTA application flow — worker, sidecar fetch, health policy, rollback demo — is not implemented yet; items 4 to 7 are pending.*

## Goal

Deliver a working over-the-air firmware update demo on the ESP32-C6 in the next release, so the rustyfarian workspace has one repeatable, hardware-validated OTA path instead of a library with no caller.

## Context

The OTA library is finished upstream in `rustyfarian-network`: `juggler::ota` (version, verifier, metadata, state; ~36 host tests), and the ESP-IDF tier's `OtaSession::fetch_and_apply` → `EspOta` swap → `mark_valid`/`rollback`.
What has never existed anywhere is a consumer: no partition table with OTA slots, no rollback-enabled bootloader, no logic that decides an update is needed, and no flashable artefact.

Upstream ADR 011 §3 assigned the demo binary and `partitions.csv` to `rustyfarian-ferriswheel-demo`.
That assignment is amended here: the demo runs in this repository instead, because this repo is already the workspace's integration test fixture and is the only place where a real firmware consumes the Wi-Fi, MQTT, provisioning and LED stacks together.
The local ADR records the amendment.

Constraints that shape the design:

- ESP32-C6 DevKitC-1, 4 MiB flash, ESP-IDF `v5.3.3`, `std` firmware.
- Flashing builds the firmware first, then calls `espflash flash` with an explicit `--bootloader` (`scripts/flash.sh`), not `cargo espflash` or an ESP-IDF menuconfig build.
- Wi-Fi and MQTT credentials live in NVS via SoftAP provisioning, so the OTA image must preserve the existing NVS region.
- The clock must keep displaying time while an update downloads.
- `rustyfarian-esp-idf-network` is consumed from crates.io at `0.5.0` with the `ota` feature enabled.

### Prerequisite: the September 2026 dependency wave

OTA work required a coordinated dependency bump before it could start, because the `ota` feature is only reachable through a `rustyfarian-esp-idf-network` revision that has moved to the September stack.
That wave has landed: `rustyfarian-esp-idf-network` has since moved from a git pin to crates.io `0.5.0` with the `ota` feature enabled, and `decide_update` ships as part of it, so no interim `Version` comparison is needed here.

| Component                                   |                        From | To                                |
|:--------------------------------------------|----------------------------:|:----------------------------------|
| `rustyfarian-esp-idf-ws2812`, `ferriswheel` |                       0.6.0 | 0.7.0                             |
| `pennant` (transitive)                      |                         0.6 | 0.7                               |
| `esp-idf-svc`                               |                        0.52 | 0.53                              |
| `esp-idf-hal`                               |                        0.46 | 0.47                              |
| `rustyfarian-esp-idf-network`               |               git `8fc9f5f` | crates.io `0.5.0` (`ota` feature) |
| Rust toolchain                              | `nightly-2025-12-01` (1.93) | `nightly-2026-01-26` (1.95)       |

The wave is not separable into smaller steps.
`rustyfarian-esp-idf-ws2812 0.6` implements `pennant 0.6`'s `StatusLed`, which post-wave network no longer accepts; `0.7` implements `pennant 0.7` but declares `rust_version = "1.95"`, which the current `nightly-2025-12-01` (rustc 1.93.0-nightly) cannot satisfy.
Every pin moves together or none does.

Network's own changelog records the wave as compile-verified only; validating it on hardware is this repository's job.
**Done on 2026-09-27**: baseline `b58d2b4` ran on an ESP32-C6-DevKitC-1 with the crates.io `0.7.0` ws2812 wave, `esp-idf-svc 0.53`, `rustyfarian-esp-idf-network 0.5.0` and `nightly-2026-01-26`, covering SoftAP provisioning, STA join, DHCP, MQTT subscribe and `tick` rendering.
One regression surfaced: the main task overflows its 8000-byte stack by 40 bytes inside network's `wait_committed` right after the portal commits, because it clones `Option<ProvisioningConfig>` on the caller's stack.
The reboot that follows lands on the provisioned path, so the clock still comes up; see `docs/project-lore.md` "MQTT & Networking" and `docs/outbox/rustyfarian-network-wait-committed-stack-clone.md`.
Fixed locally the same day: `CONFIG_ESP_MAIN_TASK_STACK_SIZE` is now 16384 in `sdkconfig.defaults`, and a clean rebuild plus a full erase, re-flash and re-provisioning showed the commit completing with the firmware's own "Provisioning committed — restarting into normal boot" line and no fault.
The upstream clone is still worth removing; the outbox request stays open.

The wave also carries two edits unrelated to version numbers.
`PortalConfig` gained two fields upstream, so the struct literal in `run_provisioning` must add `ssid_override: None` and `defaults: PortalDefaults::default()` to keep today's behaviour — a source-breaking change for any code constructing it by name.
And the nightly is pinned in two places: `rust-toolchain.toml` and three hardcoded lines in `.github/workflows/rust.yml`.
The workflow is changed to read the channel out of `rust-toolchain.toml` so there is exactly one pin site and CI cannot silently compile on a different compiler than a developer's machine.

## Decisions

- **The wave lands as its own commit, hardware-validated before any OTA code.**
  A failure then implicates the dependency bump or the OTA feature, never both.
  Both ship in the same release.

- **OTA slots are 1.75 MiB (`0x1C0000`) each, deviating from ADR 011's 1 MiB.**
  ADR 011 sized slots for a minimal ESP32-C3 MQTT example.
  This image carries Wi-Fi, MQTT, a captive-portal HTTP server, WS2812, `ferriswheel` and OTA, which measured 1,452,669 bytes (1.39 MiB) once the wave built.
  ADR 011's 1 MiB slots would overflow it, and 1.5 MiB would leave only 117 KiB of headroom for the OTA worker, the sidecar HTTP client and SHA-256 still to be added, so 1.75 MiB was chosen against that measured figure rather than an estimate.

- **`phy_init` is kept, also deviating from ADR 011.**
  The existing table has it; removing it is unrelated risk inside a release that already moves the entire dependency stack.

- **The NVS region keeps its exact offset and size (`0x9000`, `0x6000`).**
  The layout change alone therefore cannot invalidate provisioned credentials, which keeps later partial reflashes safe.
  The one-time first flash still erases the whole chip for the reason given below, so credentials are re-entered once regardless.

- **`scripts/flash.sh` must pass `--bootloader` and `--ignore-app-descriptor`, or rollback cannot work at all.**
  espflash 4.x bundles an ESP-IDF v5.5.1 bootloader and writes it whenever `--bootloader` is absent, which is the case today.
  That bootloader is not built from this project's `sdkconfig.defaults`, so `CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE=y` never reaches the device while `mark_valid()` still returns success — a silent failure with healthy-looking logs.
  It also uses a 32 KB MMU page size against the v5.3.3 app's 64 KB.
  The fix is already implemented in `rustyfarian-network`'s own `flash.sh` and is documented in its lore; this repository never adopted it.
  Point `--bootloader` at the `esp-idf-sys`-built `bootloader.bin` under `target/<target>/release/build/esp-idf-sys-*/out/build/bootloader/`.

- **Update-decision logic lives upstream in `juggler::ota`, not here.**
  It is the only way the bare-metal tier inherits it later.
  The dependency wave and the crates.io move are both done, and `decide_update(running, offered) -> UpdateDecision` ships in network `0.5.0`, so this firmware calls the `Apply`/`Skip`/`Reject` policy directly — no interim `Version` comparison is written that would later be thrown away.

- **The MQTT command carries an optional `sig` field from day one, accepted and ignored.**
  Signed manifests then become a non-breaking addition rather than a schema change.

- **The sidecar fetch is implemented locally, not requested upstream.**
  Upstream's `FirmwareDownloader` is private and its `ota` module exposes no small-body GET.
  `EspHttpConnection` has inherent `status()`, `header()` and `initiate_request()` plus `impl Read`, reachable through `esp_idf_svc::io`, so this needs roughly fifty lines and no new dependency.
  Blocking the release on an upstream API for this would be disproportionate.

- **The MQTT callback parses and enqueues only; the download runs on a dedicated thread.**
  A multi-minute blocking download inside the event-loop callback would deadlock the client — upstream's `publish_acked` has a `WrongThread` error variant for exactly this mistake.

- **The bootloader rollback demo uses a valid-but-unhealthy image, not a truncated one.**
  See below; this corrects the assumption inherited from the upstream analysis.

<details>
<summary><strong>Partition layout</strong></summary>

```
# Name,   Type, SubType, Offset,   Size,     Flags
nvs,      data, nvs,     0x9000,   0x6000,
otadata,  data, ota,     0xf000,   0x2000,
phy_init, data, phy,     0x11000,  0x1000,
ota_0,    app,  ota_0,   0x20000,  0x1c0000,
ota_1,    app,  ota_1,   0x1e0000, 0x1c0000,
```

Slot size is `0x1C0000` (1.75 MiB), chosen against a measured figure rather than an estimate.
The release image on 2026-09-26, before any OTA code, is **1,452,669 bytes (1.39 MiB)** — the sum of allocated `PROGBITS` sections.
That is an **estimate** of the downloadable image, not the artefact itself: the flashed `.bin` is assembled from the ELF and carries its own header and padding.
Treat the headroom figures below as pre-OTA-code and provisional.
When `just ota-artifact` lands it must validate the generated `.bin` against both slot sizes and hash those exact bytes, and the image must be remeasured then — `scripts/check-partitions.sh`'s `MIN_SLOT` constant is a historical floor that cannot catch future growth on its own.
ADR 011's 1 MiB slots would overflow it by 395 KiB, and 1.5 MiB would leave only 117 KiB of headroom for the OTA worker, the sidecar HTTP client and SHA-256 still to be added.
1.75 MiB leaves roughly 330 KiB of headroom after that code lands and still keeps 384 KiB of flash free at the tail for the signed-manifest partition `ota-hardened` will want.

**App partition offsets must be 64 KiB aligned**, not 4 KiB — `gen_esp32part.py` rejects anything else, and the IDF-built v5.3.3 bootloader uses 64 KiB MMU pages.
This is why `ota_0` starts at `0x20000` rather than immediately after `phy_init`, and the intervening 56 KiB is alignment padding.
Do not "reclaim" it by moving the app partitions down to a 4 KiB boundary; the table will not build.

`sdkconfig.defaults` gains `CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE=y`.
`CONFIG_PARTITION_TABLE_CUSTOM` is irrelevant because espflash supplies the table.
Two separate steps are then both required before rollback is armed: `just clean-idf`, so a bootloader is rebuilt with the new setting, and the `--bootloader` flag above, so that rebuilt bootloader actually reaches the device.
Either one alone leaves rollback silently disabled.

First flash with the new table requires a full chip erase, because the region that becomes `otadata` previously held `phy_init` and application bytes, so its contents would be garbage that the bootloader may interpret as slot state.
The erase wipes NVS, so the device must be re-provisioned once.
This is a one-time cost paid at the layout change, not on subsequent OTA updates.

</details>

<details>
<summary><strong>Rollback mechanics</strong></summary>

`OtaSession::fetch_and_apply` compares the streamed SHA-256 against the expected digest *before* calling `ota_writer.complete()`, which is what sets the boot partition.
A truncated or corrupted image therefore returns `ChecksumMismatch`, aborts the write, and leaves the boot slot unchanged.
The device never boots the bad image and the bootloader never rolls anything back.

Bootloader rollback fires only when a well-formed, correctly-hashed image *is* activated and then fails to call `esp_ota_mark_app_valid_cancel_rollback()` before the next reboot: the slot moves `PENDING_VERIFY` → `ABORTED` and the bootloader boots the other slot.
`INVALID` is what an explicit `esp_ota_mark_app_invalid_rollback_and_reboot()` sets, so the two paths are distinguishable when inspecting slot state during the demo.
Demonstrating rollback therefore requires an image that passes verification and then deliberately fails its health check.

Withholding `mark_valid` does **not** by itself reboot the device, so the unhealthy image needs an explicit bounded failure policy: it must reset itself (a deliberate `restart()` after a fixed dwell, or a watchdog it stops feeding) or the slot sits in `PENDING_VERIFY` indefinitely and nothing rolls back.

Three distinct scenarios result, all worth showing:

| Scenario | Mechanism | Proves |
|:---------|:----------|:-------|
| Truncated image | `ChecksumMismatch`, abort, boot slot untouched | Verify-before-swap |
| Unhealthy v2 | Valid image, feature-gated to skip `mark_valid` and reboot after 15 s | Bootloader A/B rollback |
| Operator rollback | `OtaSession::rollback()` driven by an MQTT command | Manual recovery |

The unhealthy image is produced by a Cargo feature that is never enabled in a release build.

</details>

<details>
<summary><strong>Firmware structure and MQTT contract</strong></summary>

```
src/ota/mod.rs      OtaWorker: sync_channel(1) + dedicated thread, 16 KiB stack
src/ota/sidecar.rs  sidecar GET via EspHttpConnection inherent API
src/ota/policy.rs   mark_valid gate: IPv4 lease acquired and 30 s since boot
```

Topics stay flat, matching the existing `"tick"` convention: `ota/command` inbound, `ota/status` outbound.

The command payload is deliberately minimal — `{ "manifest_url": String, "sig": Option<String> }`.
Everything authoritative lives in the fetched manifest (`version`, `sha256`, and the firmware `url`), so the reserved `sig` later covers the firmware identity *and* its location in one signature rather than leaving the URL outside the signed envelope.
The MVP parses `sig` and ignores it; the parse site carries a comment, not just a doc comment, so a future implementer cannot miss that the trust boundary moves when it is honoured.

Both the command and the manifest are bounded before parsing: the MQTT payload is rejected above a fixed byte cap, the sidecar body is read into a fixed buffer and rejected if it fills before EOF, and the sidecar fetch carries its own connect and read timeout rather than inheriting the firmware download's.
An unbounded payload on a public topic is a trivial memory-exhaustion path on a device with a few hundred KiB of heap.

The manifest also carries a **target** field naming the chip it was built for, and the firmware refuses an image whose target is not its own.
This repository builds the same source for both ESP32-C6 and ESP32-C3, and the two images are not interchangeable — without this field a single broadcast on `ota/command` could push a C6 image onto a C3.
The serial flash path guards against that with an `espflash board-info` chip check, because `--ignore-app-descriptor` disables espflash's own; the OTA path has no equivalent unless the manifest carries the target.

Status messages are a tagged enum: `downloading`, `verifying`, `writing`, `swap_pending`, `applied { version }`, `failed { reason }`, `rolled_back`.
Progress states publish best-effort and may be dropped.
**Only `rolled_back` uses `publish_acked`.**
Note precisely what that buys: `publish_acked` waits for the broker's PUBACK, which is *not* persistence.
Surviving a reboot is the firmware's job — the rollback evidence must be written to NVS before the publish is attempted, retried at startup while the flag is set, and cleared only once `publish_acked` returns `Ok`.
Without that, an unreachable broker at exactly the wrong moment loses the one record that a rollback happened.

A single-slot channel is **not** sufficient to reject concurrent commands: once the worker receives the first one the slot is free again, so a second command queues behind an in-flight update.
The worker therefore owns an explicit busy flag covering both queued and executing work, and the callback uses a non-blocking `try_send` — rejecting with a logged `failed` status when busy, and never blocking the MQTT event loop.
`src/main.rs:147` currently binds `let _mqtt = …`; the handle must be retained and shared with the worker, since `publish_acked` returns `WrongThread` if called from the event-loop thread.

The worker thread starts at a 16 KiB stack — the MQTT event loop uses 12 KiB for a far lighter callback, and this path adds the HTTP client, the flash write and SHA-256 frames.
That number is a starting estimate to be confirmed on hardware, not a derived one.

The `mark_valid` policy runs on its own lightweight thread, not inline at `src/main.rs:122`.
That call deliberately continues past a timeout so a slow DHCP lease cannot leave the ring dark, and bolting a 30 s dwell onto it would undo that.
The dwell is measured from an `Instant` captured at `run_clock` entry, since ADR 011 §4 specifies 30 s **since boot**, not since association.
Calling `mark_valid` on a slot that is not pending verification is harmless.

**Health means more than a DHCP lease here.** ADR 011 §4's "IPv4 plus 30 s" is the workspace-wide floor, and on its own it would accept a build whose MQTT subscription or clock rendering is broken — precisely the regressions an OTA push is most likely to introduce.
This firmware's criterion is therefore all of:

- the clock driver initialised and the display wrote at least one frame,
- the MQTT client connected **and** at least one `tick` message received and applied, proving the subscription path end to end,
- an IPv4 lease acquired, and
- 30 s elapsed since boot.

**Failure is on a deadline, not left to a watchdog.**
Never calling `mark_valid` is not a policy: the slot would sit in `PENDING_VERIFY` forever and the clock would keep running a firmware whose MQTT path is broken, since nothing reboots it.
The health thread therefore carries an explicit deadline of **120 s since boot** — four times the 30 s success dwell, which absorbs a slow DHCP lease plus one Wi-Fi reconnect and one MQTT reconnect.

At the deadline, if the running slot is in `PENDING_VERIFY` and the criteria are still unmet, the firmware calls `OtaSession::rollback()`, which marks the slot `INVALID` and reboots into the previous image.
That is deliberate and immediate rather than dependent on a watchdog firing, and it is distinguishable in the logs from the automatic `ABORTED` path.

Two guards on that behaviour:

- It applies **only** to a slot in `PENDING_VERIFY`. A normally-booted, already-valid image must never reboot itself because MQTT happens to be down — that would turn a broker outage into a reboot loop on a working clock.
- The `rolled_back` record is written to NVS before the reboot, so the evidence survives to be published on the next boot (see the persistence note above).

The unhealthy-image demo build simply shortens this path, resetting deliberately so the rollback is visible within the demo rather than after two minutes.

</details>

<details>
<summary><strong>Tooling and documentation deliverables</strong></summary>

New `just` recipes, since every build and flash operation in this project goes through `just` and direct `espflash` invocation is blocked by a repository hook:

- `ota-artifact` — stage the release binary, compute its SHA-256, and write the sidecar JSON
- `ota-serve` — serve the staged artefacts over plain HTTP from `tmp/` for the demo
- `flash-baseline` — full erase plus first flash with the OTA table, for the one-time layout change
- `board-info` — print the attached chip type; `scripts/flash-image.sh` runs the same query itself and refuses a chip that differs from the requested target, since `--ignore-app-descriptor` disables espflash's own chip-model check
- `image-check` — generate the exact app image with `espflash save-image` and fail if it does not fit both slots; `preflight.sh` runs it (after `check-partitions.sh`) before every flash and CI runs it for both chips on every build, so the `MIN_SLOT` constant in `check-partitions.sh` is only a historical floor. The image is kept at `tmp/<target>-app.bin`, overwritten per run, as the artefact `ota-artifact` will later hash
- `flash-image-tests` — hardware-free regression suite for the flash script's chip-mismatch and dry-run guards, driven by a fake `espflash`; `DRY_RUN=1` on `flash` or `flash-baseline` prints the device-touching commands without running them
- `build-unhealthy` — build the feature-gated image used by the rollback scenario

Documentation:

- A local ADR recording the ADR 011 §3 amendment (demo hosted here) together with the 1.75 MiB slot and `phy_init` deviations
- `docs/ota-security-model.md` — threat model, why SHA-256 over plain HTTP is the MVP's limit, rollback policy, and the reserved `sig` field
- ~~An outbox request to `rustyfarian-network`~~ — **delivered in network `0.5.0`**, so no request is outstanding.
  The decision API is one `core`-only function in a new `juggler::ota::decision` module: `decide_update(running: Version, offered: Version) -> UpdateDecision`, where `UpdateDecision` is `Apply` / `Skip` / `Reject` for strictly-newer, equal and older respectively.
  No new error variant is needed, because both inputs are already-parsed `Version`s and `Version::parse` already reports malformed input.
  `OtaState` deliberately gains no `Display` or serde implementation upstream; the status-string mapping stays a small match in this firmware so the library stays policy-free
- A near-term roadmap entry, a changelog entry, and the version bump to 0.3.0

</details>

## Open Questions

- [x] What is the measured release image size, and does it confirm 1.5 MiB slots? Blocked until the dependency wave builds.
      Answer: 1,452,669 bytes (1.39 MiB) as of 2026-09-26 — 1.5 MiB slots would leave only 117 KiB of headroom, so slots were sized to 1.75 MiB (`0x1C0000`) instead.
- [ ] Does the retained MQTT handle support sharing with the worker thread as-is, or does it need an `Arc`?
- [ ] Should the LED ring show OTA progress, or keep displaying time unchanged throughout? A product question, not an architectural one.

Resolved during design: the status state machine does **not** move into `clock-pure`.
That crate is scoped to time-to-LED-index mapping and colour utilities, `just test` runs only `-p clock-pure`, and nothing in `src/` is host-tested today — even `LocalTime`'s JSON parsing has no coverage.
Introducing a host-test seam for the firmware binary is a build-pipeline change that does not belong in this feature.

## Validation

- [ ] Host tests (`just test`) — thin here by design: decision-logic coverage lives upstream in `juggler`, and this tier only gains host tests if the status state machine moves into `clock-pure`
- [ ] Wokwi simulation (`just act-ci`) — Wokwi does not model OTA partition swaps or bootloader rollback, so it verifies nothing here beyond continued boot
- [x] Hardware-in-the-loop — physical device required; this is where the feature is actually proven
- [ ] Docs / config only

Hardware sequence on the ESP32-C6:

1. [x] Flash the dependency wave alone; confirm the clock keeps time and provisioning still works. This is also the workspace's first hardware validation of the September stack.
   Done 2026-09-27 together with item 2, since the wave and the OTA layout ship in one baseline commit: the clock tracks `tick`, and provisioning persists and reboots into a working clock. The `wait_committed` stack overflow found on the first run was fixed by the 16384-byte main stack and re-verified the same day with a second erase, flash and provisioning cycle.
2. [x] Full erase, flash the new partition table, re-provision once, confirm boot from `ota_0`.
   Done 2026-09-27 via `just flash-baseline` on `/dev/cu.usbmodem2101` after `just board-info` confirmed an `esp32c6`; the boot log shows the IDF-built v5.3.3 bootloader, the five-row table with no `factory` entry, and `Loaded app from partition at offset 0x20000`. Image size 1,453,152 bytes, 79.19% of the slot.
3. [x] Power-cycle twice; confirm both boots come from `ota_0` with no rollback and no boot loop. Expect **no** `mark_valid` at this stage: the baseline carries no OTA application code, and a serial-flashed slot is not in `PENDING_VERIFY` anyway because the erase left `otadata` blank. Its absence is not evidence that rollback is broken.
   Done 2026-09-27: two USB power cycles, each followed by a `just monitor` connect reset to capture the bootloader log, since the USB-JTAG console cannot record a cold boot. Every boot loaded `ota_0` at `0x20000`, reached MQTT and cancelled the rainbow on the first `tick`; no rollback, `PENDING_VERIFY` or `ota_1` lines appeared. Logs are in `tmp/boot-log-*.txt`, not committed.
4. Publish a good update; confirm download, verification, swap, reboot into `ota_1`, and `mark_valid`.
5. Publish a truncated image; confirm `ChecksumMismatch` and that the clock keeps ticking on the unchanged slot.
6. Publish the unhealthy image; confirm the bootloader rolls back to the previous slot.
7. Send the operator rollback command; confirm `OtaSession::rollback()` reboots into the previous slot.

## Out of Scope

- TLS and HTTPS. Integrity rests on SHA-256 over plain HTTP; a network attacker controlling both image and sidecar defeats it. Documented in the OTA security model, not fixed here.
- Ed25519 signing and signed manifests. The `sig` field reserves the schema slot; nothing verifies it.
- Bare-metal OTA. The `esp-hal` tier has no MQTT at all, per upstream ADR 015.
- A factory-reset partition. Recovery remains serial flashing with `otadata` erased.
- Decision logic implemented in this repository.
- Delta or compressed updates.
