use anyhow::Result;
use clock_pure::{add_colors, hour_to_index, minute_to_index, scale_color, second_to_index, Rgb};
use ferriswheel::{Direction, PulseEffect, RainbowEffect};
use log::debug;
use rgb::RGB8;
use rustyfarian_esp_idf_ws2812::Ws2812Rmt;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

// Default colors for clock hands
const DEFAULT_HOUR_COLOR: Rgb = (0, 0, 1); // Blue
const DEFAULT_MINUTE_COLOR: Rgb = (0, 1, 0); // Green
const DEFAULT_SECOND_COLOR: Rgb = (1, 0, 0); // Red
const DEFAULT_BRIGHTNESS: u8 = 10;

// Rainbow animation settings
const RAINBOW_SPEED: u8 = 3;
const RAINBOW_BRIGHTNESS: u8 = 30;
const RAINBOW_FRAME_DELAY_MS: u32 = 30;

// Provisioning ("pairing mode") indicator: an amber breathing pulse on the full
// ring. Amber sits outside the clock's blue/green/red hand palette, so it is never
// confused with a running face. See docs/features/wifi-softap-provisioning-v1.md.
const PROVISION_COLOR: RGB8 = RGB8 {
    r: 255,
    g: 120,
    b: 0,
};
const PROVISION_PULSE_SPEED: u8 = 3;
const PROVISION_MIN_BRIGHTNESS: u8 = 5;
const PROVISION_MAX_BRIGHTNESS: u8 = 60;
const PROVISION_FRAME_DELAY_MS: u32 = 30;

/// An RGB LED clock that represents time using 12 RGB LEDs arranged in a circle.
/// Each LED corresponds to an hour position on a traditional clock face.
pub struct RGBClock<'a> {
    hours_base_color: Rgb,
    minutes_base_color: Rgb,
    seconds_base_color: Rgb,
    brightness: u8,
    driver: Ws2812Rmt<'a>,
    state: [Rgb; 12],
}

// The RGBClock is built from 12 RGB LEDs, one for each hour.
// The LEDs are ordered in a circle, with the first LED at 1 o'clock.
impl<'a> RGBClock<'a> {
    /// Creates a new RGB clock with default color settings.
    ///
    /// # Default colors
    /// - Hours: Blue (0, 0, 1)
    /// - Minutes: Green (0, 1, 0)
    /// - Seconds: Red (1, 0, 0)
    pub fn new(driver: Ws2812Rmt<'a>) -> Result<Self> {
        let clock = Self {
            hours_base_color: DEFAULT_HOUR_COLOR,
            minutes_base_color: DEFAULT_MINUTE_COLOR,
            seconds_base_color: DEFAULT_SECOND_COLOR,
            brightness: DEFAULT_BRIGHTNESS,
            driver,
            state: [(0, 0, 0); 12],
        };

        Ok(clock)
    }

    /// Sets the complete time on the clock (hours, minutes, and seconds).
    ///
    /// # Arguments
    /// * `time` - A `LocalTime` struct containing hour, minute, and second values
    pub fn set_local_time(&mut self, time: LocalTime) -> Result<()> {
        self.clear()?;

        let hour_idx = hour_to_index(time.hour);
        let minute_idx = minute_to_index(time.minute);
        let second_idx = second_to_index(time.second);

        // Set state of hour LED
        self.state[hour_idx] = self.hours_base_color;

        // Add minute LED (may overlap with hour)
        self.state[minute_idx] = add_colors(self.state[minute_idx], self.minutes_base_color);

        // Add LED for the seconds (may overlap with hour or minute)
        self.state[second_idx] = add_colors(self.state[second_idx], self.seconds_base_color);

        self.show()
    }

    /// Clears all LEDs by setting them to black (off).
    pub fn clear(&mut self) -> Result<()> {
        self.state = [(0, 0, 0); 12];
        Ok(())
    }

    /// Sets all pixels directly from an RGB8 buffer.
    ///
    /// This bypasses the internal state and writes directly to the LEDs.
    /// Useful for effects like `RainbowEffect` that produce RGB8 buffers.
    pub fn set_pixels(&mut self, pixels: &[RGB8; 12]) -> Result<()> {
        self.driver.set_pixels_slice(pixels.as_slice())?;
        Ok(())
    }

    /// Updates the physical LEDs with the current state.
    pub fn show(&mut self) -> Result<()> {
        let pixels: [RGB8; 12] = self.state.map(|(r, g, b)| {
            let scaled = scale_color((r, g, b), self.brightness);
            RGB8::new(scaled.0, scaled.1, scaled.2)
        });
        debug!("Showing state: {:?}", pixels);
        self.driver.set_pixels_slice(pixels.as_slice())?;
        Ok(())
    }
}

/// Runs a rainbow startup animation in a background thread.
///
/// Uses `RainbowEffect` from `ferriswheel` to create a smooth rainbow
/// animation that rotates around the clock face until cancelled.
/// The animation is cancelled automatically when the first MQTT time
/// message is received (which sets the cancellation flag).
///
/// # Arguments
/// * `clock` - Shared reference to the RGB clock
/// * `cancel` - Shared cancellation flag
///
/// # Returns
/// A join handle for the animation thread
pub fn run_startup_animation(
    clock: Arc<Mutex<RGBClock<'static>>>,
    cancel: Arc<AtomicBool>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        use esp_idf_hal::delay::FreeRtos;

        log::info!("Starting rainbow startup animation");

        let mut rainbow = match RainbowEffect::new(12) {
            Ok(r) => match r.with_speed(RAINBOW_SPEED) {
                Ok(r) => r
                    .with_brightness(RAINBOW_BRIGHTNESS)
                    .with_direction(Direction::Clockwise),
                Err(e) => {
                    log::error!("Failed to set rainbow speed: {}", e);
                    return;
                }
            },
            Err(e) => {
                log::error!("Failed to create rainbow effect: {}", e);
                return;
            }
        };

        let mut buffer = [RGB8::default(); 12];

        loop {
            if cancel.load(Ordering::Relaxed) {
                log::info!("Rainbow animation cancelled");
                return;
            }

            if let Err(e) = rainbow.update(&mut buffer) {
                log::warn!("Rainbow update error: {}", e);
                return;
            }

            match clock.lock() {
                Ok(mut c) => {
                    // Re-check cancellation *under the lock* before writing. The
                    // on_message handler sets `cancel` and renders the first real
                    // clock frame while holding this same mutex. Without this
                    // second check, an animation frame computed before cancellation
                    // could still be flushed here — after the clock's first render —
                    // overwriting it with a trailing rainbow frame for one cycle.
                    if cancel.load(Ordering::Relaxed) {
                        log::info!("Rainbow animation cancelled");
                        return;
                    }
                    if let Err(e) = c.set_pixels(&buffer) {
                        log::warn!("Animation display error: {:?}", e);
                    }
                }
                Err(e) => log::error!("Clock mutex poisoned: {:?}", e),
            }

            FreeRtos::delay_ms(RAINBOW_FRAME_DELAY_MS);
        }
    })
}

/// Runs the provisioning "pairing mode" indicator in a background thread.
///
/// Pulses the full clock ring amber (a `ferriswheel::PulseEffect`) while the
/// SoftAP captive portal waits for credentials, signalling "configure me"
/// without a serial cable. Cancelled the same way as the startup animation:
/// the caller sets `cancel` once provisioning commits, just before restart.
///
/// # Arguments
/// * `clock` - Shared reference to the RGB clock
/// * `cancel` - Shared cancellation flag
///
/// # Returns
/// A join handle for the indicator thread
pub fn run_provisioning_animation(
    clock: Arc<Mutex<RGBClock<'static>>>,
    cancel: Arc<AtomicBool>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        use esp_idf_hal::delay::FreeRtos;

        log::info!("Starting provisioning indicator (amber pulse)");

        let mut pulse = match PulseEffect::new(12) {
            Ok(p) => match p
                .with_color(PROVISION_COLOR)
                .with_speed(PROVISION_PULSE_SPEED)
            {
                Ok(p) => p
                    .with_min_brightness(PROVISION_MIN_BRIGHTNESS)
                    .with_max_brightness(PROVISION_MAX_BRIGHTNESS),
                Err(e) => {
                    log::error!("Failed to set provisioning pulse speed: {}", e);
                    return;
                }
            },
            Err(e) => {
                log::error!("Failed to create provisioning pulse effect: {}", e);
                return;
            }
        };

        let mut buffer = [RGB8::default(); 12];

        loop {
            if cancel.load(Ordering::Relaxed) {
                log::info!("Provisioning indicator cancelled");
                return;
            }

            if let Err(e) = pulse.update(&mut buffer) {
                log::warn!("Provisioning pulse update error: {}", e);
                return;
            }

            match clock.lock() {
                Ok(mut c) => {
                    // Re-check under the lock before writing, mirroring the
                    // startup animation's handoff: the caller cancels on commit
                    // immediately before restart, so never flush a stale frame.
                    if cancel.load(Ordering::Relaxed) {
                        log::info!("Provisioning indicator cancelled");
                        return;
                    }
                    if let Err(e) = c.set_pixels(&buffer) {
                        log::warn!("Provisioning display error: {:?}", e);
                    }
                }
                Err(e) => log::error!("Clock mutex poisoned: {:?}", e),
            }

            FreeRtos::delay_ms(PROVISION_FRAME_DELAY_MS);
        }
    })
}

/// Represents a local time with hour, minute, and second components.
#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct LocalTime {
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

/// Error type for LocalTime conversion failures.
#[derive(Debug)]
pub enum ConvertError {
    /// The provided data is not valid UTF-8
    InvalidUtf8,
    /// The JSON data could not be parsed into a LocalTime
    InvalidJson,
}

impl std::fmt::Display for ConvertError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConvertError::InvalidUtf8 => write!(f, "Invalid UTF-8 in message data"),
            ConvertError::InvalidJson => write!(f, "Failed to parse JSON into LocalTime"),
        }
    }
}

impl std::error::Error for ConvertError {}

impl TryFrom<&[u8]> for LocalTime {
    type Error = ConvertError;

    fn try_from(message: &[u8]) -> Result<Self, Self::Error> {
        let json = std::str::from_utf8(message).map_err(|_| ConvertError::InvalidUtf8)?;
        let local_time: LocalTime =
            serde_json::from_str(json).map_err(|_| ConvertError::InvalidJson)?;
        Ok(local_time)
    }
}
