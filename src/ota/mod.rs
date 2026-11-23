//! The lab image's DDI client: the clock's identity, timings and health predicate,
//! wired into the upstream DDI runtime. Compiled only with the `ddi-lab` feature;
//! the default image carries no OTA code (`docs/adr/007-ddi-is-the-only-ota-path.md`).
//!
//! Everything else lives upstream in `rustyfarian_esp_idf_network::ota::ddi`: the
//! polling worker, the download thread, the persisted attempt and rollback records,
//! boot reconciliation, the health policy around our predicate, and the feedback
//! the server receives over HTTP. Nothing is published over MQTT. The only MQTT
//! part is the nudge topic, which asks the worker for an early poll; the hook only
//! compares the topic and sets a flag, and runs on the app task, never inside an
//! MQTT callback (ADR 005).
//!
//! Hook order, as the upstream module docs require: [`open_records`] early in
//! `main`, [`Lab::prepare`] before Wi-Fi, then [`Lab::run`] once the clock app is
//! spawned. On the portal path, [`roll_back_if_pending_before_portal`] runs first, so a
//! pending image never waits in the portal. The token is a secret: it is never logged, only "missing" or "invalid".

pub mod policy;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Context;
use clock_app::Hooks;
use clock_pure::encode_tick_ms;
use esp_idf_svc::ota::EspOta;
use rustyfarian_esp_idf_network::mqtt::MqttHandle;
#[cfg(feature = "unhealthy")]
use rustyfarian_esp_idf_network::ota::ddi::DeadlineAction;
use rustyfarian_esp_idf_network::ota::ddi::{
    DdiHandle, DdiNudge, DdiRuntime, DdiSettings, DdiToken, DdiTransportConfig, RestartFn,
};
#[cfg(feature = "unhealthy")]
use rustyfarian_esp_idf_network::ota::RollbackReason;
use rustyfarian_esp_idf_network::ota::{
    read_hardware_facts, url_for_log, BusyRetry, SlotState, Version, TARGET_CHIP,
};
use rustyfarian_esp_idf_network::provisioning::ProvisioningConfig;
use rustyfarian_esp_idf_network::wifi::WiFiManager;

use self::policy::HealthSignals;

pub use rustyfarian_esp_idf_network::ota::ddi::{open_records, Records};

/// The DDI server base URL with the tenant path, baked in by `build.rs`, which
/// refuses a lab build without it.
const DDI_BASE_URL: &str = env!("DDI_BASE_URL");
/// `Cargo.toml` package version, only to flag an override in the log.
const PACKAGE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The board name the configData reports. PROVISIONAL: the fleet has not named boards yet.
#[cfg(mcu = "esp32c3")]
const BOARD: &str = "c3-devkit";
#[cfg(mcu = "esp32c6")]
const BOARD: &str = "c6-devkitc1";

/// Demo build: fail fast so the bootloader rollback is visible within the demo.
#[cfg(feature = "unhealthy")]
const UNHEALTHY_FAILURE_DEADLINE: Duration = Duration::from_secs(15);

/// Granularity of the wait for the link in [`Lab::run`].
const LINK_WAIT_STEP: Duration = Duration::from_millis(500);

/// Pause before the restart in [`restart_after_failed_start`]: long enough to read the
/// refusal on serial, and it keeps a broken image from spinning.
const RESTART_DELAY: Duration = Duration::from_secs(5);

/// A lab image that fails to start (peripherals, provisioning store, portal, DDI client),
/// or a pending image whose explicit rollback failed ([`roll_back_if_pending_before_portal`]),
/// must not sit idle: returning from `main` only ends the main task, so a pending image
/// would stay unverified until someone resets the board. Restarting lets the bootloader
/// roll a pending image back to the previous one; an image flashed over serial restarts
/// until it is fixed.
pub fn restart_after_failed_start(error: &anyhow::Error) -> ! {
    log::error!(
        "Lab image cannot start: {error:#}; restarting in {} s",
        RESTART_DELAY.as_secs()
    );
    std::thread::sleep(RESTART_DELAY);
    esp_idf_svc::hal::reset::restart()
}

/// Rolls a pending image back instead of letting it open the portal.
///
/// The portal waits without a deadline and the health policy runs only on the clock
/// path, so a pending image in the portal would stay unverified until someone resets
/// the board. The image marks its slot invalid and reboots, which every bootloader
/// honours, even one built without rollback support (see lore "OTA & Firmware
/// Update"); only when that fails, e.g. with no valid previous slot, does it fall back
/// to [`restart_after_failed_start`]. An image flashed over serial is not pending and
/// reaches the portal. An unreadable slot state counts as not pending, so a read
/// error never locks a fresh board out of provisioning.
pub fn roll_back_if_pending_before_portal() {
    match read_hardware_facts(BusyRetry::default()) {
        Ok(facts) if facts.running_state == SlotState::PendingVerify => {
            log::error!(
                "The running image is pending verification and needs the portal, which has no deadline; rolling back"
            );
            // Returns only when the rollback failed.
            let error = match EspOta::new() {
                Ok(mut ota) => anyhow::Error::from(ota.mark_running_slot_invalid_and_reboot()),
                Err(e) => anyhow::Error::from(e),
            };
            restart_after_failed_start(&error.context("rollback of the pending image failed"));
        }
        Ok(_) => {}
        Err(e) => log::warn!("Cannot read the running slot state ({e:?}); opening the portal"),
    }
}

/// The stored DDI target token, or why it cannot be used.
///
/// The error text never carries the token. A missing or malformed token sends the
/// device back to the portal, which requires it under the DDI identity rule.
pub fn token(config: &ProvisioningConfig) -> anyhow::Result<DdiToken> {
    let text = match config.ddi_token() {
        Ok(Some(text)) => text,
        Ok(None) => anyhow::bail!("no DDI token stored (missing)"),
        Err(_) => anyhow::bail!("the stored DDI token is invalid"),
    };
    DdiToken::parse(text).map_err(|_| anyhow::anyhow!("the stored DDI token is invalid"))
}

/// Logs the NVS usage of the default partition once at boot, to judge from serial
/// whether the records and the provisioning store leave room.
pub fn log_nvs_stats() {
    let mut stats = esp_idf_svc::sys::nvs_stats_t::default();
    // SAFETY: a null partition name selects the default NVS partition and `stats` is a
    // valid, writable out-parameter for the duration of the call.
    let err = unsafe { esp_idf_svc::sys::nvs_get_stats(core::ptr::null(), &mut stats) };
    if err == esp_idf_svc::sys::ESP_OK {
        log::info!(
            "NVS entries: used={} free={} total={}",
            stats.used_entries,
            stats.free_entries,
            stats.total_entries
        );
    } else {
        log::warn!("NVS statistics unavailable (esp_err {err})");
    }
}

/// What the clock path needs from the early boot steps in `main`.
pub struct LabBoot {
    /// The record store, or why it could not be opened; the clock path refuses
    /// to start without it.
    records: anyhow::Result<Records>,
    /// Captured first in `main`: the health dwell and deadline are measured from it.
    booted_at: Instant,
}

impl LabBoot {
    /// Opens the record store, which also notes the boot slot so the note survives a
    /// crash later in this boot. Kept as a `Result`: the provisioning paths must not
    /// depend on it.
    pub fn open(booted_at: Instant, nvs: esp_idf_svc::nvs::EspDefaultNvsPartition) -> Self {
        let records = open_records(nvs).context("failed to open the OTA records");
        if let Err(e) = &records {
            log::error!("{e:#}");
        }
        Self { records, booted_at }
    }
}

/// The lab state of a clock boot: the DDI runtime before it starts, plus the facts
/// the health predicate and the nudge hook share with the clock app.
pub struct Lab {
    runtime: DdiRuntime,
    records: Records,
    booted_at: Instant,
    /// The health policy's failure deadline, counted from boot.
    failure_deadline: Duration,
    health: Arc<HealthSignals>,
    /// Link readiness (associated with an IP) for the health predicate and the
    /// start of the runtime; the manager itself moves to the app thread.
    wifi_ready: Arc<AtomicBool>,
}

impl Lab {
    /// Builds the runtime from the stored device name and token. Runs before Wi-Fi;
    /// a refusal here is a startup error, which restarts the device
    /// ([`restart_after_failed_start`]), so a pending slot rolls back.
    pub fn prepare(
        early: LabBoot,
        version: &str,
        device_name: &str,
        token: DdiToken,
    ) -> anyhow::Result<Self> {
        if version != PACKAGE_VERSION {
            log::warn!(
                "Firmware version override in effect: reporting {version} (package {PACKAGE_VERSION})"
            );
        }
        log::info!(
            "DDI running version {version}, target {TARGET_CHIP}, board {BOARD}, device id {device_name}, base URL {}",
            url_for_log(DDI_BASE_URL)
        );
        let running = Version::parse(version)
            .map_err(|e| anyhow::anyhow!("firmware version {version:?} does not parse: {e}"))?;
        #[cfg_attr(not(feature = "unhealthy"), allow(unused_mut))]
        let mut settings = DdiSettings::new(running, BOARD)
            .map_err(|e| anyhow::anyhow!("DDI settings refused: {e}"))?;
        #[cfg(feature = "unhealthy")]
        {
            settings.timings.health.failure_deadline = UNHEALTHY_FAILURE_DEADLINE;
            settings.timings.health.deadline_action =
                DeadlineAction::NoteAndRestart(RollbackReason::Unhealthy);
        }
        let failure_deadline = settings.timings.health.failure_deadline;
        let transport = DdiTransportConfig::new(DDI_BASE_URL, device_name, token);
        let restart: RestartFn = Arc::new(|| esp_idf_svc::hal::reset::restart());
        let runtime = DdiRuntime::new(settings, transport, restart)
            .map_err(|e| anyhow::anyhow!("DDI runtime refused: {e}"))?;
        let LabBoot { records, booted_at } = early;
        Ok(Self {
            runtime,
            records: records?,
            booted_at,
            failure_deadline,
            health: Arc::new(HealthSignals::default()),
            wifi_ready: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Follows the link's ready state for the health predicate and the runtime start.
    pub fn watch_link(&self, wifi: &WiFiManager) {
        let ready = Arc::clone(&self.wifi_ready);
        // Seed only `true`: the sink may already have stored a newer value from the
        // reconnect worker, and a stale `false` snapshot must not overwrite it.
        if wifi.subscribe_ready(move |ready_now| ready.store(ready_now, Ordering::Relaxed)) {
            self.wifi_ready.store(true, Ordering::Relaxed);
        }
    }

    /// The topic to subscribe so the server side can ask for an early poll.
    pub fn nudge_topic(&self) -> String {
        self.runtime.nudge_topic().to_owned()
    }

    /// The app hooks: the health freshness signal and the nudge intake.
    pub fn hooks(&self) -> LabHooks {
        LabHooks {
            booted_at: self.booted_at,
            health: Arc::clone(&self.health),
            nudge: self.runtime.nudge(),
        }
    }

    /// Starts the runtime once the link is up, then runs the health policy on the
    /// calling (main) thread until it settles. Keep the returned handle alive: dropping
    /// it is not part of the runtime's contract.
    ///
    /// A poll before the link is up fails and backs off by the poll interval, so the
    /// start waits for the link, or for the health failure deadline, whichever comes
    /// first; at the deadline the policy must still run to roll a pending image back.
    ///
    /// # Errors
    ///
    /// The runtime cannot start (partition or thread); with no handle a pending image
    /// is not marked valid, and the caller restarts so the bootloader aborts it.
    pub fn run(self, mqtt: &MqttHandle) -> anyhow::Result<DdiHandle> {
        let Self {
            runtime,
            records,
            booted_at,
            health,
            failure_deadline: deadline,
            wifi_ready,
        } = self;
        while !wifi_ready.load(Ordering::Relaxed) && booted_at.elapsed() < deadline {
            std::thread::sleep(LINK_WAIT_STEP);
        }
        if wifi_ready.load(Ordering::Relaxed) {
            log::info!(
                "Link up {} s after boot, starting the DDI runtime",
                booted_at.elapsed().as_secs()
            );
        } else {
            log::warn!(
                "Link still down at the health failure deadline ({} s), starting the DDI runtime anyway",
                deadline.as_secs()
            );
        }
        let handle = runtime.start(records)?;
        log::info!("Setup complete, running the DDI health policy on the main thread");
        // Returns at once unless the running slot is pending verification; otherwise
        // marks it valid once healthy or rolls back at the deadline.
        let verdict = handle.run_health_policy(
            booted_at,
            policy::healthy(booted_at, &health, mqtt, &wifi_ready),
        );
        log::info!(
            "DDI health policy finished: {verdict:?}; stack marks {:?}",
            handle.stack_high_water()
        );
        Ok(handle)
    }
}

/// Tier hooks of the shared clock app: the health signal and the nudge intake.
pub struct LabHooks {
    booted_at: Instant,
    health: Arc<HealthSignals>,
    nudge: DdiNudge,
}

impl Hooks for LabHooks {
    fn tick_rendered(&mut self, _at: embassy_time::Instant) {
        // Freshness signal for the health policy: when the last tick was received,
        // parsed and rendered. Never 0, which means "no tick yet".
        let since_boot = u64::try_from(self.booted_at.elapsed().as_millis()).unwrap_or(u64::MAX);
        self.health
            .last_tick_ms
            .store(encode_tick_ms(since_boot), Ordering::Relaxed);
    }

    /// Flag only (ADR 005): the nudge compares the topic and wakes the worker, and
    /// nothing is published from here.
    fn other_message(&mut self, topic: &str, _payload: &[u8]) -> bool {
        self.nudge.on_message(topic)
    }
}
