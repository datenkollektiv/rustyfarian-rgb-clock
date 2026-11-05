//! The clock app shared by the ESP-IDF and esp-hal tiers.
//!
//! A tier supplies the Wi-Fi link and MQTT client (juggler's [`NetworkLink`] and
//! [`MqttClient`]), the LED ring ([`RingPort`]) and optional [`Hooks`]; [`run`] does the
//! rest: a rainbow until the first valid tick, then the clock face on every tick.
//!
//! The app never publishes: all outbound traffic stays with the tier (ADR 005).
#![cfg_attr(not(test), no_std)]

use core::convert::Infallible;
use core::fmt::Debug;
use core::pin::pin;

use clock_pure::{
    parse_tick, render_tick, ClockColors, TickTime, LED_COUNT, PROVISION_COLOR,
    PROVISION_MAX_BRIGHTNESS, PROVISION_MIN_BRIGHTNESS, PROVISION_PULSE_SPEED, RAINBOW_BRIGHTNESS,
    RAINBOW_FRAME_DELAY_MS, RAINBOW_SPEED,
};
use embassy_futures::join::join;
use embassy_futures::select::{select, Either};
use embassy_time::{Duration, Instant, Ticker, Timer};
use ferriswheel::{Direction, PulseEffect, RainbowEffect};
use juggler::mqtt::{MqttClient, MqttMessage};
use juggler::wifi::NetworkLink;
use rgb::RGB8;

/// How often the link watcher samples the link state.
const LINK_POLL_MS: u64 = 1_000;

/// The LED ring the app draws on.
pub trait RingPort {
    /// Error returned when the frame cannot be written.
    type Error: Debug;

    /// Writes one frame to the ring.
    ///
    /// # Errors
    ///
    /// Returns the driver error when the frame cannot be written.
    fn show(&mut self, pixels: &[RGB8; LED_COUNT]) -> Result<(), Self::Error>;
}

/// Tier-specific extension points; every method has a do-nothing default.
pub trait Hooks {
    /// Called once after each valid tick has been drawn.
    fn tick_rendered(&mut self, _at: Instant) {}

    /// Called for every inbound message whose topic is not the tick topic.
    ///
    /// Runs on the app task, never inside an MQTT callback, and must not block.
    /// Returns whether the message was handled.
    fn other_message(&mut self, _topic: &str, _payload: &[u8]) -> bool {
        false
    }
}

impl Hooks for () {}

/// Static configuration of the clock app.
#[derive(Debug, Clone, Copy)]
pub struct ClockAppConfig {
    /// Topic carrying the `{"hour":..,"minute":..,"second":..}` ticks.
    pub tick_topic: &'static str,
    /// Hand colors before brightness scaling.
    pub colors: ClockColors,
    /// Brightness factor applied to the hands.
    pub brightness: u8,
    /// Tier diagnostics (heap, stack), called on the app's task with the moment:
    /// `"link up"` or `"first tick"`.
    pub diagnostics: Option<fn(&str)>,
}

/// Builds the startup rainbow, or `None` when the effect cannot be constructed.
#[must_use]
pub fn rainbow_effect() -> Option<RainbowEffect> {
    let rainbow = RainbowEffect::new(LED_COUNT)
        .and_then(|r| r.with_speed(RAINBOW_SPEED))
        .ok()?;
    Some(
        rainbow
            .with_brightness(RAINBOW_BRIGHTNESS)
            .with_direction(Direction::Clockwise),
    )
}

/// Builds the amber "pairing mode" pulse, or `None` when the effect cannot be constructed.
#[must_use]
pub fn pulse_effect() -> Option<PulseEffect> {
    let pulse = PulseEffect::new(LED_COUNT)
        .and_then(|p| {
            p.with_color(RGB8::new(
                PROVISION_COLOR.0,
                PROVISION_COLOR.1,
                PROVISION_COLOR.2,
            ))
            .with_speed(PROVISION_PULSE_SPEED)
        })
        .ok()?;
    Some(
        pulse
            .with_min_brightness(PROVISION_MIN_BRIGHTNESS)
            .with_max_brightness(PROVISION_MAX_BRIGHTNESS),
    )
}

/// Runs the clock app forever.
///
/// Shows the rainbow until the first valid tick arrives on `cfg.tick_topic`, then draws the
/// clock face for every valid tick. A malformed tick is logged and the frame stays as it
/// was; messages on any other topic go to [`Hooks::other_message`].
pub async fn run<L, M, R, H>(
    link: &L,
    mqtt: &M,
    ring: &mut R,
    mut hooks: H,
    cfg: ClockAppConfig,
) -> !
where
    L: NetworkLink,
    M: MqttClient,
    R: RingPort,
    H: Hooks,
{
    let (never, _) = join(
        link_watch(link, cfg.diagnostics),
        clock_loop(mqtt, ring, &mut hooks, &cfg),
    )
    .await;
    match never {}
}

/// Logs link transitions; the library supervises and reconnects the link itself.
async fn link_watch<L: NetworkLink>(link: &L, diagnostics: Option<fn(&str)>) -> Infallible {
    loop {
        link.wait_connected().await;
        log::info!("Wi-Fi link up (connect #{})", link.connect_count());
        report(diagnostics, "link up");
        while link.is_connected() {
            Timer::after(Duration::from_millis(LINK_POLL_MS)).await;
        }
        log::warn!("Wi-Fi link lost, state {:?}", link.state());
    }
}

async fn clock_loop<M: MqttClient, R: RingPort, H: Hooks>(
    mqtt: &M,
    ring: &mut R,
    hooks: &mut H,
    cfg: &ClockAppConfig,
) -> Infallible {
    let mut buffer = [RGB8::default(); LED_COUNT];
    let mut rainbow = rainbow_effect();
    if rainbow.is_none() {
        log::warn!("Rainbow setup failed, waiting for the first tick");
    }
    let mut ticker = Ticker::every(Duration::from_millis(u64::from(RAINBOW_FRAME_DELAY_MS)));
    if let Some(r) = rainbow.as_mut() {
        rainbow_frame(r, &mut buffer, ring);
    }
    loop {
        let mut receive = pin!(mqtt.receive());
        let message = match rainbow.as_mut() {
            None => receive.await,
            Some(r) => loop {
                match select(receive.as_mut(), ticker.next()).await {
                    Either::First(message) => break message,
                    Either::Second(()) => rainbow_frame(r, &mut buffer, ring),
                }
            },
        };
        let Some(tick) = route(&message, cfg, hooks) else {
            continue;
        };
        if rainbow.take().is_some() {
            log::info!("First tick received, leaving the rainbow");
            report(cfg.diagnostics, "first tick");
        }
        let leds = render_tick(&tick, &cfg.colors, cfg.brightness);
        buffer = leds.map(|(r, g, b)| RGB8::new(r, g, b));
        show(ring, &buffer);
        hooks.tick_rendered(Instant::now());
    }
}

/// Routes one message: ticks are parsed, everything else goes to the hooks.
fn route<G: MqttMessage, H: Hooks>(
    message: &G,
    cfg: &ClockAppConfig,
    hooks: &mut H,
) -> Option<TickTime> {
    if message.topic() != cfg.tick_topic {
        if !hooks.other_message(message.topic(), message.payload()) {
            log::debug!("Unhandled message on {}", message.topic());
        }
        return None;
    }
    match parse_tick(message.payload()) {
        Ok(tick) => Some(tick),
        Err(e) => {
            log::warn!("Ignoring malformed tick: {:?}", e);
            None
        }
    }
}

fn rainbow_frame<R: RingPort>(
    rainbow: &mut RainbowEffect,
    buffer: &mut [RGB8; LED_COUNT],
    ring: &mut R,
) {
    match rainbow.update(buffer) {
        Ok(()) => show(ring, buffer),
        Err(e) => log::warn!("Rainbow update failed: {:?}", e),
    }
}

fn show<R: RingPort>(ring: &mut R, pixels: &[RGB8; LED_COUNT]) {
    if let Err(e) = ring.show(pixels) {
        log::warn!("Ring write failed: {:?}", e);
    }
}

fn report(diagnostics: Option<fn(&str)>, when: &str) {
    if let Some(diagnostics) = diagnostics {
        diagnostics(when);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::cell::RefCell;
    use core::future::Future;
    use core::task::Poll;
    use embassy_futures::poll_once;
    use embassy_time::MockDriver;
    use juggler::mqtt::mock::MockMqttClient;
    use juggler::wifi::mock::MockLink;
    use std::rc::Rc;
    use std::vec::Vec;

    const TICK: &str = "clock/tick";

    static DRIVER_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// The mock time driver is process-global: serialise the tests and restart its clock.
    fn serialised() -> std::sync::MutexGuard<'static, ()> {
        let guard = DRIVER_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        MockDriver::get().reset();
        guard
    }

    #[derive(Default)]
    struct TestRing {
        frames: Rc<RefCell<Vec<[RGB8; LED_COUNT]>>>,
    }

    impl RingPort for TestRing {
        type Error = Infallible;

        fn show(&mut self, pixels: &[RGB8; LED_COUNT]) -> Result<(), Self::Error> {
            self.frames.borrow_mut().push(*pixels);
            Ok(())
        }
    }

    type Other = (std::string::String, Vec<u8>);

    #[derive(Default)]
    struct Recorder {
        rendered: Rc<RefCell<u32>>,
        others: Rc<RefCell<Vec<Other>>>,
    }

    impl Hooks for Recorder {
        fn tick_rendered(&mut self, _at: Instant) {
            *self.rendered.borrow_mut() += 1;
        }

        fn other_message(&mut self, topic: &str, payload: &[u8]) -> bool {
            self.others
                .borrow_mut()
                .push((topic.into(), payload.to_vec()));
            true
        }
    }

    fn cfg() -> ClockAppConfig {
        ClockAppConfig {
            tick_topic: TICK,
            colors: ClockColors::default(),
            brightness: 10,
            diagnostics: None,
        }
    }

    fn expected(h: u8, m: u8, s: u8) -> [RGB8; LED_COUNT] {
        clock_pure::render_time(h, m, s, &ClockColors::default(), 10)
            .map(|(r, g, b)| RGB8::new(r, g, b))
    }

    /// Polls the app once; it must stay pending because it never returns.
    fn poll<F: Future>(app: &mut core::pin::Pin<&mut F>) {
        assert!(matches!(poll_once(app.as_mut()), Poll::Pending));
    }

    #[test]
    fn rainbow_stays_until_a_valid_tick() {
        let _serial = serialised();
        let link = MockLink::new();
        let mqtt = MockMqttClient::new();
        let mut ring = TestRing::default();
        let frames = ring.frames.clone();
        let rec = Recorder::default();
        let rendered = rec.rendered.clone();
        let fut = run(&link, &mqtt, &mut ring, rec, cfg());
        let mut app = pin!(fut);

        poll(&mut app);
        assert_eq!(frames.borrow().len(), 1);

        MockDriver::get().advance(Duration::from_millis(100));
        poll(&mut app);
        assert!(frames.borrow().len() > 1);
        assert_eq!(*rendered.borrow(), 0);

        mqtt.deliver(TICK, br#"{"hour":3,"minute":15,"second":30}"#);
        poll(&mut app);
        assert_eq!(*frames.borrow().last().unwrap(), expected(3, 15, 30));
        assert_eq!(*rendered.borrow(), 1);

        let shown = frames.borrow().len();
        MockDriver::get().advance(Duration::from_millis(100));
        poll(&mut app);
        assert_eq!(frames.borrow().len(), shown, "rainbow must be gone");
    }

    #[test]
    fn malformed_tick_keeps_the_frame() {
        let _serial = serialised();
        let link = MockLink::new();
        let mqtt = MockMqttClient::new();
        let mut ring = TestRing::default();
        let frames = ring.frames.clone();
        let rec = Recorder::default();
        let rendered = rec.rendered.clone();
        let fut = run(&link, &mqtt, &mut ring, rec, cfg());
        let mut app = pin!(fut);

        mqtt.deliver(TICK, br#"{"hour":1,"minute":2,"second":3}"#);
        poll(&mut app);
        let shown = frames.borrow().len();

        mqtt.deliver(TICK, br#"{"hour":24,"minute":0,"second":0}"#);
        mqtt.deliver(TICK, b"not json");
        poll(&mut app);
        assert_eq!(frames.borrow().len(), shown);
        assert_eq!(*frames.borrow().last().unwrap(), expected(1, 2, 3));
        assert_eq!(*rendered.borrow(), 1);
    }

    #[test]
    fn malformed_first_tick_keeps_the_rainbow() {
        let _serial = serialised();
        let link = MockLink::new();
        let mqtt = MockMqttClient::new();
        let mut ring = TestRing::default();
        let frames = ring.frames.clone();
        let rec = Recorder::default();
        let rendered = rec.rendered.clone();
        let fut = run(&link, &mqtt, &mut ring, rec, cfg());
        let mut app = pin!(fut);

        mqtt.deliver(TICK, b"{}");
        poll(&mut app);
        let shown = frames.borrow().len();
        MockDriver::get().advance(Duration::from_millis(100));
        poll(&mut app);
        assert!(frames.borrow().len() > shown, "rainbow keeps animating");
        assert_eq!(*rendered.borrow(), 0);
    }

    #[test]
    fn other_topics_reach_the_hook_and_never_the_ring() {
        let _serial = serialised();
        let link = MockLink::new();
        let mqtt = MockMqttClient::new();
        let mut ring = TestRing::default();
        let frames = ring.frames.clone();
        let rec = Recorder::default();
        let others = rec.others.clone();
        let rendered = rec.rendered.clone();
        let fut = run(&link, &mqtt, &mut ring, rec, cfg());
        let mut app = pin!(fut);

        poll(&mut app);
        let shown = frames.borrow().len();
        mqtt.deliver("ota/command", b"{}");
        poll(&mut app);
        assert_eq!(
            others.borrow().as_slice(),
            &[("ota/command".into(), b"{}".to_vec())]
        );
        assert_eq!(frames.borrow().len(), shown);
        assert_eq!(*rendered.borrow(), 0);
        assert!(mqtt.published().is_empty(), "the app never publishes");
    }

    #[test]
    fn tick_rendered_fires_once_per_valid_tick() {
        let _serial = serialised();
        let link = MockLink::new();
        let mqtt = MockMqttClient::new();
        let mut ring = TestRing::default();
        let rec = Recorder::default();
        let rendered = rec.rendered.clone();
        let fut = run(&link, &mqtt, &mut ring, rec, cfg());
        let mut app = pin!(fut);

        for s in 0..3 {
            mqtt.deliver(
                TICK,
                format!(r#"{{"hour":0,"minute":0,"second":{s}}}"#).as_bytes(),
            );
        }
        mqtt.deliver(TICK, b"garbage");
        poll(&mut app);
        assert_eq!(*rendered.borrow(), 3);
    }

    #[test]
    fn unit_hooks_ignore_everything() {
        let mut hooks = ();
        assert!(!hooks.other_message("a", b"b"));
        hooks.tick_rendered(Instant::now());
    }

    #[test]
    fn effect_builders_construct() {
        assert!(rainbow_effect().is_some());
        assert!(pulse_effect().is_some());
    }
}
