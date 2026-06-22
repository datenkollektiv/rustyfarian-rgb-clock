# Feature: Wi-Fi/MQTT SoftAP Provisioning v1

Replace the build-time, env-baked Wi-Fi and MQTT credentials with runtime
SoftAP captive-portal provisioning provided by `rustyfarian-esp-idf-network`
(`provisioning` feature, `SchemaProfile::WifiMqttDevice`). No credentials in the
firmware image.

## Decisions

|                                                                                       Decision | Reason                                                                                                                                                                                                                                                                      | Rejected Alternative                                                                         |
|-----------------------------------------------------------------------------------------------:|:----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|:---------------------------------------------------------------------------------------------|
|                      Use the network crate's `provisioning` feature (`WifiMqttDevice` profile) | Ships a complete SoftAP + captive portal + NVS store; nothing to hand-roll                                                                                                                                                                                                  | Hand-rolled SoftAP/DNS/HTTP portal; keeping `.env`/`env!()`                                  |
|                         Provision all five values (Wi-Fi SSID/pass + MQTT host/port/client-id) | The `WifiMqttDevice` profile bundles Wi-Fi + MQTT, so this fully removes baked-in credentials                                                                                                                                                                               | Wi-Fi only (leaves a partial `.env`/`build.rs` path with MQTT creds in the image)            |
|                                                              Provision-then-restart boot model | On commit, store + `esp_idf_svc::hal::reset::restart()` into normal STA boot; provisioning and STA each own `peripherals.modem` cleanly on separate boots. Matches the crate example.                                                                                       | Provision-then-continue in the same boot (modem teardown/re-init complexity)                 |
|                                                           Open provisioning AP (no PSK) for v1 | Simplest first-time UX; the setup window is brief and user-initiated                                                                                                                                                                                                        | WPA2 via `option_env!("PROVISION_AP_PSK")` (deferred)                                        |
|                           Amber breathing-pulse on the full 12-LED ring while the portal waits | Adopts the pulsing "pairing mode" convention, but in a color **outside** the clock's blue/green/red palette, so it's unmistakably "configure me" and never confused with the running face; `ferriswheel::PulseEffect` is already available (clock depends on `ferriswheel`) | Blue or green pulse (overlap the hour/minute hand colors); static indicator; serial-log only |
|                      Reuse the existing background-animation-thread pattern for the blue pulse | Mirrors `run_startup_animation` in `rgb_clock.rs`; cancel/handoff logic already proven                                                                                                                                                                                      | New ad-hoc threading                                                                         |
| Strip the credential vars from `build.rs` + `.env`; keep the `mcu` cfg flag + `embuild` output | Credentials leave the binary, but `build.rs` is still needed for the chip cfg flag and `embuild::espidf::sysenv::output()`                                                                                                                                                  | Delete `build.rs` entirely (would drop the `#[cfg(mcu=...)]` flag)                           |

## Reference implementation

`rustyfarian-network/crates/rustyfarian-esp-idf-network/examples/idf_c3_provision_mqtt.rs`
is the closest model (Wi-Fi + MQTT). Boot flow:

1. `ProvisioningStore::open(nvs.clone())` → `is_provisioned()?`.
2. **Provisioned:** `store.load()?` → build `WiFiConfig::new(ssid, pass)` and
   `MqttConfig` from the `StoredConfig` (the example's `mqtt_config_from_stored` /
   `derive_client_id` show the lifetime + client-id handling) → normal STA + MQTT boot
   (the clock's current `main.rs` path).
3. **Not provisioned:** start the amber breathing-pulse ring indicator, then
   `ProvisioningBuilder::new(PortalConfig { ssid_prefix, ap_password: None, channel,
   device_name, firmware_version, profile: WifiMqttDevice })
   .on_event(..).start(modem, sys_loop, nvs)` → `session.wait_committed(timeout)` →
   on commit `session.shutdown()?` then `restart()`.

Key API: `PortalConfig`, `ProvisioningBuilder`, `ProvisioningSession`,
`ProvisioningStore`, `StoredConfig`, `SchemaProfile::WifiMqttDevice` —
re-exported from `rustyfarian_esp_idf_network::provisioning`.

## Constraints

- Firmware-only change; `clock-pure` (`#![no_std]`) is unaffected.
- Enable the `provisioning` feature on `rustyfarian-esp-idf-network` (requires `wifi`,
  pulls `embedded-svc`); keep `wifi` + `mqtt`.
- `partitions.csv` must include an `nvs` partition large enough for the provisioning store.
- Provisioning and STA boot each consume `peripherals.modem` → separate boots only
  (provision → `restart()` → STA).
- `WiFiConfig`/`MqttConfig` borrow `&str`; the owned `StoredConfig` strings must outlive them.
- When `mqtt_client` is blank, derive the client-id from `device_name` truncated to the
  23-byte MQTT 3.1.1 cap (as in the example).
- Public repo: no secrets committed; `.env.example` drops the credential vars (or the file goes away).
- ws2812/network `pennant` family stays version-coupled (existing constraint).
- `just verify` and `just check` (C6 **and** C3) must stay green.

## Security stance & threat model

The open AP (no PSK) is a **conscious tradeoff** for local, physical, first-boot setup — not a
convenience default. The accepted boundaries:

- The provisioning portal is reachable **only while the device is unprovisioned**; after commit it
  restarts into STA mode and the AP does not return without an explicit reset.
- **Physical presence is assumed** for setup; the window is short and user-initiated (powering on an
  unprovisioned device).
- **No remote re-provisioning** — the portal is never exposed over the joined STA network.
- Accepted residual risk for v1: during the open window a nearby actor could join the AP and submit
  credentials first, or observe submitted values on the local AP link. Bounded by the short,
  unprovisioned-only window and the physical-presence assumption.
- Hardening path if the deployment context isn't local/physical: flip the AP to WPA2 via
  `PortalConfig.ap_password` (`option_env!("PROVISION_AP_PSK")`) — the flow already supports it;
  deferred for v1 UX.

**MQTT credential model:** the `WifiMqttDevice` schema already carries optional `mqtt_user` /
`mqtt_pass` (+ `mqtt_client`), so adding broker authentication later needs **no schema change** (the
example maps them via `with_auth` / `with_username_only`). v1 targets an anonymous broker, so auth is
left empty. **Out of scope for v1:** TLS / broker-identity validation — `MqttConfig` has no TLS
surface today and would need upstream support; flagged for the future.

## Re-provisioning & recovery

- **v1 non-goal:** a user-facing re-provisioning trigger. v1 provisions only when the NVS store is empty.
- **Supported recovery (v1):** erase the provisioning state via NVS/flash erase — `just erase-flash`,
  then power-cycle, re-opens the portal. This is the documented developer/field recovery path; it
  requires host tooling + a cable.
- **Planned (separate work):** a cable-free user trigger — hold **BOOT / GPIO9** at power-on to clear
  the store and re-open the portal (tracked under Open Questions).
- Why this can't be fully deferred: a mistyped Wi-Fi password, a broker change, an AP rename/disappearance,
  or moving networks all force a re-provision — so a recovery path is operationally **required** even
  while the *trigger UX* is deferred.

## Open Questions

### Prerequisites — all resolved before implementation

- [x] **NVS partition sizing.** Confirmed on hardware (2026-06-23): the existing
      `nvs` partition (24 KB) persisted and re-loaded the committed `wifi_mqtt` config across a reboot.
- [x] **Reset/recovery.** Resolved: `just erase-flash` clears the provisioning store and re-opens the
      portal on the next boot (verified in the 2026-06-23 hardware run). The cable-free BOOT/GPIO9
      trigger is explicitly deferred to a follow-up, not a v1 blocker — see [Follow-up](#follow-up-not-in-this-pr).
- [x] **AP auth final call.** Resolved: ship v1 with an open AP per the decision + threat model above.
      WPA2 via `PortalConfig.ap_password` (`option_env!("PROVISION_AP_PSK")`) is the documented hardening
      path if the deployment context isn't local/physical; deferred for v1 UX.

### Later decisions (non-blocking)

- [ ] Pulse tuning: amber shade (e.g. `(255, ~120, 0)`), min/max brightness, and period for the
      ring `PulseEffect` (and whether to also pulse the onboard LED via `pennant::PulseEffect`).
- [ ] AP naming: fixed `ssid_prefix` / `device_name` (e.g. "Rustyfarian" / "rgb-clock") vs configurable.
- [ ] Testing/Wokwi coverage depth: provisioning needs a SoftAP + a connecting client — can Wokwi/CI
      exercise it, or is it hardware-only? (rgb-clock is the integration-test fixture.)

## State

- [x] Design approved
- [x] Core implementation — `main.rs` (provisioning-aware boot + `run_clock`/`run_provisioning`
      + `derive_client_id`/`mqtt_config_from_stored`), `rgb_clock.rs` (amber `run_provisioning_animation`),
      `build.rs` (creds dropped), `Cargo.toml` (`provisioning` feature), `.env.example`, CI.
- [x] Tests passing — host `just verify` + C6/C3 `just check` green; code-reviewer pass applied
      (restart on every provisioning exit, client-id truncation, MQTT-target diagnostic log).
      No automated provisioning test yet — the portal path is hardware-validated (see below).
- [x] Documentation updated — feature doc, README setup section, `.env.example`, `build.rs` comment.

## Known merge debt — Wokwi integration signal degraded

An unprovisioned device now boots into provisioning mode, so the `wokwi/test-*.yaml` scenarios no longer
validate steady-state clock behavior — they exercise the provisioning-mode boot instead. CI stays green
because the Wokwi job is `continue-on-error` and only flags panics, so **this is a silent loss of an
integration signal**, not a visible failure. Until the scenarios are reworked to drive the portal (a
client POSTing the form), **hardware validation (below) is the authoritative integration signal for the
clock face.** Tracked as a follow-up (see [Follow-up](#follow-up-not-in-this-pr)).

## Verification / hardware test — PASSED (ESP32-C3, 2026-06-23)

Confirmed end-to-end on device: unprovisioned boot → amber pulse + SoftAP portal → form
submit → `Persisting provisioning config (profile=wifi_mqtt)` → `Committed` → pulse cancelled →
restart → `Provisioned (wifi_mqtt) — booting clock` → WiFi (WPA2) → IP → MQTT connect +
`subscribed to 'tick'` → ticks rendering (rainbow cancelled on the first tick). No credentials in
the image. (The portal page-load issue from an earlier attempt did not recur — form loaded and
committed cleanly.)

Repro: `just erase-flash` → `just flash` → connect to the open `Rustyfarian-XXXX` AP, submit the
portal form; the device restarts into the clock; power-cycle stays in clock mode; `just erase-flash`
returns it to provisioning.

Benign log noise observed (not firmware bugs): `OWE`/`i2c old driver` ESP-IDF warnings, and
`/favicon.ico` + `/generate_204` 404s and a `setsockopt: 22` error during portal **shutdown**
(late captive probes hitting handlers as the server tears down — network-crate teardown cosmetics).

## Pre-release consumption — git pin (flip back on release)

The network crate's `WifiMqttBoot` / `run_wifi_mqtt_portal` API (commit
`c0aabac`, branch `provision-or-load`) is not on crates.io yet, so we consume it
via a git pin — the same pre-release pattern used before the 0.4.0 release.
**When the network team publishes it (e.g. 0.5.0), revert all three:**

1. `Cargo.toml` — `rustyfarian-esp-idf-network = { git = "…", rev = "c0aabac…" }`
   → `{ version = "0.5.0", default-features = false, features = ["wifi", "mqtt", "provisioning"] }`.
2. `.cargo/config.toml` (local dev, gitignored) — move the `rustyfarian-esp-idf-network`
   patch from `[patch."https://github.com/datenkollektiv/rustyfarian-network"]` back under
   `[patch.crates-io]`.
3. `deny.toml` — remove `https://github.com/datenkollektiv/rustyfarian-network` from
   `[sources] allow-git` (back to `allow-git = []`).

CI builds fine in the meantime: `.cargo/config.toml.dist` carries no patches, so CI fetches
the crate straight from the pushed git rev.

## Follow-up (not in this PR)

- **Wokwi CI:** an unprovisioned device now boots into provisioning mode (amber pulse + SoftAP), so the
  `wokwi/test-*.yaml` scenarios exercise provisioning-mode boot, not the clock face. CI does not hard-fail
  (the Wokwi job is `continue-on-error` and only flags panics), but the scenarios are now semantically
  stale. Reworking them to drive the portal (a client POSTing the form) is the "later decision" above.
- BOOT/GPIO9 re-provision trigger; WPA2 AP; pulse tuning; AP-name configurability.
- **Boilerplate reduction:** ~105 of the ~150 firmware boot lines are generic glue copied from the
  network crate's own example. An upstream feature request has been filed to push it into the library
  (`provision_or_load` + `WifiMqttBoot`, firmware ~150 → ~50 lines). Firmware stays as-is until that lands.

## Session Log

- 2026-06-22 — Feature doc created via /feature dialog. Scope: Wi-Fi + MQTT (full `WifiMqttDevice`
  profile); open AP; **amber** breathing-pulse ring indicator (pairing-mode convention, kept off
  the clock's blue/green/red palette) via `ferriswheel::PulseEffect`; re-provision trigger deferred.
- 2026-06-22 — Review pass: added a "Security stance & threat model" section (open-AP tradeoff
  spelled out and MQTT credential/TLS model), a "Re-provisioning & recovery" section (v1 non-goal +
  `just erase-flash` recovery path + planned BOOT/GPIO9 trigger), and split Open Questions into
  "must answer before coding" (NVS sizing prerequisite, reset path, AP auth) vs "later decisions."
- 2026-06-22 — Implemented. Prerequisites cleared (NVS `0x6000`/24 KB is ample — no partition change;
  `just erase-flash` is the recovery path; open AP confirmed). `main.rs` boots provisioning-aware;
  amber pulse via `ferriswheel::PulseEffect`; creds dropped from `build.rs`. `code-reviewer` pass
  applied (restart on every provisioning exit; truncate supplied client-id; MQTT-target diag log).
  `just verify` + C6/C3 `just check` green. Wokwi-scenario rework deferred (see Follow-up).
- 2026-06-22 — Adopted the network team's `WifiMqttBoot` / `run_wifi_mqtt_portal` API (their impl of
  the outbox request, two-call split). Rewrote `main.rs` onto it (deleted `derive_client_id` +
  `mqtt_config_from_stored`; now `WifiMqttBoot::load` → `boot.wifi_config()`/`boot.mqtt_config()`,
  and `run_wifi_mqtt_portal` for the portal incl. `FactoryResetRequested` handling). Consumed
  **pre-release via git pin** (rev `c0aabac`) — see "Pre-release consumption" above for the flip-back.
  `just ci` (fmt + deny + check + clippy + test) and C3 `just check` all green.
- 2026-06-23 — **Hardware-validated end-to-end on ESP32-C3**: provision → commit → restart → STA →
  MQTT → fresh ticks rendering. NVS prerequisite confirmed. Earlier portal page-load issue did not recur.
  Only benign teardown/framework log noise observed (OWE/i2c warnings, shutdown-time 404s + setsockopt).
