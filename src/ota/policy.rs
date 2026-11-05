//! The clock's health predicate for the upstream `mark_valid` policy.
//!
//! Health here is stricter than the workspace floor (IPv4 plus 30 s): a build whose
//! MQTT subscription or clock rendering is broken must not be marked valid. The
//! upstream policy adds the 30 s dwell, the refusal flag set by boot
//! reconciliation, the 120 s failure deadline and the rollback; it never touches
//! a slot that is not pending verification, so a normally booted image cannot
//! reboot itself because the broker happens to be down.

#[cfg(not(feature = "unhealthy"))]
use clock_pure::tick_is_fresh;
use rustyfarian_esp_idf_network::mqtt::MqttHandle;
#[cfg(not(feature = "unhealthy"))]
use std::sync::atomic::Ordering;
use std::sync::atomic::{AtomicBool, AtomicU32};
#[cfg(not(feature = "unhealthy"))]
use std::time::Duration;
use std::time::Instant;

/// A tick must have rendered within this window to count. Ticks arrive once a
/// second; a clock that rendered once at boot and then stalled is not healthy.
#[cfg(not(feature = "unhealthy"))]
const TICK_FRESHNESS: Duration = Duration::from_secs(10);

/// Health facts only the app can observe.
#[derive(Default)]
pub struct HealthSignals {
    /// Milliseconds since boot at which the last `tick` was parsed and rendered
    /// on the ring; `0` means never. Proves the subscription path and the LED
    /// driver end to end, and that the clock is still running now. 32 bits
    /// (no 64-bit atomics on RV32) wrap after 49 days; the policy only reads
    /// this within the first two minutes after boot.
    pub last_tick_ms: AtomicU32,
}

/// The predicate handed to `OtaHandle::run_health_policy`: a fresh tick, MQTT
/// connected, and an IPv4 lease. `ready` is the link's ready flag (associated with an
/// IP address), set from `WiFiManager::subscribe_ready` because the manager itself
/// lives on the app thread. The lease is latched: a lease once held counts.
#[cfg(not(feature = "unhealthy"))]
pub fn healthy<'a>(
    boot: Instant,
    signals: &'a HealthSignals,
    mqtt: &'a MqttHandle,
    ready: &'a AtomicBool,
) -> impl FnMut() -> bool + 'a {
    let mut ipv4 = false;
    move || {
        ipv4 = ipv4 || ready.load(Ordering::Relaxed);
        let tick_fresh = tick_is_fresh(
            u64::try_from(boot.elapsed().as_millis()).unwrap_or(u64::MAX),
            signals.last_tick_ms.load(Ordering::Relaxed),
            u64::try_from(TICK_FRESHNESS.as_millis()).unwrap_or(u64::MAX),
        );
        tick_fresh && mqtt.is_connected() && ipv4
    }
}

/// Demo build for hardware scenario 7: a valid, correctly hashed image that never
/// passes its health check.
#[cfg(feature = "unhealthy")]
pub fn healthy<'a>(
    _boot: Instant,
    _signals: &'a HealthSignals,
    _mqtt: &'a MqttHandle,
    _ready: &'a AtomicBool,
) -> impl FnMut() -> bool + 'a {
    || false
}
