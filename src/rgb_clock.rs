use anyhow::Result;
use clock_app::{pulse_effect, RingPort};
use clock_pure::{LED_COUNT, PROVISION_FRAME_DELAY_MS};
use rgb::RGB8;
use rustyfarian_esp_idf_ws2812::Ws2812Rmt;

/// The 12 RGB LEDs of the clock ring (the first LED sits at 1 o'clock), written through
/// the RMT-backed WS2812 driver. Implements the shared app's [`RingPort`]; exactly one
/// owner draws at a time (the app thread, or the provisioning pulse thread).
pub struct IdfRing {
    driver: Ws2812Rmt<'static>,
}

impl IdfRing {
    pub fn new(driver: Ws2812Rmt<'static>) -> Self {
        Self { driver }
    }
}

impl RingPort for IdfRing {
    type Error = anyhow::Error;

    fn show(&mut self, pixels: &[RGB8; LED_COUNT]) -> Result<()> {
        self.driver.set_pixels_slice(pixels.as_slice())?;
        Ok(())
    }
}

/// Runs the provisioning "pairing mode" indicator in a background thread.
///
/// Pulses the full clock ring amber (a `ferriswheel::PulseEffect`) while the
/// SoftAP captive portal waits for credentials, signalling "configure me"
/// without a serial cable. The thread owns the ring and runs until the firmware
/// restarts after the portal outcome.
///
/// # Arguments
/// * `ring` - The clock ring, moved into the thread
///
/// # Returns
/// A join handle for the indicator thread
pub fn run_provisioning_animation(mut ring: IdfRing) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        use esp_idf_hal::delay::FreeRtos;

        log::info!("Starting provisioning indicator (amber pulse)");

        let Some(mut pulse) = pulse_effect() else {
            log::error!("Failed to create provisioning pulse effect");
            return;
        };

        let mut buffer = [RGB8::default(); LED_COUNT];

        loop {
            if let Err(e) = pulse.update(&mut buffer) {
                log::warn!("Provisioning pulse update error: {}", e);
                return;
            }

            if let Err(e) = ring.show(&buffer) {
                log::warn!("Provisioning display error: {:?}", e);
            }

            FreeRtos::delay_ms(PROVISION_FRAME_DELAY_MS);
        }
    })
}
