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

    embuild::espidf::sysenv::output();
}
