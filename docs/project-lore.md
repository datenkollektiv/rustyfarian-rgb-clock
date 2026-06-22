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

---

## Toolchain & Dependencies

**When `rustyfarian-esp-idf-ws2812` and the network crates both depend on `pennant`, they must resolve to the same compiled `pennant` package or `WiFiManager<L: StatusLed>` fails to compile.**
`WiFiManager::new<L: StatusLed>` requires the `Ws2812Rmt` type to implement the `StatusLed` trait from the *same* compiled `pennant` package.
If the two crates pull `pennant` from different sources (e.g. one from git `v0.5.0`, the other from crates.io `v0.6.0`), Cargo produces two separate packages and the trait bound fails with a confusing type-mismatch error that names `pennant::StatusLed` twice.
Fix: ensure both crates resolve to the same `pennant` source and version. When `rustyfarian-esp-idf-ws2812 v0.6.0` (crates.io) is in use, pin `rustyfarian-network` to a commit that also resolves `pennant` from crates.io `v0.6`.

**When ws2812 crates move from git to crates.io, the `[patch]` key in `.cargo/config.toml` must change from `[patch."<git-url>"]` to `[patch.crates-io]`.**
Cargo's `[patch]` mechanism uses the source URL as the section key.
If the key is wrong, Cargo silently uses the published crates.io version instead of the local sibling repo and emits "patch was not used in the crate graph" warnings — local dev patches have no effect without a clear error.

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
