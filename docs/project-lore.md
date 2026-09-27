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

**Portal form prefills are baked in at build time via `option_env!`, so a commented-out `.env` silently yields an empty form.**
The broker URL prefills only when `MQTT_HOST` is set, and editing `.env` without a rebuild changes nothing.
Fix: set `WIFI_SSID` and `MQTT_HOST` (plain hostname, no scheme or port) in `.env` and rebuild.
Historical: network `0.5.0` made `ota_url` optional for `WifiMqttDevice` (ADR 014 amendment), prefilled `dev_name` from the configured `device_name`, and re-rendered a rejected `POST /save` through `load_prefill` instead of an empty form.
Before that, empty OTA-URL and device-name fields made every submission fail with "N field error(s)" and wiped the whole form on each retry.

**A `Stack protection fault` in task `main` right after `Provisioning event: Committed` is a 40-byte main-task stack overflow inside the network crate, not a provisioning failure.**
`ProvisioningSession::wait_committed` in `rustyfarian-esp-idf-network 0.5.0` does `guard.committed.clone()` on the caller's stack, and `Option<ProvisioningConfig>` is over a kilobyte of fixed-capacity `heapless` strings, which overruns `CONFIG_ESP_MAIN_TASK_STACK_SIZE=8000` after the portal's own frames.
The credentials are already persisted, and the `SW_CPU` reboot lands on the provisioned path, so the clock comes up and the crash is easy to miss; the firmware's own "Provisioning committed — restarting" line never prints.
The panic's stack-memory dump contains the freshly submitted Wi-Fi and MQTT secrets in clear text, so never paste it into an issue or commit.
Fix: `CONFIG_ESP_MAIN_TASK_STACK_SIZE=16384` in `sdkconfig.defaults` (applied 2026-09-27, verified on hardware after `just clean-idf`); the upstream clone should still move off the caller's stack, see `docs/outbox/rustyfarian-network-wait-committed-stack-clone.md`.
Symbolize a C6 dump with `riscv32-esp-elf-addr2line` from `~/.espressif/tools/esp-clang/*/esp-clang/bin/` against the release ELF; no `just` recipe wraps it yet.

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

## OTA & Firmware Update

**A truncated or corrupted firmware image does not exercise bootloader rollback — it never reaches the bootloader.**
`OtaSession::fetch_and_apply` compares the streamed SHA-256 against the expected digest *before* calling `complete()`, which is the call that sets the boot partition.
A digest mismatch aborts the write and leaves the boot slot unchanged, so the device keeps running the old image and no rollback occurs.
Rollback fires only when a well-formed, correctly-hashed image is activated and then fails to call `esp_ota_mark_app_valid_cancel_rollback()` before the next reboot, moving the slot from `PENDING_VERIFY` to `ABORTED`.
`INVALID` is the state an *explicit* `esp_ota_mark_app_invalid_rollback_and_reboot()` sets, not the automatic path — the two are easy to conflate when reading a slot's state during a rollback demo.
Note also that withholding `mark_valid` does not itself reboot anything: something else must trigger the reset (a watchdog, a crash, or a power cycle) before the bootloader can act.
Fix: to demonstrate rollback, build a valid image that deliberately skips `mark_valid` and reboots; use the truncated image to demonstrate verify-before-swap instead.

**A cached bootloader silently disables rollback while every log line still looks healthy.**
`CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE=y` in `sdkconfig.defaults` only takes effect once the bootloader is rebuilt, and `mark_valid()` returns success against a rollback-disabled bootloader.
Fix: run `just clean-idf` after changing `sdkconfig.defaults`, and treat a passing `mark_valid()` as no evidence that rollback is armed.

**`espflash` writes its own bundled ESP-IDF v5.5.1 bootloader unless `--bootloader` is passed, so no `sdkconfig` bootloader setting in this repo currently reaches the device.**
`scripts/flash.sh` does not pass `--bootloader`, so the bootloader on the chip is never the one `esp-idf-sys` builds from `sdkconfig.defaults`, and it uses a 32 KB MMU page size against the v5.3.3 app's 64 KB.
Any bootloader-level feature — rollback above all — is therefore inert while the build logs look correct.
Fix: `scripts/flash.sh` now builds first and passes `--bootloader` plus `--ignore-app-descriptor`, resolving the path via `find_idf_bootloader` in `scripts/lib.sh`; `just bootloader-path` prints what it will use.
A second `esp-idf-sys-*` build directory makes that resolution ambiguous and is a hard error — run `just clean-idf`.

**App partition offsets must be 64 KiB aligned; 4 KiB alignment is only enough for data partitions.**
`gen_esp32part.py` rejects a misaligned `app` partition, and the v5.3.3 bootloader maps flash in 64 KiB MMU pages.
Fix: start the first app slot at the next `0x10000` boundary and accept the padding, rather than packing app partitions tightly behind `otadata`/`phy_init`.

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
