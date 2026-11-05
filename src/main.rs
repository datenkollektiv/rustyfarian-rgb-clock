mod ota;
mod rgb_clock;

use crate::ota::policy::HealthSignals;
use crate::rgb_clock::IdfRing;
use anyhow::Context;
use clock_app::{ClockAppConfig, Hooks};
use clock_pure::{encode_tick_ms, prefill_or, ClockColors, DEFAULT_BRIGHTNESS};
use esp_idf_hal::modem::Modem;
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use rustyfarian_esp_idf_network::mqtt::{MqttBuilder, QoS};
use rustyfarian_esp_idf_network::ota::runtime::OtaSubmitter;
use rustyfarian_esp_idf_network::provisioning::{
    run_wifi_mqtt_portal, BootConfig, DeviceName, PortalConfig, PortalDefaults, PortalOutcome,
    ProvisioningEvent, ProvisioningStore, SchemaProfile, WifiMqttBoot, WifiMqttLoadOutcome,
};
use rustyfarian_esp_idf_network::wifi::WiFiManager;
use rustyfarian_esp_idf_ws2812::Ws2812Rmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

// Links the generic timer queue that backs `embassy_time::Timer` on the esp-idf time driver.
use embassy_time_queue_utils as _;

/// Time source topic, `tick` unless a demo OTA image was built with
/// `OTA_TICK_TOPIC` (see `build.rs`) to fail its health check on purpose.
const MQTT_TOPIC: &str = env!("TICK_TOPIC");

/// SoftAP SSID prefix; the network crate appends the AP MAC's last two bytes as
/// `Rustyfarian-XXXX`. Also documented in `docs/features/wifi-softap-provisioning-v1.md`.
const AP_SSID_PREFIX: &str = "Rustyfarian";
/// Device name shown on the portal `/status` page. Doubles as the MQTT client-id
/// prefill fallback (the client-id defaults to the device name when unset).
const DEVICE_NAME: &str = "rgb-clock";
/// MQTT broker port prefilled in the portal form when `MQTT_PORT` is not supplied
/// at build time. The IANA-assigned plaintext-MQTT port.
const DEFAULT_MQTT_PORT: &str = "1883";
/// Stack of the `clock-app` thread: the async app, ring writes and the OTA intake hook.
const CLOCK_APP_STACK_SIZE: usize = 16 * 1024;

fn main() -> anyhow::Result<()> {
    // The OTA health dwell and failure deadline are measured from here: the
    // runtime requires the boot instant, not association.
    let booted_at = Instant::now();
    // It is necessary to call this function once. Otherwise, some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take()?;
    let sys_loop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;

    // Before anything that could crash: if this boot came from the slot the
    // last OTA attempt targeted, record that the attempt was activated, so a
    // later rollback of this image is still reported.
    // Kept as a `Result`: the provisioning paths below must not depend on it;
    // the clock path refuses to start without it.
    let ota_records = ota::open_records(nvs.clone()).context("failed to open the OTA records");
    if let Err(e) = &ota_records {
        log::error!("{e:#}");
    }

    // The clock ring follows the rustyfarian standard WS2812 data pin per chip
    // (ESP32-C3: GPIO 4, ESP32-C6: GPIO 18), the same wiring as the
    // `rustyfarian-ws2812` examples and the shared pogo-pin test rigs. The MCU
    // cfg flag is emitted by build.rs. GPIO 8 stays the onboard status LED on
    // both chips.
    #[cfg(mcu = "esp32c3")]
    let clock_driver = Ws2812Rmt::new(peripherals.pins.gpio4)?;
    #[cfg(mcu = "esp32c6")]
    let clock_driver = Ws2812Rmt::new(peripherals.pins.gpio18)?;
    let clock = IdfRing::new(clock_driver);

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
        clock,
        boot,
        OtaBoot {
            records: ota_records,
            booted_at,
        },
    )
}

/// What the clock path needs from the early OTA boot steps in `main`.
struct OtaBoot {
    /// The record store, or why it could not be opened; the clock path refuses
    /// to start without it.
    records: anyhow::Result<rustyfarian_esp_idf_network::ota::runtime::Records>,
    /// Captured first in `main`: the health dwell and deadline are measured
    /// from it.
    booted_at: Instant,
}

/// Tier hooks of the shared clock app: the OTA health signal and OTA command intake.
struct IdfHooks {
    booted_at: Instant,
    health: Arc<HealthSignals>,
    submitter: OtaSubmitter,
}

impl Hooks for IdfHooks {
    fn tick_rendered(&mut self, _at: embassy_time::Instant) {
        // Freshness signal for the OTA health policy: when the last tick was
        // received, parsed and rendered. Never 0, which means "no tick yet".
        let since_boot = u64::try_from(self.booted_at.elapsed().as_millis()).unwrap_or(u64::MAX);
        self.health
            .last_tick_ms
            .store(encode_tick_ms(since_boot), Ordering::Relaxed);
    }

    /// Enqueue only (ADR 005): `handle` parses and does `try_send`/atomics, and never
    /// publishes; rejections are reported by the OTA reporter thread.
    fn other_message(&mut self, topic: &str, payload: &[u8]) -> bool {
        self.submitter.handle(topic, payload)
    }
}

/// The app's diagnostics hook, run on the `clock-app` thread on link-up and after the
/// first tick: heap, and the thread's stack high-water mark (bytes on ESP-IDF, see lore).
fn app_diagnostics(when: &str) {
    log_heap(when);
    // SAFETY: a null handle queries the calling task; no other preconditions.
    let stack_free =
        unsafe { esp_idf_svc::sys::uxTaskGetStackHighWaterMark(core::ptr::null_mut()) };
    log::info!("clock-app stack high-water mark {when}: {stack_free} bytes free");
}

/// Logs free and minimum-ever-free heap, to judge the heap budget from serial.
fn log_heap(when: &str) {
    // SAFETY: reads heap statistics; no preconditions.
    let (free, min_free) = unsafe {
        (
            esp_idf_svc::sys::esp_get_free_heap_size(),
            esp_idf_svc::sys::esp_get_minimum_free_heap_size(),
        )
    };
    log::info!("Heap {when}: free={free} min_free={min_free}");
}

/// Boots the clock in normal STA mode from a loaded provisioning bundle.
///
/// Steady-state path: Wi-Fi (non-blocking, retried forever), MQTT subscribe, OTA
/// worker, the shared clock app on its own thread (rainbow until the first valid
/// tick, then the hands), and the OTA health policy on the main thread.
fn run_clock(
    modem: Modem<'static>,
    sys_loop: EspSystemEventLoop,
    nvs: EspDefaultNvsPartition,
    clock: IdfRing,
    boot: WifiMqttBoot,
    ota_boot: OtaBoot,
) -> anyhow::Result<()> {
    let OtaBoot {
        records: ota_records,
        booted_at,
    } = ota_boot;
    log_heap("at boot");

    // `boot` owns the config strings; `wifi_config()` / `mqtt_config()` borrow them
    // and are valid as long as `boot` lives (it outlives both uses below).
    //
    // Non-blocking: the ring belongs to the clock app, which shows the rainbow until
    // the first tick, and the link supervisor reconnects forever. A Wi-Fi failure no
    // longer reboot-loops after a timeout; the BOOT portal is the recovery path.
    let wifi = WiFiManager::new_without_led(
        modem,
        sys_loop,
        Some(nvs),
        boot.wifi_config().connect_nonblocking(),
    )?;

    // Link readiness (associated with an IP) for the OTA health predicate: the
    // manager itself moves to the app thread. The sink must return at once.
    let wifi_ready = Arc::new(AtomicBool::new(false));
    let ready_for_sink = Arc::clone(&wifi_ready);
    // Seed only `true`: the sink may already have stored a newer value from the
    // reconnect worker, and a stale `false` snapshot must not overwrite it.
    if wifi.subscribe_ready(move |ready| ready_for_sink.store(ready, Ordering::Relaxed)) {
        wifi_ready.store(true, Ordering::Relaxed);
    }

    // OTA: the submitter is handed to the app's hooks; the worker and reporter
    // threads start once the MQTT handle exists. Intake only parses and enqueues,
    // and never publishes (bug 001).
    let (ota_submitter, ota_runtime) = ota::channel(ota::config()?);
    let health = Arc::new(HealthSignals::default());

    let mqtt_config = boot.mqtt_config();
    // Lengths only — never the secret values. Makes a rejected (e.g. empty) host
    // or client_id diagnosable from serial without a debugger.
    log::info!(
        "MQTT target: host len={}, port={}, client_id len={}",
        mqtt_config.host.len(),
        mqtt_config.port,
        mqtt_config.client_id.len()
    );
    if MQTT_TOPIC != "tick" {
        log::warn!(
            "Tick topic override in effect: subscribing to {MQTT_TOPIC:?} instead of \"tick\""
        );
    }

    // Subscription is managed by the builder, not by this firmware. The
    // contract we rely on from rustyfarian-esp-idf-network (documented on
    // `MqttBuilder::subscribe`): the topic is (re)subscribed on every broker
    // `Connected` event — both the initial connection and every automatic
    // reconnect — and a failed SUBSCRIBE is logged and retried on the next
    // reconnect rather than dropped forever.
    //
    // No `on_message`: an inbound message goes either to the callback or to the
    // receive queue, never both. The clock app drains the queue and hands every
    // non-tick topic (`ota/command`) to `IdfHooks::other_message`.
    let mqtt = MqttBuilder::new(mqtt_config)
        .subscribe(MQTT_TOPIC, QoS::AtLeastOnce)
        .subscribe(ota::COMMAND_TOPIC, QoS::AtLeastOnce)
        .build(&wifi)?;

    // A firmware that cannot take further updates must not boot silently: a
    // failed spawn of the worker or reporter thread aborts startup, and a
    // pending slot then rolls back.
    let ota = ota_runtime.start(mqtt.clone(), ota_records?)?;

    // The shared clock app runs on its own thread and owns `wifi` (dropping it would
    // disconnect) and the ring. The health policy stays on the main thread.
    let hooks = IdfHooks {
        booted_at,
        health: Arc::clone(&health),
        submitter: ota_submitter,
    };
    let app_mqtt = mqtt.clone();
    let mut ring = clock;
    std::thread::Builder::new()
        .name("clock-app".into())
        .stack_size(CLOCK_APP_STACK_SIZE)
        .spawn(move || {
            let cfg = ClockAppConfig {
                tick_topic: MQTT_TOPIC,
                colors: ClockColors::default(),
                brightness: DEFAULT_BRIGHTNESS,
                diagnostics: Some(app_diagnostics),
            };
            esp_idf_hal::task::block_on(clock_app::run(&wifi, &app_mqtt, &mut ring, hooks, cfg));
        })
        .context("failed to spawn the clock-app thread")?;

    log::info!("Setup complete, running the OTA health policy on the main thread");
    // Returns at once unless the running slot is pending verification; otherwise
    // marks it valid once healthy or rolls back at the deadline.
    let verdict = ota.run_health_policy(
        booted_at,
        ota::policy::healthy(booted_at, &health, &mqtt, &wifi_ready),
    );
    log::info!(
        "OTA health policy finished: {verdict:?}; stack marks {:?}",
        ota.stack_high_water()
    );

    log::info!("Parking main thread");
    // Park the main thread indefinitely — runtime work runs on background threads:
    // the clock app, the MQTT event loop, and the OTA worker and reporter.
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
    clock: IdfRing,
) -> anyhow::Result<()> {
    // Amber "pairing mode" pulse on the ring while the portal waits. The thread
    // runs until the restart below.
    let _pulse_handle = rgb_clock::run_provisioning_animation(clock);

    // Keep a clone for the factory-reset erase path before `nvs` is moved.
    let nvs_for_erase = nvs.clone();

    // Open AP (no PSK): a conscious v1 tradeoff for local, physical first-boot
    // setup. See the "Security stance" section of the feature doc.
    let boot_config = BootConfig {
        portal: PortalConfig {
            ssid_prefix: AP_SSID_PREFIX,
            // Keep the per-device MAC-derived `Rustyfarian-XXXX` SSID (no override).
            ssid_override: None,
            ap_password: None,
            channel: 1,
            device_name: DeviceName::Fixed(DEVICE_NAME),
            firmware_version: ota::RUNNING_VERSION,
            profile: SchemaProfile::WifiMqttDevice,
            // Non-secret prefill so a fresh device shows sensible values instead of
            // empty fields. Universally-sane fallbacks apply out of the box;
            // deployment-specific fields come from an optional `.env` at build time
            // (see `.env.example`). Secrets (`wifi_pass`, `mqtt_pass`) are never
            // included — the API redacts them and re-prompts on submit.
            //
            // Note: upstream renders the `mqtt_host:port` prefill only when the host
            // is non-empty, so `mqtt_port` surfaces in the form only alongside a set
            // `MQTT_HOST` (see `PortalDefaults` / `compose_mqtt_uri` upstream). A
            // typo'd `MQTT_PORT` is caught at build time in `build.rs`.
            defaults: PortalDefaults {
                wifi_ssid: option_env!("WIFI_SSID").unwrap_or(""),
                mqtt_host: option_env!("MQTT_HOST").unwrap_or(""),
                // Trim + empty-fallback to agree exactly with the `build.rs` guard,
                // so a padded `MQTT_PORT` can't slip a whitespace-y port into the form.
                mqtt_port: prefill_or(option_env!("MQTT_PORT"), DEFAULT_MQTT_PORT),
                mqtt_user: option_env!("MQTT_USER").unwrap_or(""),
                mqtt_client: prefill_or(option_env!("MQTT_CLIENT_ID"), DEVICE_NAME),
                ..PortalDefaults::default()
            },
        },
        // First boot has no deadline — block until the user provisions.
        portal_timeout: None,
        on_event: Some(Arc::new(|event: ProvisioningEvent| {
            log::info!("Provisioning event: {event:?}");
        })),
    };

    let outcome = run_wifi_mqtt_portal(modem, sys_loop, nvs, boot_config)
        .context("provisioning portal failed")?;

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
