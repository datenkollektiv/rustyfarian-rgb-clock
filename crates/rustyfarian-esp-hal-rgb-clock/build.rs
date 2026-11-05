//! Re-runs when the portal prefill variables change and rejects a malformed `MQTT_PORT`.
//! The values are baked by `option_env!` in `src/main.rs`; mirrors the root `build.rs`.
//! Also resolves the MQTT tick topic (`TICK_TOPIC_OVERRIDE`) with the root build's validation.

#[path = "../../build_support/tick_topic.rs"]
mod tick_topic;

/// Longest client id the hal portal stores; longer prefill values are truncated silently.
const MQTT_CLIENT_ID_MAX_LEN: usize = 23;

fn main() {
    for var in [
        "WIFI_SSID",
        "MQTT_HOST",
        "MQTT_PORT",
        "MQTT_USER",
        "MQTT_CLIENT_ID",
    ] {
        println!("cargo:rerun-if-env-changed={var}");
    }

    println!("cargo:rerun-if-changed=../../build_support/tick_topic.rs");
    println!("cargo:rerun-if-env-changed=TICK_TOPIC_OVERRIDE");
    let raw = std::env::var("TICK_TOPIC_OVERRIDE").ok();
    let tick_topic = tick_topic::resolve(raw.as_deref()).unwrap_or_else(|e| panic!("{e}"));
    if tick_topic != tick_topic::DEFAULT_TICK_TOPIC {
        println!("cargo:warning=TICK_TOPIC_OVERRIDE in effect: this image subscribes to \"{tick_topic}\", not \"tick\" (OTA demo only)");
    }
    println!("cargo:rustc-env=TICK_TOPIC={tick_topic}");

    // Guard and use site agree: main.rs trims the same way and treats empty as the default.
    if let Some(port) = std::env::var("MQTT_PORT")
        .ok()
        .map(|p| p.trim_ascii().to_string())
        .filter(|p| !p.is_empty())
    {
        if port.parse::<u16>().is_err() {
            panic!("MQTT_PORT=\"{port}\" is not a valid port number (0-65535); fix it in .env");
        }
    }

    if let Ok(id) = std::env::var("MQTT_CLIENT_ID") {
        if id.trim_ascii().len() > MQTT_CLIENT_ID_MAX_LEN {
            println!(
                "cargo:warning=MQTT_CLIENT_ID is longer than {MQTT_CLIENT_ID_MAX_LEN} bytes; the esp-hal portal truncates it"
            );
        }
    }
}
