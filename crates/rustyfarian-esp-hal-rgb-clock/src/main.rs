//! Bare-metal (esp-hal) RGB clock for the ESP32-C3.
//!
//! Boots, logs over UART and drives the 12-LED WS2812 ring (DIN on GPIO4).
//! An unprovisioned board opens the SoftAP captive portal (`Rustyfarian-XXXX`) and
//! reboots after a commit; a provisioned board joins Wi-Fi and MQTT and runs the shared
//! `clock-app`: a rainbow until the first valid tick, then the clock face.
//! The scheduler is started once, after the BOOT window and the store load (ADR 021).
//!
//! Re-entry (upstream `provisioning-reentry-hook-v1`): BOOT (GPIO9) is polled for
//! `BOOT_WINDOW_MS` after the boot prompt. A short press reopens the portal and keeps
//! the stored record; a hold of `BOOT_HOLD_MS` erases the record, and both continue
//! into the portal.
//! Press BOOT after releasing RESET: holding it through reset enters ROM download mode.

#![no_std]
#![no_main]

use clock_app::{ClockAppConfig, RingPort};
use core::convert::Infallible;
use core::net::Ipv4Addr;
use core::str::FromStr;
use embassy_executor::Spawner;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use embassy_time::{Duration, Ticker, Timer};
use esp_backtrace as _;
use esp_hal::{
    gpio::{Input, InputConfig, Level, Pull},
    peripherals::WIFI,
    rmt::{Rmt, TxChannelConfig, TxChannelCreator},
    rng::Rng,
    time::Rate,
    timer::timg::TimerGroup,
};
use esp_radio::wifi::Interface;
use esp_storage::FlashStorage;
use rgb::RGB8;
use rustyfarian_esp_hal_network::mqtt::{MqttBuilder, MqttConfig, MqttRunner, QoS};
use rustyfarian_esp_hal_network::provisioning::{
    gate_reentry, resolve_softap_ssid, DeviceName, PortalConfig, PortalDefaults,
    ProvisioningBuilder, ProvisioningConfig, ProvisioningEvent, ProvisioningOutcome,
    ProvisioningStore, ReentryGate, ReentryRequest, SchemaProfile,
};
use rustyfarian_esp_hal_network::wifi::{
    link_task, AsyncWifiHandle, WiFiConfig, WiFiConfigExt, WiFiManager, WifiLink, WifiPowerSave,
};
use rustyfarian_esp_hal_ws2812::{buffer_size, Ws2812RmtBlocking, RMT_CLK_DIV};
use static_cell::StaticCell;

esp_bootloader_esp_idf::esp_app_desc!();

const LED_COUNT: usize = clock_pure::LED_COUNT;
const RMT_BUFFER: usize = buffer_size(LED_COUNT);

/// Topic carrying the ticks; resolved by `build.rs` (`TICK_TOPIC_OVERRIDE`).
const TICK_TOPIC: &str = env!("TICK_TOPIC");
/// Longest string copied into static storage for the MQTT config.
const MQTT_STR_MAX: usize = 64;

/// SoftAP SSID prefix; the AP MAC's last two bytes are appended as `Rustyfarian-XXXX`.
const AP_SSID_PREFIX: &str = "Rustyfarian";
/// Device name shown on the portal header.
const DEVICE_NAME: &str = "rgb-clock";
/// Single source for the AP channel: consumed by `PortalConfig`.
const AP_CHANNEL: u8 = 1;
/// MQTT broker port prefilled in the portal when `MQTT_PORT` is not supplied at build time.
const DEFAULT_MQTT_PORT: &str = "1883";

/// Provisioning store: the NVS partition of espflash's default table (24 KiB).
/// `espflash flash` does not erase it, so credentials survive a reflash; the hal image
/// never uses NVS otherwise. See docs/features/esp-hal-clock-v1.md.
const STORE_OFFSET: u32 = 0x9000;
const STORE_SIZE: u32 = 0x6000;

/// Heap for the Wi-Fi radio buffers and the provisioning session.
const HEAP_SIZE: usize = 72 * 1024;

/// Gives the phone time to receive the "saved" page before the radio goes down.
const COMMIT_FLUSH_MS: u64 = 1500;
/// Lets the UART drain the last log line before a software reset (bug 004); ~11 ms at 115200 baud.
const LOG_DRAIN_MS: u64 = 50;
/// Pause before rebooting on a possibly transient error, so the log can be read.
const REBOOT_DELAY_MS: u32 = 3000;
/// Holding BOOT this long answers `FactoryReset`; a shorter press answers `Portal`.
const BOOT_HOLD_MS: u32 = 3000;
/// How long after the prompt a BOOT press is accepted; every normal boot waits this long.
const BOOT_WINDOW_MS: u32 = 2000;
/// A low read is confirmed after this delay, so one noisy read does not open the portal.
const BOOT_DEBOUNCE_MS: u32 = 20;
/// Poll interval for the BOOT window and while BOOT is held.
const BOOT_POLL_MS: u32 = 20;

// Portal prefill values are baked at build time from `.env`; `build.rs` guards the port.
// Passwords are never prefilled.
const PREFILL_WIFI_SSID: &str = match option_env!("WIFI_SSID") {
    Some(v) => v,
    None => "",
};
const PREFILL_MQTT_HOST: &str = match option_env!("MQTT_HOST") {
    Some(v) => v,
    None => "",
};
const PREFILL_MQTT_USER: &str = match option_env!("MQTT_USER") {
    Some(v) => v,
    None => "",
};
// Port and client id fall back through `clock_pure::prefill_or` in `run_portal`: it trims,
// as `build.rs` does when it guards them, so a padded value cannot pass the guard yet ship padded.
const PREFILL_MQTT_PORT_RAW: Option<&str> = option_env!("MQTT_PORT");
const PREFILL_MQTT_CLIENT_RAW: Option<&str> = option_env!("MQTT_CLIENT_ID");

/// What the ring shows; switched between frames by `ring_task`.
#[derive(Clone, Copy)]
enum RingMode {
    Rainbow,
    Pairing,
    Connecting,
    /// One finished frame from the shared clock app.
    Frame([RGB8; LED_COUNT]),
}

static RING_MODE: Signal<CriticalSectionRawMutex, RingMode> = Signal::new();

type Ring = Ws2812RmtBlocking<'static, RMT_BUFFER>;

/// Pushes a frame to the ring; driver errors are logged, never fatal.
/// The default `LongFramePolicy::CriticalSection` masks this multi-LED frame, so interrupts
/// (Wi-Fi above all) cannot delay the RMT refill past its ~30 µs deadline (bug 002).
fn show(ring: &mut Ring, pixels: &[RGB8; LED_COUNT]) {
    if let Err(e) = ring.set_pixels_slice(pixels.as_slice()) {
        log::warn!("LED update failed: {:?}", e);
    }
}

/// Runs the rainbow until another mode is requested.
async fn run_rainbow(ring: &mut Ring) -> RingMode {
    let Some(mut rainbow) = clock_app::rainbow_effect() else {
        log::warn!("Rainbow setup failed");
        return RING_MODE.wait().await;
    };
    let mut buffer = [RGB8::default(); LED_COUNT];
    let mut ticker = Ticker::every(Duration::from_millis(u64::from(
        clock_pure::RAINBOW_FRAME_DELAY_MS,
    )));
    loop {
        if let Some(next) = RING_MODE.try_take() {
            return next;
        }
        if let Err(e) = rainbow.update(&mut buffer) {
            log::warn!("Rainbow update failed: {:?}", e);
            return RING_MODE.wait().await;
        }
        show(ring, &buffer);
        ticker.next().await;
    }
}

/// Runs the amber provisioning pulse until another mode is requested.
async fn run_pairing(ring: &mut Ring) -> RingMode {
    let Some(mut pulse) = clock_app::pulse_effect() else {
        log::warn!("Provisioning pulse setup failed");
        return RING_MODE.wait().await;
    };
    let mut buffer = [RGB8::default(); LED_COUNT];
    let mut ticker = Ticker::every(Duration::from_millis(u64::from(
        clock_pure::PROVISION_FRAME_DELAY_MS,
    )));
    loop {
        if let Some(next) = RING_MODE.try_take() {
            return next;
        }
        if let Err(e) = pulse.update(&mut buffer) {
            log::warn!("Provisioning pulse update failed: {:?}", e);
            return RING_MODE.wait().await;
        }
        show(ring, &buffer);
        ticker.next().await;
    }
}

/// The shared clock app's ring: hands each frame to `ring_task`, which owns the driver.
struct HalRing;

impl RingPort for HalRing {
    type Error = Infallible;

    fn show(&mut self, pixels: &[RGB8; LED_COUNT]) -> Result<(), Self::Error> {
        RING_MODE.signal(RingMode::Frame(*pixels));
        Ok(())
    }
}

/// Owns the ring; the mode switch happens between frames.
#[embassy_executor::task]
async fn ring_task(mut ring: Ring) -> ! {
    let mut mode = RingMode::Rainbow;
    loop {
        mode = match mode {
            RingMode::Rainbow | RingMode::Connecting => run_rainbow(&mut ring).await,
            RingMode::Pairing => run_pairing(&mut ring).await,
            RingMode::Frame(pixels) => {
                show(&mut ring, &pixels);
                RING_MODE.wait().await
            }
        };
    }
}

static LINK: WifiLink = WifiLink::new();

#[embassy_executor::task]
async fn net_task(mut runner: embassy_net::Runner<'static, Interface>) -> ! {
    runner.run().await
}

#[embassy_executor::task]
async fn mqtt_task(runner: MqttRunner) -> ! {
    runner.run().await
}

/// The app's diagnostics hook: free heap on link-up and after the first tick.
fn diagnostics(when: &str) {
    log::info!("Free heap at {}: {} bytes", when, esp_alloc::HEAP.free());
}

/// Copies `value` into static storage; `None` when it does not fit.
fn static_str<const N: usize>(
    cell: &'static StaticCell<heapless::String<N>>,
    value: &str,
) -> Option<&'static str> {
    let copy = heapless::String::<N>::try_from(value).ok()?;
    Some(cell.init(copy).as_str())
}

/// Lifecycle callback; runs on the portal's HTTP task, so it only logs.
fn on_provisioning_event(event: ProvisioningEvent) {
    log::info!("Provisioning event: {:?}", event);
}

/// Spins forever. Only for errors that repeat on every boot (bad store geometry),
/// where a reboot loop would add nothing.
fn halt() -> ! {
    loop {
        core::hint::spin_loop();
    }
}

/// Recovery for errors that may be transient: wait, then reboot like the ESP-IDF tier does.
/// Busy-waits because it must work before and after the scheduler starts.
fn reboot_after_error() -> ! {
    esp_hal::delay::Delay::new().delay_millis(REBOOT_DELAY_MS);
    esp_hal::system::software_reset()
}

/// Polls BOOT for up to `BOOT_WINDOW_MS`; true once a low read survives the debounce.
/// A bounced read keeps polling, and its debounce counts toward the window.
fn wait_for_press(button: &Input<'_>, delay: &esp_hal::delay::Delay) -> bool {
    let mut waited_ms = 0;
    while waited_ms < BOOT_WINDOW_MS {
        if button.is_low() {
            delay.delay_millis(BOOT_DEBOUNCE_MS);
            if button.is_low() {
                return true;
            }
            waited_ms += BOOT_DEBOUNCE_MS;
        }
        delay.delay_millis(BOOT_POLL_MS);
        waited_ms += BOOT_POLL_MS;
    }
    false
}

/// The re-entry hook: polls BOOT (GPIO9, active low) for `BOOT_WINDOW_MS`, because the
/// hal image reaches it ~250 ms after reset, too early for upstream's single read in
/// `idf_c3_provision_mqtt`. A normal boot waits the window; a press then blocks while
/// BOOT is held. Busy-waits: the scheduler is not running yet.
fn boot_button_reentry(pin: esp_hal::peripherals::GPIO9<'static>) -> ReentryRequest {
    let button = Input::new(pin, InputConfig::default().with_pull(Pull::Up));
    let delay = esp_hal::delay::Delay::new();

    if !wait_for_press(&button, &delay) {
        return ReentryRequest::None;
    }

    log::info!(
        "BOOT held — release before {} ms to reopen the portal, keep holding to factory-reset",
        BOOT_HOLD_MS
    );
    let mut held_ms = 0;
    while button.is_low() && held_ms < BOOT_HOLD_MS {
        delay.delay_millis(BOOT_POLL_MS);
        held_ms += BOOT_POLL_MS;
    }

    // Decide from the loop's exit reason: still low means the hold ran out.
    if button.is_low() {
        log::info!(
            "BOOT held for {} ms — factory reset requested",
            BOOT_HOLD_MS
        );
        ReentryRequest::FactoryReset
    } else {
        log::info!("BOOT released early — portal requested");
        ReentryRequest::Portal
    }
}

#[esp_rtos::main]
async fn main(spawner: Spawner) {
    esp_println::logger::init_logger(log::LevelFilter::Info);
    let peripherals = esp_hal::init(esp_hal::Config::default());
    esp_alloc::heap_allocator!(size: HEAP_SIZE);

    log::info!(
        "rustyfarian-esp-hal-rgb-clock {} (esp-hal, ESP32-C3, WS2812 DIN GPIO4, {} LEDs)",
        env!("CARGO_PKG_VERSION"),
        LED_COUNT
    );

    let flash = FlashStorage::new(peripherals.FLASH);
    let mut store = match ProvisioningStore::open(flash, STORE_OFFSET, STORE_SIZE) {
        Ok(s) => s,
        Err(e) => {
            log::error!("Failed to open the provisioning store: {:?}", e);
            halt();
        }
    };
    // The hook runs before the store is read, so a re-entry answer never loads the record.
    // `load` alone: the config is ~1.3 KiB, so it is moved, never cloned.
    log::info!(
        "Re-entry: press BOOT within {} ms to reopen the portal (hold {} ms to factory-reset)",
        BOOT_WINDOW_MS,
        BOOT_HOLD_MS
    );
    let gate = match gate_reentry(|| boot_button_reentry(peripherals.GPIO9), || store.load()) {
        Ok(gate) => gate,
        Err(e) => {
            log::error!("Failed to read the provisioning store: {:?}", e);
            reboot_after_error();
        }
    };
    let provisioned = match gate {
        ReentryGate::Proceed(Some(cfg)) if cfg.profile() == SchemaProfile::WifiMqttDevice => {
            Some(cfg)
        }
        ReentryGate::Proceed(Some(cfg)) => {
            log::warn!(
                "Provisioned under {:?}, not WifiMqttDevice — entering SoftAP provisioning",
                cfg.profile()
            );
            None
        }
        ReentryGate::Proceed(None) => {
            log::info!("Not provisioned — entering SoftAP provisioning");
            None
        }
        ReentryGate::Portal => {
            log::info!("Re-entry: portal requested — the stored record is kept");
            None
        }
        ReentryGate::FactoryReset => {
            // The library never erases; the consumer does, before the store moves into the portal.
            match store.erase_all() {
                Ok(()) => log::info!("Re-entry: stored record erased"),
                Err(e) => log::warn!(
                    "Re-entry: erase failed ({:?}); continuing into the portal",
                    e
                ),
            }
            None
        }
    };

    let rmt = Rmt::new(peripherals.RMT, Rate::from_mhz(80)).expect("RMT init");
    let tx_config = TxChannelConfig::default()
        .with_clk_divider(RMT_CLK_DIV)
        .with_idle_output_level(Level::Low)
        .with_idle_output(true)
        .with_carrier_modulation(false);
    let channel = rmt
        .channel0
        .configure_tx(&tx_config)
        .expect("RMT TX channel")
        .with_pin(peripherals.GPIO4);
    let ring: Ring = Ws2812RmtBlocking::new(channel);

    // The scheduler starts once per boot, here, after the BOOT window and the store load;
    // the Wi-Fi and portal entry points below are the `scheduler_running` variants (ADR 021).
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, peripherals.FROM_CPU_INTR0);
    spawner.spawn(ring_task(ring).expect("ring task"));

    match provisioned {
        Some(cfg) => {
            log::info!("Provisioned (wifi_mqtt) — booting clock");
            let wifi_config = WiFiConfig::new(cfg.wifi_ssid(), cfg.wifi_password())
                .with_power_save(WifiPowerSave::MinModem)
                .with_wifi_scheduler_running(peripherals.WIFI);
            let handle = match WiFiManager::init_async(wifi_config) {
                Ok(h) => h,
                Err(e) => {
                    log::error!("Wi-Fi init failed: {}", e);
                    reboot_after_error();
                }
            };
            run_station(spawner, handle, &cfg).await;
        }
        None => {
            RING_MODE.signal(RingMode::Pairing);
            run_portal(spawner, peripherals.WIFI, store).await;
        }
    }
}

/// Parks the task forever; the ring keeps whatever `ring_task` shows.
async fn park() -> ! {
    core::future::pending().await
}

/// Builds the MQTT config from static copies of the provisioned strings.
///
/// Logs lengths only and returns `None` when the broker cannot be used: a missing or
/// oversized field, or a host that is not an IPv4 literal (no DNS on this tier yet).
fn mqtt_config(cfg: &ProvisioningConfig) -> Option<MqttConfig<'static>> {
    static HOST: StaticCell<heapless::String<MQTT_STR_MAX>> = StaticCell::new();
    static CLIENT: StaticCell<heapless::String<MQTT_STR_MAX>> = StaticCell::new();
    static USER: StaticCell<heapless::String<MQTT_STR_MAX>> = StaticCell::new();
    static PASS: StaticCell<heapless::String<MQTT_STR_MAX>> = StaticCell::new();

    let Some(mqtt) = cfg.mqtt() else {
        log::error!("Provisioned record has no MQTT fields");
        return None;
    };
    let host = mqtt.host().trim();
    log::info!(
        "MQTT target: host len={}, port={}, client_id len={}",
        host.len(),
        mqtt.port(),
        mqtt.client_id().map_or(0, str::len)
    );
    if Ipv4Addr::from_str(host).is_err() {
        log::error!(
            "MQTT host (len={}) is not an IPv4 literal; the esp-hal tier has no DNS yet. \
             Keeping the rainbow - press BOOT after a reset and enter the broker's IPv4 address",
            host.len()
        );
        return None;
    }
    let client_id = clock_pure::prefill_or(mqtt.client_id(), DEVICE_NAME);
    let (Some(host), Some(client_id)) = (static_str(&HOST, host), static_str(&CLIENT, client_id))
    else {
        log::error!("MQTT host or client id does not fit {MQTT_STR_MAX} bytes");
        return None;
    };
    let mut config = MqttConfig::new(host, mqtt.port(), client_id);
    if let Some(user) = mqtt.username() {
        let Some(user) = static_str(&USER, user) else {
            log::error!("MQTT username does not fit {MQTT_STR_MAX} bytes");
            return None;
        };
        config = match mqtt.password() {
            Some(pass) => {
                let Some(pass) = static_str(&PASS, pass) else {
                    log::error!("MQTT password does not fit {MQTT_STR_MAX} bytes");
                    return None;
                };
                config.with_auth(user, pass)
            }
            None => config.with_username_only(user),
        };
    }
    Some(config)
}

/// Provisioned path: join Wi-Fi, then hand the link, MQTT and the ring to the clock app.
///
/// The ring already shows the rainbow (`ring_task`'s default), which also stays when the
/// broker config is unusable.
async fn run_station(spawner: Spawner, handle: AsyncWifiHandle, cfg: &ProvisioningConfig) {
    let AsyncWifiHandle {
        controller,
        stack,
        runner,
    } = handle;
    spawner.spawn(net_task(runner).expect("net task"));
    spawner.spawn(link_task(&LINK, controller, stack).expect("link task"));

    log::info!(
        "Free heap after Wi-Fi bring-up: {} of {} bytes",
        esp_alloc::HEAP.free(),
        HEAP_SIZE
    );

    let Some(config) = mqtt_config(cfg) else {
        park().await
    };
    let (mqtt, mqtt_runner) = match MqttBuilder::new(config)
        .subscribe(TICK_TOPIC, QoS::AtLeastOnce)
        .build(stack)
    {
        Ok(pair) => pair,
        Err(e) => {
            log::error!("MQTT build failed: {}", e);
            park().await
        }
    };
    spawner.spawn(mqtt_task(mqtt_runner).expect("mqtt task"));
    log::info!("Subscribed to tick topic {:?}", TICK_TOPIC);

    let app_config = ClockAppConfig {
        tick_topic: TICK_TOPIC,
        colors: clock_pure::ClockColors::default(),
        brightness: clock_pure::DEFAULT_BRIGHTNESS,
        diagnostics: Some(diagnostics),
    };
    clock_app::run(&LINK, &mqtt, &mut HalRing, (), app_config).await
}

/// Portal path: serve the captive portal until the session ends, then reboot.
async fn run_portal(
    spawner: Spawner,
    wifi: WIFI<'static>,
    store: ProvisioningStore<FlashStorage<'static>>,
) {
    let portal_config = PortalConfig {
        ssid_prefix: AP_SSID_PREFIX,
        ssid_override: None,
        ap_password: None,
        channel: AP_CHANNEL,
        device_name: DeviceName::Fixed(DEVICE_NAME),
        firmware_version: env!("CARGO_PKG_VERSION"),
        profile: SchemaProfile::WifiMqttDevice,
        defaults: PortalDefaults {
            wifi_ssid: PREFILL_WIFI_SSID,
            mqtt_host: PREFILL_MQTT_HOST,
            mqtt_port: clock_pure::prefill_or(PREFILL_MQTT_PORT_RAW, DEFAULT_MQTT_PORT),
            mqtt_user: PREFILL_MQTT_USER,
            mqtt_client: clock_pure::prefill_or(PREFILL_MQTT_CLIENT_RAW, DEVICE_NAME),
            ..PortalDefaults::default()
        },
    };
    let session = match ProvisioningBuilder::new(portal_config)
        .on_event(on_provisioning_event)
        .start_scheduler_running(spawner, wifi, store, Rng::new())
    {
        Ok(s) => s,
        Err(e) => {
            log::error!("Provisioning start failed: {:?}", e);
            reboot_after_error();
        }
    };

    let ip = session.ap_ip();
    // `start()` already rejected an invalid SSID; this re-derives it for the log line only.
    let ssid = resolve_softap_ssid(None, AP_SSID_PREFIX, &session.ap_mac());
    log::info!(
        "Provisioning AP \"{}\" up — open http://{}.{}.{}.{}/ to configure",
        ssid.as_ref().map_or("?", |s| s.as_str()),
        ip[0],
        ip[1],
        ip[2],
        ip[3]
    );
    log::info!(
        "Free heap after portal bring-up: {} of {} bytes",
        esp_alloc::HEAP.free(),
        HEAP_SIZE
    );

    match session.wait_outcome().await {
        ProvisioningOutcome::Committed(cfg) => {
            log::info!(
                "Credentials committed (ssid len={}, profile={:?})",
                cfg.wifi_ssid().len(),
                cfg.profile()
            );
            RING_MODE.signal(RingMode::Connecting);
            Timer::after_millis(COMMIT_FLUSH_MS).await;
            log::info!("Provisioning committed — restarting into normal boot");
        }
        ProvisioningOutcome::FactoryResetRequested => {
            // The store moved into the session, so it cannot be erased from here.
            log::warn!("Factory reset requested but the hal session offers no erase — restarting");
        }
        _ => log::warn!("Provisioning ended without a commit — restarting"),
    }
    Timer::after_millis(LOG_DRAIN_MS).await;
    esp_hal::system::software_reset()
}
