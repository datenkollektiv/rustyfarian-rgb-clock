//! Bare-metal (esp-hal) RGB clock for the ESP32-C3.
//!
//! M1: boots, logs over UART, plays the startup rainbow on the 12-LED WS2812 ring
//! (DIN on GPIO4) and then shows a fixed time rendered through `clock-pure`.
//! Wi-Fi, provisioning and MQTT follow in later milestones; the embassy + esp-rtos
//! shape is chosen now so they only add spawned tasks.

#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_time::{Duration, Ticker, Timer};
use esp_backtrace as _;
use esp_hal::{
    gpio::Level,
    rmt::{Rmt, TxChannelConfig, TxChannelCreator},
    time::Rate,
    timer::timg::TimerGroup,
};
use ferriswheel::{Direction, RainbowEffect};
use rgb::RGB8;
use rustyfarian_esp_hal_ws2812::{buffer_size, Ws2812Rmt, RMT_CLK_DIV};

esp_bootloader_esp_idf::esp_app_desc!();

const LED_COUNT: usize = clock_pure::LED_COUNT;
const RMT_BUFFER: usize = buffer_size(LED_COUNT);

const RAINBOW_SPEED: u8 = 3;
const RAINBOW_BRIGHTNESS: u8 = 30;
const RAINBOW_FRAME_DELAY_MS: u64 = 30;
const RAINBOW_FRAMES: u32 = 100;

// Expected ring: hour blue on LED 11, minute green on LED 5, second red on LED 10 (no overlap).
const FIXED_TIME: (u8, u8, u8) = (12, 34, 56);
const REFRESH_SECS: u64 = 1;

type Ring<'d> = Ws2812Rmt<'d, esp_hal::Async, RMT_BUFFER>;

/// Pushes a frame to the ring; driver errors are logged, never fatal.
async fn show(ring: &mut Ring<'_>, pixels: &[RGB8; LED_COUNT]) {
    if let Err(e) = ring.set_pixels_slice(pixels.as_slice()).await {
        log::warn!("LED update failed: {:?}", e);
    }
}

/// Plays the rainbow for a fixed number of frames.
async fn startup_rainbow(ring: &mut Ring<'_>) {
    let mut rainbow = match RainbowEffect::new(LED_COUNT).and_then(|r| r.with_speed(RAINBOW_SPEED))
    {
        Ok(r) => r
            .with_brightness(RAINBOW_BRIGHTNESS)
            .with_direction(Direction::Clockwise),
        Err(e) => {
            log::warn!("Rainbow setup failed: {:?}", e);
            return;
        }
    };
    let mut buffer = [RGB8::default(); LED_COUNT];
    let mut ticker = Ticker::every(Duration::from_millis(RAINBOW_FRAME_DELAY_MS));
    for _ in 0..RAINBOW_FRAMES {
        if let Err(e) = rainbow.update(&mut buffer) {
            log::warn!("Rainbow update failed: {:?}", e);
            return;
        }
        show(ring, &buffer).await;
        ticker.next().await;
    }
}

/// Renders a time through `clock-pure` onto the ring.
async fn show_time(ring: &mut Ring<'_>, (hour, minute, second): (u8, u8, u8)) {
    let leds = clock_pure::render_time(
        hour,
        minute,
        second,
        &clock_pure::ClockColors::default(),
        clock_pure::DEFAULT_BRIGHTNESS,
    );
    show(ring, &leds.map(|(r, g, b)| RGB8::new(r, g, b))).await;
}

#[esp_rtos::main]
async fn main(_spawner: Spawner) {
    esp_println::logger::init_logger(log::LevelFilter::Info);
    let peripherals = esp_hal::init(esp_hal::Config::default());

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, peripherals.FROM_CPU_INTR0);

    log::info!(
        "rustyfarian-esp-hal-rgb-clock {} (esp-hal, ESP32-C3, WS2812 DIN GPIO4, {} LEDs)",
        env!("CARGO_PKG_VERSION"),
        LED_COUNT
    );

    let rmt = Rmt::new(peripherals.RMT, Rate::from_mhz(80))
        .expect("RMT init")
        .into_async();
    let config = TxChannelConfig::default()
        .with_clk_divider(RMT_CLK_DIV)
        .with_idle_output_level(Level::Low)
        .with_idle_output(true)
        .with_carrier_modulation(false);
    let channel = rmt
        .channel0
        .configure_tx(&config)
        .expect("RMT TX channel")
        .with_pin(peripherals.GPIO4);
    let mut ring: Ring<'_> = Ws2812Rmt::new(channel);

    log::info!("Starting rainbow startup animation");
    startup_rainbow(&mut ring).await;
    log::info!("Startup rainbow done");

    let (h, m, s) = FIXED_TIME;
    log::info!("Showing fixed time {:02}:{:02}:{:02}", h, m, s);
    show_time(&mut ring, FIXED_TIME).await;
    log::info!("First LED update done");

    loop {
        Timer::after_secs(REFRESH_SECS).await;
        show_time(&mut ring, FIXED_TIME).await;
    }
}
