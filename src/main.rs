mod rgb_clock;

use crate::rgb_clock::RGBClock;
use anyhow::Context;
use esp32_mqtt_manager::{MqttConfig, MqttManager};
use esp32_wifi_manager::{WiFiConfig, WiFiManager};
use esp32_ws2812_rmt::WS2812RMT;
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

fn main() -> anyhow::Result<()> {
    // It is necessary to call this function once. Otherwise, some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take()?;
    let sys_loop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;

    // WiFi credentials from .env
    const WIFI_SSID: &str = env!("WIFI_SSID");
    const WIFI_PASS: &str = env!("WIFI_PASS");

    // ESP32-C6 DevKit onboard RGB LED is on GPIO8
    let mut driver = WS2812RMT::new(peripherals.pins.gpio8, peripherals.rmt.channel0)?;

    // Initialize Wi-Fi with an LED indicator
    let wifi_config = WiFiConfig::new(WIFI_SSID, WIFI_PASS);
    let wifi = WiFiManager::new(
        peripherals.modem,
        sys_loop,
        Some(nvs),
        wifi_config,
        Some(&mut driver),
    )?;

    // Wait some seconds for an IP address
    if let Some(ip) = wifi.get_ip(10000)? {
        log::info!("Got IP address: {:?}", ip);
    } else {
        log::error!("Failed to get IP address within timeout");
    }

    // ESP32-C6 GPI10 for the NeoPixel clock
    let clock_driver = WS2812RMT::new(peripherals.pins.gpio10, peripherals.rmt.channel1)?;
    let rgb_clock = RGBClock::new(clock_driver)?;

    // Wrap clock in Arc<Mutex<>> for sharing between threads
    let clock = Arc::new(Mutex::new(rgb_clock));

    // Start the startup animation in a background thread
    let animation_cancel = Arc::new(AtomicBool::new(false));
    let _animation_handle =
        rgb_clock::run_startup_animation(Arc::clone(&clock), Arc::clone(&animation_cancel));

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
    let mut _mqtt = MqttManager::new(mqtt_config, "tick", move |data: &[u8]| {
        use rgb_clock::LocalTime;

        // Cancel any running startup animation on the first time update
        animation_cancel_clone.store(true, Ordering::Relaxed);

        match LocalTime::try_from(data) {
            Ok(time) => {
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
    })?;
    _mqtt.send_startup_message()?;

    log::info!("Setup complete, parking main thread");
    // Park the main thread indefinitely - MQTT callbacks handle all work
    std::thread::park();

    Ok(())
}
