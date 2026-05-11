mod rgb_clock;

use crate::rgb_clock::RGBClock;
use anyhow::Context;
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use rustyfarian_esp_idf_mqtt::{MqttBuilder, MqttConfig};
use rustyfarian_esp_idf_wifi::{WiFiConfig, WiFiManager};
use rustyfarian_esp_idf_ws2812::WS2812RMT;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

const MQTT_TOPIC: &str = "tick";

fn main() -> anyhow::Result<()> {
    // It is necessary to call this function once. Otherwise, some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take()?;
    let sys_loop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;

    // GPIO 10 for the clock ring and GPIO 8 for the onboard status LED are the
    // same on both supported chips (ESP32-C3 and ESP32-C6). The MCU cfg flag
    // emitted by build.rs is available for future pin divergence if needed.
    let clock_driver = WS2812RMT::new(peripherals.pins.gpio10)?;
    let rgb_clock = RGBClock::new(clock_driver)?;

    // Wrap clock in Arc<Mutex<>> for sharing between threads
    let clock = Arc::new(Mutex::new(rgb_clock));

    // Start the startup animation in a background thread
    let animation_cancel = Arc::new(AtomicBool::new(false));
    let _animation_handle =
        rgb_clock::run_startup_animation(Arc::clone(&clock), Arc::clone(&animation_cancel));

    // WiFi credentials from .env
    const WIFI_SSID: &str = env!("WIFI_SSID");
    const WIFI_PASS: &str = env!("WIFI_PASS");

    // Onboard RGB LED for Wi-Fi status
    let mut onboard_led = WS2812RMT::new(peripherals.pins.gpio8)?;

    let wifi_config = WiFiConfig::new(WIFI_SSID, WIFI_PASS);
    let wifi = WiFiManager::new(
        peripherals.modem,
        sys_loop,
        Some(nvs),
        wifi_config,
        Some(&mut onboard_led),
    )?;

    // Wait some seconds for an IP address
    if let Some(ip) = wifi.get_ip(10000)? {
        log::info!("Got IP address: {:?}", ip);
    } else {
        log::error!("Failed to get IP address within timeout");
    }

    // MQTT configuration from .env
    const MQTT_HOST: &str = env!("MQTT_HOST");
    const MQTT_PORT: &str = env!("MQTT_PORT");
    const MQTT_CLIENT_ID: &str = env!("MQTT_CLIENT_ID");

    // Connect to MQTT broker - MUST assign to a variable to keep it alive!
    let clock_clone = Arc::clone(&clock);
    let animation_cancel_clone = Arc::clone(&animation_cancel);
    let mqtt_port: u16 = MQTT_PORT
        .parse()
        .context("MQTT_PORT must be a valid port number (0-65535)")?;
    let mqtt_config = MqttConfig::new(MQTT_HOST, mqtt_port, MQTT_CLIENT_ID);

    // Calling subscribe() from inside the on_connect callback deadlocks on
    // esp-idf-svc 0.52+: subscribe() blocks waiting for SUBACK, but the event
    // loop is stuck inside the callback and cannot process the SUBACK.
    // Instead, on_connect sets a flag that a dedicated thread picks up and
    // calls subscribe() from a normal (non-callback) thread context.
    let subscribe_needed = Arc::new(AtomicBool::new(false));
    let subscribe_flag = Arc::clone(&subscribe_needed);

    let _mqtt = MqttBuilder::new(mqtt_config)
        .on_connect(move |_client, _is_clean| {
            subscribe_flag.store(true, Ordering::Release);
            Ok(())
        })
        .on_message(move |topic: &str, data: &[u8]| {
            use rgb_clock::LocalTime;

            // Cancel any running startup animation on the first time update
            animation_cancel_clone.store(true, Ordering::Relaxed);

            match LocalTime::try_from(data) {
                Ok(time) => {
                    log::debug!(
                        "tick [{}] -> {:02}:{:02}:{:02}",
                        topic,
                        time.hour,
                        time.minute,
                        time.second
                    );
                    if let Ok(mut c) = clock_clone.lock() {
                        if let Err(e) = c.set_local_time(time) {
                            log::error!("Failed to set time: {:?}", e);
                        }
                    }
                }
                Err(e) => {
                    log::error!("Failed to parse time: {} (raw: {:02x?})", e, data);
                }
            }
        })
        .build()?;

    // Subscription watcher: subscribes to MQTT_TOPIC on every (re)connect.
    // On failure we do NOT re-arm the flag — the next on_connect will set it
    // again, avoiding a retry storm during broker or network outages.
    {
        use esp_idf_svc::mqtt::client::QoS;
        let mqtt_for_sub = _mqtt.clone();
        std::thread::Builder::new()
            .stack_size(4096)
            .name("mqtt-subscribe-watcher".into())
            .spawn(move || loop {
                if subscribe_needed.swap(false, Ordering::AcqRel) {
                    match mqtt_for_sub.subscribe(MQTT_TOPIC, QoS::AtLeastOnce) {
                        Ok(()) => log::info!("Subscribed to '{}'", MQTT_TOPIC),
                        Err(e) => log::error!("Subscribe failed: {:?}", e),
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            })
            .context("failed to spawn subscription watcher thread")?;
    }

    log::info!("Setup complete, parking main thread");
    // Park the main thread indefinitely - MQTT callbacks handle all work
    std::thread::park();

    Ok(())
}
