#[path = "build_support/tick_topic.rs"]
mod tick_topic;

fn main() {
    // Re-run only if the chip target changes.
    println!("cargo:rerun-if-env-changed=MCU");

    // Emit a cfg flag so main.rs can select chip-specific GPIO assignments with #[cfg(mcu = "...")]
    println!("cargo:rustc-check-cfg=cfg(mcu, values(\"esp32c3\", \"esp32c6\"))");
    let mcu = std::env::var("MCU").unwrap_or_else(|_| "esp32c6".to_string());
    println!("cargo:rustc-cfg=mcu=\"{mcu}\"");

    // Wi-Fi and MQTT credentials are no longer baked into the firmware — they are
    // provisioned at runtime via the SoftAP captive portal and stored in NVS.
    // See docs/features/wifi-softap-provisioning-v1.md.
    //
    // The optional, non-secret portal prefill values (read via `option_env!` in
    // main.rs) are baked at compile time, so rebuild when they change in `.env`.
    for var in [
        "WIFI_SSID",
        "MQTT_HOST",
        "MQTT_PORT",
        "MQTT_USER",
        "MQTT_CLIENT_ID",
    ] {
        println!("cargo:rerun-if-env-changed={var}");
    }

    // Fail fast on a typo'd port rather than baking an invalid form default: a
    // set-but-unparseable MQTT_PORT (e.g. "abc") can never be a valid port.
    // Empty/unset is fine — main.rs falls back to the built-in default.
    if let Some(port) = std::env::var("MQTT_PORT")
        .ok()
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
    {
        if port.parse::<u16>().is_err() {
            panic!("MQTT_PORT=\"{port}\" is not a valid port number (0-65535); fix it in .env");
        }
    }

    // The version the firmware reports and compares OTA offers against. Normally
    // the package version; `just ota-artifact version=X` overrides it so a demo
    // image really carries the version its manifest advertises. A manifest that
    // promises a version the image does not report is refused at the next boot
    // (see `src/ota/mod.rs`, `reconcile_boot`), so a manifest-only override
    // would only ever roll itself back.
    println!("cargo:rerun-if-env-changed=FIRMWARE_VERSION_OVERRIDE");
    let firmware_version = match std::env::var("FIRMWARE_VERSION_OVERRIDE")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
    {
        Some(v) => {
            // Each part must fit the `u16` fields of the network crate's `Version`,
            // or every device would reject the manifest as `manifest_invalid`.
            let semver =
                v.split('.').count() == 3 && v.split('.').all(|part| part.parse::<u16>().is_ok());
            if !semver {
                panic!(
                    "FIRMWARE_VERSION_OVERRIDE=\"{v}\" is not MAJOR.MINOR.PATCH with parts below 65536"
                );
            }
            v
        }
        None => std::env::var("CARGO_PKG_VERSION").expect("cargo sets CARGO_PKG_VERSION"),
    };
    println!("cargo:rustc-env=FIRMWARE_VERSION={firmware_version}");

    // The MQTT topic the clock takes its time from. Normally `tick`; demo-only
    // `OTA_TICK_TOPIC=X just ota-artifact` points an OTA image at a topic nobody
    // publishes, so the release health check genuinely never sees a fresh tick
    // and the 120 s deadline rollback runs while the shared publisher keeps going.
    // Validation lives in `build_support/tick_topic.rs` so `just tick-topic-tests`
    // can unit-test it on the host.
    println!("cargo:rerun-if-changed=build_support/tick_topic.rs");
    println!("cargo:rerun-if-env-changed=TICK_TOPIC_OVERRIDE");
    let raw = std::env::var("TICK_TOPIC_OVERRIDE").ok();
    let tick_topic = tick_topic::resolve(raw.as_deref()).unwrap_or_else(|e| panic!("{e}"));
    if tick_topic != tick_topic::DEFAULT_TICK_TOPIC {
        println!("cargo:warning=TICK_TOPIC_OVERRIDE in effect: this image subscribes to \"{tick_topic}\", not \"tick\" (OTA demo only)");
    }
    println!("cargo:rustc-env=TICK_TOPIC={tick_topic}");

    embuild::espidf::sysenv::output();
}
