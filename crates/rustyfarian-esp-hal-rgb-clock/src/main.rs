//! Bare-metal (esp-hal) RGB clock for the ESP32-C3.
//!
//! M2: boots, logs over UART and drives the 12-LED WS2812 ring (DIN on GPIO4).
//! An unprovisioned board opens the SoftAP captive portal (`Rustyfarian-XXXX`) and
//! reboots after a commit; a provisioned board joins Wi-Fi and shows a fixed time
//! rendered through `clock-pure`. MQTT time follows in M3.
//!
//! Re-entry (upstream `provisioning-reentry-hook-v1`): BOOT (GPIO9) is polled for
//! `BOOT_WINDOW_MS` after the boot prompt. A short press reopens the portal and keeps
//! the stored record; a hold of `BOOT_HOLD_MS` erases the record, and both continue
//! into the portal.
//! Press BOOT after releasing RESET: holding it through reset enters ROM download mode.

#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use embassy_time::{with_timeout, Duration, Ticker, Timer};
use esp_backtrace as _;
use esp_hal::{
    gpio::{Input, InputConfig, Level, Pull},
    peripherals::{FROM_CPU_INTR0, TIMG0, WIFI},
    rmt::{Rmt, TxChannelConfig, TxChannelCreator},
    rng::Rng,
    time::Rate,
};
use esp_radio::wifi::{Interface, WifiController};
use esp_storage::FlashStorage;
use ferriswheel::{Direction, PulseEffect, RainbowEffect};
use rgb::RGB8;
use rustyfarian_esp_hal_network::provisioning::{
    gate_reentry, resolve_softap_ssid, DeviceName, PortalConfig, PortalDefaults,
    ProvisioningBuilder, ProvisioningEvent, ProvisioningOutcome, ProvisioningStore, ReentryGate,
    ReentryRequest, SchemaProfile,
};
use rustyfarian_esp_hal_network::wifi::{
    AsyncWifiHandle, WiFiConfig, WiFiConfigExt, WiFiManager, WifiPowerSave,
};
use rustyfarian_esp_hal_ws2812::{buffer_size, Ws2812RmtBlocking, RMT_CLK_DIV};

esp_bootloader_esp_idf::esp_app_desc!();

const LED_COUNT: usize = clock_pure::LED_COUNT;
const RMT_BUFFER: usize = buffer_size(LED_COUNT);

const RAINBOW_SPEED: u8 = 3;
const RAINBOW_BRIGHTNESS: u8 = 30;
const RAINBOW_FRAME_DELAY_MS: u64 = 30;

// Provisioning ("pairing mode") indicator, same look as the ESP-IDF clock: amber pulse.
const PROVISION_COLOR: RGB8 = RGB8 {
    r: 255,
    g: 120,
    b: 0,
};
const PROVISION_PULSE_SPEED: u8 = 3;
const PROVISION_MIN_BRIGHTNESS: u8 = 5;
const PROVISION_MAX_BRIGHTNESS: u8 = 60;
const PROVISION_FRAME_DELAY_MS: u64 = 30;

// Expected ring: hour blue on LED 11, minute green on LED 5, second red on LED 10 (no overlap).
const FIXED_TIME: (u8, u8, u8) = (12, 34, 56);
const REFRESH_SECS: u64 = 1;

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

const IP_WAIT_SECS: u64 = 10;
const CONNECT_BACKOFF_SECS: u64 = 5;
/// Gives the phone time to receive the "saved" page before the radio goes down.
const COMMIT_FLUSH_MS: u64 = 1500;
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
const PREFILL_MQTT_PORT: &str = match option_env!("MQTT_PORT") {
    Some(v) => {
        let v = v.trim_ascii();
        if v.is_empty() {
            DEFAULT_MQTT_PORT
        } else {
            v
        }
    }
    None => DEFAULT_MQTT_PORT,
};
const PREFILL_MQTT_USER: &str = match option_env!("MQTT_USER") {
    Some(v) => v,
    None => "",
};
// Trimmed like `build.rs` measures it, so a padded id cannot pass its length check.
const PREFILL_MQTT_CLIENT: &str = match option_env!("MQTT_CLIENT_ID") {
    Some(v) => {
        let v = v.trim_ascii();
        if v.is_empty() {
            DEVICE_NAME
        } else {
            v
        }
    }
    None => DEVICE_NAME,
};

/// What the ring shows; switched between frames by `ring_task`.
#[derive(Clone, Copy)]
enum RingMode {
    Rainbow,
    Pairing,
    Connecting,
    Time,
}

static RING_MODE: Signal<CriticalSectionRawMutex, RingMode> = Signal::new();

type Ring = Ws2812RmtBlocking<'static, RMT_BUFFER>;

/// Pushes a frame to the ring; driver errors are logged, never fatal.
/// Blocking inside a critical section: the RMT refills must meet a ~30 µs deadline (bug 002).
fn show(ring: &mut Ring, pixels: &[RGB8; LED_COUNT]) {
    if let Err(e) = critical_section::with(|_| ring.set_pixels_slice(pixels.as_slice())) {
        log::warn!("LED update failed: {:?}", e);
    }
}

/// Runs the rainbow until another mode is requested.
async fn run_rainbow(ring: &mut Ring) -> RingMode {
    let mut rainbow = match RainbowEffect::new(LED_COUNT).and_then(|r| r.with_speed(RAINBOW_SPEED))
    {
        Ok(r) => r
            .with_brightness(RAINBOW_BRIGHTNESS)
            .with_direction(Direction::Clockwise),
        Err(e) => {
            log::warn!("Rainbow setup failed: {:?}", e);
            return RING_MODE.wait().await;
        }
    };
    let mut buffer = [RGB8::default(); LED_COUNT];
    let mut ticker = Ticker::every(Duration::from_millis(RAINBOW_FRAME_DELAY_MS));
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
    let mut pulse = match PulseEffect::new(LED_COUNT).and_then(|p| {
        p.with_color(PROVISION_COLOR)
            .with_speed(PROVISION_PULSE_SPEED)
    }) {
        Ok(p) => p
            .with_min_brightness(PROVISION_MIN_BRIGHTNESS)
            .with_max_brightness(PROVISION_MAX_BRIGHTNESS),
        Err(e) => {
            log::warn!("Provisioning pulse setup failed: {:?}", e);
            return RING_MODE.wait().await;
        }
    };
    let mut buffer = [RGB8::default(); LED_COUNT];
    let mut ticker = Ticker::every(Duration::from_millis(PROVISION_FRAME_DELAY_MS));
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

/// Renders the fixed time through `clock-pure` until another mode is requested.
async fn run_time(ring: &mut Ring) -> RingMode {
    let (hour, minute, second) = FIXED_TIME;
    log::info!(
        "Showing fixed time {:02}:{:02}:{:02} (no time source until M3)",
        hour,
        minute,
        second
    );
    let leds = clock_pure::render_time(
        hour,
        minute,
        second,
        &clock_pure::ClockColors::default(),
        clock_pure::DEFAULT_BRIGHTNESS,
    );
    let pixels = leds.map(|(r, g, b)| RGB8::new(r, g, b));
    let mut ticker = Ticker::every(Duration::from_secs(REFRESH_SECS));
    loop {
        if let Some(next) = RING_MODE.try_take() {
            return next;
        }
        show(ring, &pixels);
        ticker.next().await;
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
            RingMode::Time => run_time(&mut ring).await,
        };
    }
}

/// Association loop: connect, wait for the drop, retry after a back-off.
/// A wrong password retries forever (no portal fallback in the hal tier yet).
#[embassy_executor::task]
async fn wifi_task(mut controller: WifiController<'static>) {
    loop {
        match controller.connect_async().await {
            Ok(_) => {
                log::info!("WiFi associated");
                if let Ok(info) = controller.wait_for_disconnect_async().await {
                    log::warn!("WiFi disconnected: {:?}", info.reason);
                }
            }
            Err(e) => log::warn!("WiFi connect failed: {:?}", e),
        }
        Timer::after_secs(CONNECT_BACKOFF_SECS).await;
    }
}

#[embassy_executor::task]
async fn net_task(mut runner: embassy_net::Runner<'static, Interface>) -> ! {
    runner.run().await
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

    match provisioned {
        Some(cfg) => {
            log::info!("Provisioned (wifi_mqtt) — booting clock");
            if let Some(mqtt) = cfg.mqtt() {
                log::info!(
                    "MQTT target (used from M3): host len={}, port={}, client_id len={}",
                    mqtt.host().len(),
                    mqtt.port(),
                    mqtt.client_id().map_or(0, str::len)
                );
            }
            // The init functions start the scheduler; the ring stays dark until then.
            let wifi_config = WiFiConfig::new(cfg.wifi_ssid(), cfg.wifi_password())
                .with_power_save(WifiPowerSave::MinModem)
                .with_peripherals(
                    peripherals.TIMG0,
                    peripherals.FROM_CPU_INTR0,
                    peripherals.WIFI,
                );
            let handle = match WiFiManager::init_async(wifi_config) {
                Ok(h) => h,
                Err(e) => {
                    log::error!("Wi-Fi init failed: {}", e);
                    reboot_after_error();
                }
            };
            spawner.spawn(ring_task(ring).expect("ring task"));
            RING_MODE.signal(RingMode::Connecting);
            run_station(spawner, handle).await;
        }
        None => {
            spawner.spawn(ring_task(ring).expect("ring task"));
            RING_MODE.signal(RingMode::Pairing);
            run_portal(
                spawner,
                peripherals.TIMG0,
                peripherals.FROM_CPU_INTR0,
                peripherals.WIFI,
                store,
            )
            .await;
        }
    }
}

/// Provisioned path: join Wi-Fi, wait for the IP, then show the fixed time.
async fn run_station(spawner: Spawner, handle: AsyncWifiHandle) {
    let AsyncWifiHandle {
        controller,
        stack,
        runner,
    } = handle;
    spawner.spawn(net_task(runner).expect("net task"));
    spawner.spawn(wifi_task(controller).expect("wifi task"));

    log::info!(
        "Free heap after Wi-Fi bring-up: {} of {} bytes",
        esp_alloc::HEAP.free(),
        HEAP_SIZE
    );
    log::info!("WiFi connecting...");
    let mut link_up =
        match with_timeout(Duration::from_secs(IP_WAIT_SECS), stack.wait_config_up()).await {
            Ok(()) => {
                log_ip(stack);
                true
            }
            Err(_) => {
                // M2 only: mirrors the IDF "continue anyway"; M3 needs the link for MQTT.
                log::error!(
                    "No IP address after {} s — still retrying in the background",
                    IP_WAIT_SECS
                );
                false
            }
        };
    RING_MODE.signal(RingMode::Time);

    // Keeps the task alive and reports every later DHCP lease; a link that never came
    // up is not reported as going down.
    loop {
        if link_up {
            stack.wait_config_down().await;
            log::warn!("WiFi link down");
        }
        stack.wait_config_up().await;
        log_ip(stack);
        link_up = true;
    }
}

fn log_ip(stack: embassy_net::Stack<'static>) {
    match stack.config_v4() {
        Some(v4) => log::info!("Got IP address: {}", v4.address),
        None => log::warn!("IP config reported up but has no IPv4 address"),
    }
}

/// Portal path: serve the captive portal until the session ends, then reboot.
async fn run_portal(
    spawner: Spawner,
    timg0: TIMG0<'static>,
    sw_int: FROM_CPU_INTR0<'static>,
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
            mqtt_port: PREFILL_MQTT_PORT,
            mqtt_user: PREFILL_MQTT_USER,
            mqtt_client: PREFILL_MQTT_CLIENT,
            ..PortalDefaults::default()
        },
    };
    let session = match ProvisioningBuilder::new(portal_config)
        .on_event(on_provisioning_event)
        .start(spawner, timg0, sw_int, wifi, store, Rng::new())
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
    esp_hal::system::software_reset()
}
