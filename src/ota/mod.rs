//! OTA application layer: the clock's topics, firmware version, timings and
//! health predicate, wired into the upstream OTA runtime.
//!
//! Everything else lives upstream in `rustyfarian_esp_idf_network::ota::runtime`:
//! MQTT command intake, the update worker and `rolled_back` reporter threads,
//! the persisted attempt and rollback records, boot reconciliation, the health
//! policy around our predicate, and every status published on [`STATUS_TOPIC`].
//! See `docs/features/ota-mvp-v1.md` for the contract.
//!
//! Wire contract on `ota/command` (inbound, bounded to
//! [`MAX_COMMAND_BYTES`](rustyfarian_esp_idf_network::ota::MAX_COMMAND_BYTES)):
//!
//! ```json
//! {"manifest_url": "http://host:8000/idf_c6_rgb_clock/release/manifest.json", "sig": null}
//! {"action": "rollback", "from": "0.3.0"}
//! ```
//!
//! A `sig` is accepted but NOT verified: today the trust boundary is the LAN, so
//! whoever can publish on `ota/command` and answer the manifest URL controls the
//! image. Nothing may publish from an MQTT callback (bug 001); the runtime's
//! submitter only parses and enqueues.

pub mod policy;

use std::sync::Arc;

use anyhow::Context;
#[cfg(feature = "unhealthy")]
use rustyfarian_esp_idf_network::ota::runtime::DeadlineAction;
use rustyfarian_esp_idf_network::ota::runtime::{OtaConfig, RestartFn};
#[cfg(feature = "unhealthy")]
use rustyfarian_esp_idf_network::ota::RollbackReason;
use rustyfarian_esp_idf_network::ota::{Version, TARGET_CHIP};

pub use rustyfarian_esp_idf_network::ota::runtime::{channel, open_records};

/// Inbound command topic. Flat, matching the existing `tick` convention.
pub const COMMAND_TOPIC: &str = "ota/command";
/// Outbound status topic.
pub const STATUS_TOPIC: &str = "ota/status";
/// Version this image reports and compares offers against: the package version,
/// or `FIRMWARE_VERSION_OVERRIDE` for demo images (validated in `build.rs`).
pub const RUNNING_VERSION: &str = env!("FIRMWARE_VERSION");
/// `Cargo.toml` package version, only to flag an override in the log.
const PACKAGE_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Demo build: fail fast so the bootloader rollback is visible within the demo.
#[cfg(feature = "unhealthy")]
const UNHEALTHY_FAILURE_DEADLINE: std::time::Duration = std::time::Duration::from_secs(15);

/// The runtime configuration: our topics and version, upstream's default
/// timings and stacks (they are the clock's values), and a plain restart.
///
/// The unhealthy demo build (hardware scenario 7) fails its health check after
/// 15 s and restarts *without* `rollback()`, so the bootloader's own `ABORTED`
/// path rolls it back and the report reads `unhealthy`.
pub fn config() -> anyhow::Result<OtaConfig> {
    if RUNNING_VERSION != PACKAGE_VERSION {
        log::warn!(
            "Firmware version override in effect: reporting {RUNNING_VERSION} (package {PACKAGE_VERSION})"
        );
    }
    log::info!("OTA running version {RUNNING_VERSION}, target {TARGET_CHIP}");
    let running = Version::parse(RUNNING_VERSION)
        .map_err(|e| anyhow::anyhow!("firmware version {RUNNING_VERSION:?} does not parse: {e}"))?;
    let restart: RestartFn = Arc::new(|| esp_idf_svc::hal::reset::restart());
    let config = OtaConfig::new(COMMAND_TOPIC, STATUS_TOPIC, running, restart)
        .context("invalid OTA configuration")?;
    #[cfg(feature = "unhealthy")]
    let config = {
        let mut config = config;
        config.settings.timings.failure_deadline = UNHEALTHY_FAILURE_DEADLINE;
        config.settings.timings.deadline_action =
            DeadlineAction::NoteAndRestart(RollbackReason::Unhealthy);
        config
    };
    config.validate().context("invalid OTA configuration")?;
    Ok(config)
}
