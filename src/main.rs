mod rgb_clock;

use crate::rgb_clock::RGBClock;
use anyhow::Context;
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::mqtt::client::QoS;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use rustyfarian_esp_idf_network::mqtt::{MqttBuilder, MqttConfig};
use rustyfarian_esp_idf_network::wifi::{WiFiConfig, WiFiManager};
use rustyfarian_esp_idf_ws2812::Ws2812Rmt;
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
    let clock_driver = Ws2812Rmt::new(peripherals.pins.gpio10)?;
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
    let mut onboard_led = Ws2812Rmt::new(peripherals.pins.gpio8)?;

    let wifi_config = WiFiConfig::new(WIFI_SSID, WIFI_PASS);
    let wifi = WiFiManager::new(
        peripherals.modem,
        sys_loop,
        Some(nvs),
        wifi_config,
        Some(&mut onboard_led),
    )?;

    // Wait some seconds for an IP address.
    //
    // We intentionally continue past a timeout rather than returning an error:
    // the MQTT client below runs its own auto-reconnecting background event loop
    // (build() returns immediately and connects asynchronously), so a slow DHCP
    // lease or a Wi-Fi blip at boot should not abort startup. The clock keeps
    // showing its rainbow until the first tick arrives once connectivity is up.
    // A hard failure here would only turn a recoverable, transient delay into a
    // dark device that needs a manual reboot.
    if let Some(ip) = wifi.get_ip(10000)? {
        log::info!("Got IP address: {:?}", ip);
    } else {
        log::error!("Failed to get IP address within timeout; continuing — MQTT will connect once Wi-Fi is up");
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

    // Subscription is managed by the builder, not by this firmware. The
    // contract we rely on from rustyfarian-esp-idf-network (documented on
    // `MqttBuilder::subscribe`): the topic is (re)subscribed on every broker
    // `Connected` event — both the initial connection and every automatic
    // reconnect — and a failed SUBSCRIBE is logged and retried on the next
    // reconnect rather than dropped forever. This replaces the former local
    // on_connect flag + watcher-thread workaround for the esp-idf-svc 0.52+
    // subscribe-in-callback SUBACK deadlock, which now lives in the crate.
    let _mqtt = MqttBuilder::new(mqtt_config)
        .subscribe(MQTT_TOPIC, QoS::AtLeastOnce)
        .on_message(move |topic: &str, data: &[u8]| {
            use rgb_clock::LocalTime;

            // Cancel any running startup animation on the first time update.
            // Relaxed is sufficient: this is a standalone one-bit stop flag that
            // publishes no other memory, and the shared LED state is serialized by
            // the clock mutex below — the animation thread re-reads this flag under
            // that same lock before writing, so no happens-before edge is needed here.
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
                    match clock_clone.lock() {
                        Ok(mut c) => {
                            if let Err(e) = c.set_local_time(time) {
                                log::error!("Failed to set time: {:?}", e);
                            }
                        }
                        // Don't drop ticks silently: a panic in another clock user
                        // (e.g. the startup animation thread) poisons this mutex, which
                        // would otherwise freeze the display forever with no clue in the
                        // logs as to why time updates stopped.
                        Err(e) => log::error!("Clock mutex poisoned, skipping tick: {:?}", e),
                    }
                }
                Err(e) => {
                    log::error!("Failed to parse time: {} (raw: {:02x?})", e, data);
                }
            }
        })
        .build()?;

    log::info!("Setup complete, parking main thread");
    // Park the main thread indefinitely — runtime work runs on background threads:
    // the MQTT event-loop callbacks (time updates) and, during boot, the startup
    // animation thread.
    std::thread::park();

    Ok(())
}
