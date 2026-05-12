# Project Lore

Non-obvious technical discoveries: facts that caused surprising failures, took significant time to debug, or would save a future developer 30+ minutes if known upfront.

Each entry: **bold fact** → root cause → fix.
To add an entry: read this file first to avoid duplicates, then append to the relevant `##` section (or add a new one).

---

## MQTT & Networking

**Calling `subscribe()` from the MQTT `on_connect` callback deadlocks on `esp-idf-svc 0.52+`.**
`subscribe()` blocks waiting for a SUBACK, but the MQTT event loop is frozen inside the callback and cannot process the SUBACK — a self-deadlock with no clear error message, just a hung application.
Affected versions: `esp-idf-svc 0.52+`.
Fix: set an `AtomicBool` flag in `on_connect`; poll it from a dedicated watcher thread and call `subscribe()` from that normal (non-callback) thread context.
See `src/main.rs` for the implementation.

---

## Wokwi Simulation

**`save-to` in Wokwi CI scenario files resolves relative to the scenario file, not the project root.**
A scenario at `wokwi/test-startup.yaml` with `save-to: ../screenshots/foo.png` writes to `screenshots/foo.png` at the project root — not `./screenshots/foo.png` relative to whatever directory the CI runner uses.
Keep `save-to` paths relative to the scenario file when authoring new scenarios.
