#[cfg(feature = "ddi-lab")]
mod ota;
mod rgb_clock;

use crate::rgb_clock::IdfRing;
use anyhow::Context;
use clock_app::ClockAppConfig;
use clock_pure::{parse_hands, prefill_or, ClockColors};
use esp_idf_hal::delay::FreeRtos;
use esp_idf_hal::gpio::{InputPin, PinDriver, Pull};
use esp_idf_hal::modem::Modem;
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use rustyfarian_esp_idf_network::mqtt::{MqttBuilder, QoS};
use rustyfarian_esp_idf_network::provisioning::{
    run_wifi_mqtt_portal, BootConfig, DeviceName, PortalConfig, PortalDefaults, PortalOutcome,
    ProvisioningEvent, ProvisioningStore, ReentryRequest, SchemaProfile, WifiMqttBoot,
    WifiMqttLoadOutcome,
};
use rustyfarian_esp_idf_network::wifi::WiFiManager;
use rustyfarian_esp_idf_ws2812::Ws2812Rmt;
use std::sync::Arc;
use std::time::{Duration, Instant};

// Links the generic timer queue that backs `embassy_time::Timer` on the esp-idf time driver.
use embassy_time_queue_utils as _;

/// Time source topic, `tick` unless a demo lab image was built with
/// `TICK_TOPIC_OVERRIDE` (see `build.rs`) to fail its health check on purpose.
const MQTT_TOPIC: &str = env!("TICK_TOPIC");

/// SoftAP SSID prefix; the network crate appends the AP MAC's last two bytes as
/// `Rustyfarian-XXXX`. Also documented in `docs/features/archive/wifi-softap-provisioning-v1.md`.
const AP_SSID_PREFIX: &str = "Rustyfarian";
/// Device name prefix shown on the portal `/status` page; the network crate appends the
/// MAC suffix, giving the device id `rgb-clock-<mac4>` (also the DDI controller id).
const DEVICE_NAME: &str = "rgb-clock";
/// Version this image reports: the package version, or `FIRMWARE_VERSION_OVERRIDE` for
/// bundles (validated in `build.rs`). The portal shows it in every build.
const FIRMWARE_VERSION: &str = env!("FIRMWARE_VERSION");
/// MQTT broker port prefilled in the portal form when `MQTT_PORT` is not supplied
/// at build time. The IANA-assigned plaintext-MQTT port.
const DEFAULT_MQTT_PORT: &str = "1883";
/// Stack of the `clock-app` thread: the async app, ring writes and the nudge intake hook.
const CLOCK_APP_STACK_SIZE: usize = 16 * 1024;
/// Holding BOOT this long answers `FactoryReset`; a shorter press answers `Portal`.
const BOOT_HOLD_MS: u64 = 3000;
/// How long after the prompt a BOOT press is accepted; every normal boot waits this long.
const BOOT_WINDOW_MS: u64 = 2000;
/// A low read is confirmed after this delay, so one noisy read does not open the portal.
const BOOT_DEBOUNCE_MS: u32 = 20;
/// Poll interval for the BOOT window and while BOOT is held.
const BOOT_POLL_MS: u32 = 20;

fn main() -> anyhow::Result<()> {
    // The DDI health dwell and failure deadline are measured from here: the
    // runtime requires the boot instant, not association.
    #[cfg(feature = "ddi-lab")]
    let booted_at = Instant::now();
    // It is necessary to call this function once. Otherwise, some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    let result = start(
        #[cfg(feature = "ddi-lab")]
        booted_at,
    );
    // Returning from `main` only ends the main task: a lab image must not stop
    // silently, because a pending slot would stay unverified until someone resets the
    // board. Every startup error, the portal's included, restarts the device instead,
    // and the bootloader then rolls a pending slot back.
    #[cfg(feature = "ddi-lab")]
    if let Err(e) = &result {
        ota::restart_after_failed_start(e);
    }
    result
}

/// Everything after the logger: peripherals, the provisioning store, then either the
/// portal or the clock. `main` decides what an error means.
fn start(#[cfg(feature = "ddi-lab")] booted_at: Instant) -> anyhow::Result<()> {
    let peripherals = Peripherals::take()?;
    let sys_loop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;

    // Before anything that could crash: if this boot came from the slot the
    // last OTA attempt targeted, record that the attempt was activated, so a
    // later rollback of this image is still reported.
    #[cfg(feature = "ddi-lab")]
    let lab_boot = {
        ota::log_nvs_stats();
        ota::LabBoot::open(booted_at, nvs.clone())
    };

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

    // Ask the BOOT re-entry hook, then load the stored Wi-Fi + MQTT config, or fall
    // into provisioning. The hook runs before the store is read, and the load is
    // modem-free, so on the provisioned path we still own `peripherals.modem` for the
    // STA boot; only the portal path claims it.
    // See docs/features/archive/wifi-softap-provisioning-v1.md.
    let gpio9 = peripherals.pins.gpio9;
    log::info!(
        "Re-entry: press BOOT now (within {BOOT_WINDOW_MS} ms) to reopen the portal (hold {BOOT_HOLD_MS} ms to factory-reset)"
    );
    let loaded = WifiMqttBoot::load_with_reentry(
        nvs.clone(),
        || boot_button_reentry(gpio9),
        WifiMqttBoot::default_client_id,
    )
    .context("failed to read provisioning store")?;
    let boot = match loaded {
        WifiMqttLoadOutcome::Ready(boot) => boot,
        WifiMqttLoadOutcome::NotProvisioned => {
            log::info!("Not provisioned — entering SoftAP provisioning");
            return run_provisioning(peripherals.modem, sys_loop, nvs, clock);
        }
        WifiMqttLoadOutcome::PortalRequested => {
            log::info!("Re-entry: portal requested — the stored record is kept");
            return run_provisioning(peripherals.modem, sys_loop, nvs, clock);
        }
        WifiMqttLoadOutcome::FactoryResetRequested => {
            // A pending update rolls back before the erase, so the previous image keeps
            // the stored record and the hold can be repeated there. An unreadable slot
            // state skips the erase: the image may be pending.
            #[cfg(feature = "ddi-lab")]
            let erase = ota::roll_back_if_pending_before_portal();
            #[cfg(not(feature = "ddi-lab"))]
            let erase = true;
            // The library never erases. Best-effort: continue into the portal even if
            // the erase fails or is skipped, so the board stays reachable.
            if erase {
                match ProvisioningStore::open(nvs.clone()).and_then(|mut s| s.erase_all()) {
                    Ok(()) => log::info!("Re-entry: stored record erased"),
                    Err(e) => {
                        log::warn!("Re-entry: erase failed ({e:#}); continuing into the portal")
                    }
                }
            } else {
                log::warn!(
                    "Re-entry: factory reset skipped, the slot state is unknown; the stored record is kept and the portal can change it"
                );
            }
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

    // A record stored before the MAC suffix carries the bare prefix, and the portal
    // keeps a stored name: two such clocks would share one DDI target and MQTT client id.
    #[cfg(feature = "ddi-lab")]
    if boot.device_name() == DEVICE_NAME {
        log::warn!(
            "Stored device id is the legacy shared {DEVICE_NAME:?}; erase the flash (`just erase-flash`), flash again and provision again to get a per-device id"
        );
    }

    // The lab image cannot poll without a valid DDI token; the portal requires one
    // under the DDI identity rule, so a record without it is provisioned again.
    #[cfg(feature = "ddi-lab")]
    if let Err(e) = ota::token(boot.config()) {
        log::error!("{e:#} — re-provisioning");
        return run_provisioning(peripherals.modem, sys_loop, nvs, clock);
    }

    log::info!("Provisioned (wifi_mqtt) — booting clock");
    run_clock(
        peripherals.modem,
        sys_loop,
        nvs,
        clock,
        boot,
        #[cfg(feature = "ddi-lab")]
        lab_boot,
    )
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
/// Steady-state path: Wi-Fi (non-blocking, retried forever), MQTT subscribe, the
/// shared clock app on its own thread (rainbow until the first valid tick, then the
/// hands) and, in the lab image, the DDI runtime and its health policy on the main thread.
fn run_clock(
    modem: Modem<'static>,
    sys_loop: EspSystemEventLoop,
    nvs: EspDefaultNvsPartition,
    clock: IdfRing,
    boot: WifiMqttBoot,
    #[cfg(feature = "ddi-lab")] lab_boot: ota::LabBoot,
) -> anyhow::Result<()> {
    log_heap("at boot");

    // `boot` owns the config strings; `wifi_config()` / `mqtt_config()` borrow them
    // and are valid as long as `boot` lives (it outlives both uses below).
    //
    // Non-blocking: the ring belongs to the clock app, which shows the rainbow until
    // the first tick, and the link supervisor reconnects forever. A Wi-Fi failure no
    // longer reboot-loops after a timeout; the BOOT portal is the recovery path.
    // Lab image: the DDI runtime is built before Wi-Fi (it validates every setting
    // first) and started once the link is up; the hooks carry the nudge intake, which
    // never publishes (ADR 005).
    #[cfg(feature = "ddi-lab")]
    let lab = {
        let token = ota::token(boot.config())?;
        ota::Lab::prepare(lab_boot, FIRMWARE_VERSION, boot.device_name(), token)?
    };

    let wifi = WiFiManager::new_without_led(
        modem,
        sys_loop,
        Some(nvs),
        boot.wifi_config().connect_nonblocking(),
    )?;
    #[cfg(feature = "ddi-lab")]
    lab.watch_link(&wifi);

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
    // non-tick topic (the DDI nudge) to `Hooks::other_message`.
    let builder = MqttBuilder::new(mqtt_config).subscribe(MQTT_TOPIC, QoS::AtLeastOnce);
    #[cfg(feature = "ddi-lab")]
    let builder = builder.subscribe(lab.nudge_topic(), QoS::AtMostOnce);
    let mqtt = builder.build(&wifi)?;

    // The shared clock app runs on its own thread and owns `wifi` (dropping it would
    // disconnect) and the ring. In the lab image the health policy stays on the main thread.
    #[cfg(feature = "ddi-lab")]
    let hooks = lab.hooks();
    #[cfg(not(feature = "ddi-lab"))]
    let hooks = ();
    let app_mqtt = mqtt.clone();
    let mut ring = clock;
    std::thread::Builder::new()
        .name("clock-app".into())
        .stack_size(CLOCK_APP_STACK_SIZE)
        .spawn(move || {
            // build.rs rejects a bad CLOCK_HANDS, so the fallback to sharp is unreachable.
            let mode = parse_hands(option_env!("CLOCK_HANDS")).unwrap_or_default();
            let cfg = ClockAppConfig {
                tick_topic: MQTT_TOPIC,
                colors: ClockColors::default(),
                brightness: mode.default_brightness(),
                mode,
                diagnostics: Some(app_diagnostics),
            };
            esp_idf_hal::task::block_on(clock_app::run(&wifi, &app_mqtt, &mut ring, hooks, cfg));
        })
        .context("failed to spawn the clock-app thread")?;

    // A failed start of the DDI runtime is a startup error (see `main`).
    #[cfg(feature = "ddi-lab")]
    let _ota = lab.run(&mqtt)?;
    #[cfg(not(feature = "ddi-lab"))]
    log::info!("Setup complete");

    log::info!("Parking main thread");
    // Park the main thread indefinitely — runtime work runs on background threads:
    // the clock app, the MQTT event loop, and in the lab image the DDI worker.
    // In a loop: `park` may return spuriously, and returning would drop the DDI handle.
    loop {
        std::thread::park();
    }
}

/// The re-entry hook: watches BOOT (GPIO9, active low) for `BOOT_WINDOW_MS` after the
/// prompt. A normal boot waits the window; a press then blocks while BOOT is held.
///
/// GPIO9 is a strapping pin: press BOOT after releasing RESET, because holding it
/// through reset enters ROM download mode. The hook cannot fail the boot: a pin that
/// cannot be configured is logged and answers `None`. The `PinDriver` is dropped on
/// return, releasing GPIO9.
fn boot_button_reentry(pin: impl InputPin) -> ReentryRequest {
    let button = match PinDriver::input(pin, Pull::Up) {
        Ok(button) => button,
        Err(e) => {
            log::warn!("BOOT button unavailable ({e}); skipping the re-entry check");
            return ReentryRequest::None;
        }
    };

    let window = Duration::from_millis(BOOT_WINDOW_MS);
    let window_start = Instant::now();
    loop {
        if button.is_low() {
            FreeRtos::delay_ms(BOOT_DEBOUNCE_MS);
            if button.is_low() {
                break;
            }
        }
        if window_start.elapsed() >= window {
            return ReentryRequest::None;
        }
        FreeRtos::delay_ms(BOOT_POLL_MS);
    }

    log::info!(
        "BOOT held — release before {BOOT_HOLD_MS} ms to reopen the portal, keep holding to factory-reset"
    );
    let hold = Duration::from_millis(BOOT_HOLD_MS);
    let pressed_at = Instant::now();
    while button.is_low() && pressed_at.elapsed() < hold {
        FreeRtos::delay_ms(BOOT_POLL_MS);
    }

    // Decide from the loop's exit reason: still low means the hold ran out.
    if button.is_low() {
        log::info!("BOOT held for {BOOT_HOLD_MS} ms — factory reset requested");
        ReentryRequest::FactoryReset
    } else {
        log::info!("BOOT released early — portal requested");
        ReentryRequest::Portal
    }
}

/// Runs the SoftAP captive portal until a terminal outcome, then restarts.
///
/// A `ddi-lab` image that is pending verification rolls back before the portal opens.
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
    // A pending update must not wait in the portal: it rolls back to the previous image.
    #[cfg(feature = "ddi-lab")]
    ota::roll_back_if_pending_before_portal();

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
            device_name: DeviceName::MacSuffixed(DEVICE_NAME),
            firmware_version: FIRMWARE_VERSION,
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
