mod rgb_clock;

use crate::rgb_clock::RGBClock;
use anyhow::Context;
use esp_idf_hal::gpio::Gpio8;
use esp_idf_hal::modem::Modem;
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::mqtt::client::QoS;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use rustyfarian_esp_idf_network::mqtt::MqttBuilder;
use rustyfarian_esp_idf_network::provisioning::{
    run_wifi_mqtt_portal, BootConfig, PortalConfig, PortalOutcome, ProvisioningEvent,
    ProvisioningStore, SchemaProfile, WifiMqttBoot, WifiMqttLoadOutcome,
};
use rustyfarian_esp_idf_network::wifi::WiFiManager;
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
    let clock = Arc::new(Mutex::new(RGBClock::new(clock_driver)?));

    // Load the stored Wi-Fi + MQTT config, or fall into provisioning. The network
    // crate's `WifiMqttBoot::load` is modem-free, so on the provisioned path we
    // still own `peripherals.modem` for the STA boot; only the portal path claims
    // it. See docs/features/wifi-softap-provisioning-v1.md.
    let boot = match WifiMqttBoot::load(nvs.clone()).context("failed to read provisioning store")? {
        WifiMqttLoadOutcome::Ready(boot) => boot,
        WifiMqttLoadOutcome::NotProvisioned => {
            log::info!("Not provisioned — entering SoftAP provisioning");
            return run_provisioning(peripherals.modem, sys_loop, nvs, clock);
        }
        WifiMqttLoadOutcome::OtherProfile(profile) => {
            log::warn!("Provisioned under {profile:?}, not WifiMqttDevice — re-provisioning");
            return run_provisioning(peripherals.modem, sys_loop, nvs, clock);
        }
        // `WifiMqttLoadOutcome` is `#[non_exhaustive]`.
        other => {
            log::warn!("Unexpected load outcome {other:?} — entering provisioning");
            return run_provisioning(peripherals.modem, sys_loop, nvs, clock);
        }
    };

    log::info!("Provisioned (wifi_mqtt) — booting clock");
    run_clock(
        peripherals.modem,
        sys_loop,
        nvs,
        peripherals.pins.gpio8,
        clock,
        boot,
    )
}

/// Boots the clock in normal STA mode from a loaded provisioning bundle.
///
/// Steady-state path: rainbow startup animation, Wi-Fi connect, MQTT subscribe,
/// then park while callbacks drive the display.
fn run_clock(
    modem: Modem<'static>,
    sys_loop: EspSystemEventLoop,
    nvs: EspDefaultNvsPartition,
    onboard_pin: Gpio8,
    clock: Arc<Mutex<RGBClock<'static>>>,
    boot: WifiMqttBoot,
) -> anyhow::Result<()> {
    // Start the startup animation in a background thread
    let animation_cancel = Arc::new(AtomicBool::new(false));
    let _animation_handle =
        rgb_clock::run_startup_animation(Arc::clone(&clock), Arc::clone(&animation_cancel));

    // Onboard RGB LED for Wi-Fi status
    let mut onboard_led = Ws2812Rmt::new(onboard_pin)?;

    // `boot` owns the config strings; `wifi_config()` / `mqtt_config()` borrow them
    // and are valid as long as `boot` lives (it outlives both uses below).
    let wifi = WiFiManager::new(
        modem,
        sys_loop,
        Some(nvs),
        boot.wifi_config(),
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

    let clock_clone = Arc::clone(&clock);
    let animation_cancel_clone = Arc::clone(&animation_cancel);

    let mqtt_config = boot.mqtt_config();
    // Lengths only — never the secret values. Makes a rejected (e.g. empty) host
    // or client_id diagnosable from serial without a debugger.
    log::info!(
        "MQTT target: host len={}, port={}, client_id len={}",
        mqtt_config.host.len(),
        mqtt_config.port,
        mqtt_config.client_id.len()
    );

    // Subscription is managed by the builder, not by this firmware. The
    // contract we rely on from rustyfarian-esp-idf-network (documented on
    // `MqttBuilder::subscribe`): the topic is (re)subscribed on every broker
    // `Connected` event — both the initial connection and every automatic
    // reconnect — and a failed SUBSCRIBE is logged and retried on the next
    // reconnect rather than dropped forever.
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

/// Runs the SoftAP captive portal until a terminal outcome, then restarts.
///
/// On the first boot (empty store) the ring shows the amber "pairing mode" pulse while
/// the open AP and portal wait for the user to submit Wi-Fi + MQTT settings. The
/// network crate owns the portal lifecycle and never reboots/erases itself — this
/// firmware cancels the pulse, handles the outcome, and restarts.
fn run_provisioning(
    modem: Modem<'static>,
    sys_loop: EspSystemEventLoop,
    nvs: EspDefaultNvsPartition,
    clock: Arc<Mutex<RGBClock<'static>>>,
) -> anyhow::Result<()> {
    // Amber "pairing mode" pulse on the ring while the portal waits.
    let pulse_cancel = Arc::new(AtomicBool::new(false));
    let _pulse_handle =
        rgb_clock::run_provisioning_animation(Arc::clone(&clock), Arc::clone(&pulse_cancel));

    // Keep a clone for the factory-reset erase path before `nvs` is moved.
    let nvs_for_erase = nvs.clone();

    // Open AP (no PSK): a conscious v1 tradeoff for local, physical first-boot
    // setup. See the "Security stance" section of the feature doc.
    let boot_config = BootConfig {
        portal: PortalConfig {
            ssid_prefix: "Rustyfarian",
            ap_password: None,
            channel: 1,
            device_name: "rgb-clock",
            firmware_version: env!("CARGO_PKG_VERSION"),
            profile: SchemaProfile::WifiMqttDevice,
        },
        // First boot has no deadline — block until the user provisions.
        portal_timeout: None,
        on_event: Some(Arc::new(|event: ProvisioningEvent| {
            log::info!("Provisioning event: {event:?}");
        })),
    };

    let outcome = run_wifi_mqtt_portal(modem, sys_loop, nvs, boot_config)
        .context("provisioning portal failed")?;

    // Stop the amber pulse before restarting.
    pulse_cancel.store(true, Ordering::Relaxed);

    match outcome {
        PortalOutcome::JustProvisioned => {
            log::info!("Provisioning committed — restarting into normal boot");
        }
        PortalOutcome::FactoryResetRequested => {
            // Erase the provisioning namespace so the next boot re-enters the portal.
            // Best-effort: log and restart even if the erase fails — a stranded
            // device is worse than a retry on the next boot.
            match ProvisioningStore::open(nvs_for_erase).and_then(|mut s| s.erase_all()) {
                Ok(()) => log::info!("Factory reset — provisioning store erased"),
                Err(e) => log::warn!("Factory reset: erase_all failed (retry next boot): {e:#}"),
            }
        }
        PortalOutcome::PortalExitedWithoutCommit => {
            log::warn!("Portal exited without a commit — restarting to re-open the portal");
        }
        // `PortalOutcome` is `#[non_exhaustive]`.
        other => log::warn!("Unexpected portal outcome {other:?} — restarting"),
    }

    esp_idf_svc::hal::reset::restart()
}
