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

    embuild::espidf::sysenv::output();
}
