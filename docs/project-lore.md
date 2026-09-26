# Project Lore

Non-obvious technical discoveries: facts that caused surprising failures, took significant time to debug, or would save a future developer 30+ minutes if known upfront.

Each entry: **bold fact** → root cause → fix.
To add an entry: read this file first to avoid duplicates, then append to the relevant `##` section (or add a new one).

---

## MQTT & Networking

**Calling `subscribe()` from the MQTT `on_connect` callback deadlocks on `esp-idf-svc 0.52+`.**
`subscribe()` blocks waiting for a SUBACK, but the MQTT event loop is frozen inside the callback and cannot process the SUBACK — a self-deadlock with no clear error message, just a hung application.
Affected versions: `esp-idf-svc 0.52+`.
Fix: register subscriptions with `MqttBuilder::subscribe()`; the network crate spawns a dedicated subscriber thread after `on_connect` returns and repeats that on reconnect.
Do not add firmware-local watcher threads unless a future network crate regression removes this behavior.

**The provisioning portal rejects `POST /save` ("N field error(s)") unless the OTA URL and device name are filled, and a fresh device prefills neither.**
`parse_form` requires `ota_url` (must start with `http://`) and `dev_name`, but the firmware sets no `PortalDefaults::ota_url` and the IDF tier's `Prefill::from_defaults` never falls back to `PortalConfig::device_name` (upstream gap at network `fcf536d`, see `docs/outbox/rustyfarian-network-portal-device-name-prefill.md`).
The broker URL prefills only when `MQTT_HOST` is set, and `.env` values are baked in at build time via `option_env!`, so a commented-out `.env` silently yields an empty form.
A rejected submit re-renders the form completely empty (`Prefill::empty()`), so every field must be retyped.
Fix: set `WIFI_SSID` and `MQTT_HOST` (plain hostname, no scheme or port) in `.env` and rebuild; the device name still needs the upstream fallback fix.
The OTA half is resolved upstream after `fcf536d` — `ota_url` is optional for `WifiMqttDevice` (ADR 014 amendment) and an empty value stores `""`, so the firmware's `http://ota.invalid/` placeholder is gone.
Deleting that placeholder while pinned at or before `fcf536d` reintroduces the rejection, and a local `[patch]` to the sibling tree hides it.

---

## Toolchain & Dependencies

**When `rustyfarian-esp-idf-ws2812` and the network crates both depend on `pennant`, they must resolve to the same compiled `pennant` package or `WiFiManager<L: StatusLed>` fails to compile.**
`WiFiManager::new<L: StatusLed>` requires the `Ws2812Rmt` type to implement the `StatusLed` trait from the *same* compiled `pennant` package.
If the two crates pull `pennant` from different sources (e.g. one from git `v0.5.0`, the other from crates.io `v0.6.0`), Cargo produces two separate packages and the trait bound fails with a confusing type-mismatch error that names `pennant::StatusLed` twice.
Fix: ensure both crates resolve to the same `pennant` source and version. When `rustyfarian-esp-idf-ws2812 v0.6.0` (crates.io) is in use, pin `rustyfarian-network` to a commit that also resolves `pennant` from crates.io `v0.6`.

**When ws2812 crates move from git to crates.io, the `[patch]` key in `.cargo/config.toml` must change from `[patch."<git-url>"]` to `[patch.crates-io]`.**
Cargo's `[patch]` mechanism uses the source URL as the section key.
If the key is wrong, Cargo silently uses the published crates.io version instead of the local sibling repo and emits "patch was not used in the crate graph" warnings — local dev patches have no effect without a clear error.

**`cargo deny` flags advisories on crates this firmware never compiles, because it scans the whole `Cargo.lock`, not the enabled-feature graph.**
`RUSTSEC-2023-0089` (`atomic-polyfill` unmaintained) fails `just deny` even though it sits behind the network crate's `lora` feature (`atomic-polyfill → heapless 0.7 → lorawan-device → juggler`), which we never enable (`features = ["wifi", "mqtt", "provisioning"]`).
`Cargo.lock` records feature-gated-off optional deps, and cargo-deny checks the full lockfile.
Fix: add a justified `[advisories] ignore` entry in `deny.toml` (note the chain + that it is never built), rather than chasing a non-existent compiled dependency.
Related: after migrating off git deps, empty `[sources] allow-git` — stale entries emit `unmatched-source` warnings since crates.io/local-path sources never match them.

**`rustyfarian-esp-idf-ws2812 0.7.0` declares `rust-version = "1.95"`, so the ws2812 0.7 wave fails with `requires rustc 1.95 or newer` on the older nightly pin — and staying on 0.6 instead fails the `pennant 0.7` trait bound once network moves.**
The ws2812 crates, `esp-idf-hal 0.47` / `esp-idf-svc 0.53`, the network pin, and the nightly must move as one wave (September 2026: `nightly-2025-12-01` → `nightly-2026-01-26`, which reports `1.95.0-nightly` and satisfies `1.95`).
The nightly is pinned in two places — `rust-toolchain.toml` and `.github/workflows/rust.yml` — and both must change together.
A locally installed rolling `nightly` of the right date is not the dated toolchain: rustup leaves a broken stub and every `rustc` call (including the justfile's `scripts/host-target.sh` backtick) fails with `missing manifest in toolchain`; run `rustup toolchain install nightly-YYYY-MM-DD` explicitly.
Verify the wave without the local `[patch]` redirects too (comment them out, rebuild, restore) — local builds otherwise test the sibling working trees, not the published crates and git rev CI uses.

**The macOS RAM disk at `/Volumes/RustBuilds` is shared by every rustyfarian project, so another repo's ESP-IDF target (several GiB each) can fill it and fail this build with `No space left on device (os error 28)`.**
The error surfaces as unrelated `could not compile core`/linker failures, and a toolchain bump rebuilds everything, so it usually hits during upgrades.
Check with `df -h /Volumes/RustBuilds` and `du -sh /Volumes/RustBuilds/targets/*/*`; build on disk meanwhile with `just --set idf_dir target <recipe>`, or remount a bigger RAM disk (24 GiB fits the full rgb-clock build with room to spare).

---

## Clock Display

**At `DEFAULT_BRIGHTNESS = 10`, the cyan blend from overlapping hour and minute hands is visually indistinguishable from blue.**
When the current minute falls in the same 5-minute LED segment as the hour, both hands land on one LED: blue `(0,0,1)` + green `(0,1,0)` = cyan `(0,1,1)`, which renders as `(0, 10, 10)` after brightness scaling.
At that dim level the green component is imperceptible; the user sees "blue and red" and reports a missing green LED.
Fix: increase `DEFAULT_BRIGHTNESS` (30–50) so the cyan is perceptibly distinct from blue.

---

## Hardware

**Intermittent or briefly flashing LEDs on the WS2812 clock ring are more likely loose cables than firmware bugs.**
WS2812 strips use thin wires that fracture easily.
A momentary open circuit in the data chain causes all LEDs from that point onward to go dark or flash unexpectedly — while the firmware reports no errors and `just monitor` shows normal tick messages.
Diagnostic: flex the cable while watching the LEDs; if the symptom tracks movement, resolder or replace the wire.

---

## Wokwi Simulation

**`save-to` in Wokwi CI scenario files resolves relative to the scenario file, not the project root.**
A scenario at `wokwi/test-startup.yaml` with `save-to: ../screenshots/foo.png` writes to `screenshots/foo.png` at the project root — not `./screenshots/foo.png` relative to whatever directory the CI runner uses.
Keep `save-to` paths relative to the scenario file when authoring new scenarios.
