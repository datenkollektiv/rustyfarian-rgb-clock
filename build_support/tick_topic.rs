//! Resolves the MQTT tick topic baked into the firmware.
//!
//! Included by `build.rs` via `#[path]` and unit-tested on the host with plain
//! `rustc --test` (`just tick-topic-tests`), since the firmware crate itself
//! cannot run `cargo test` on the host.

/// Topic used when no override is set.
pub const DEFAULT_TICK_TOPIC: &str = "tick";

/// Returns the tick topic for a raw `TICK_TOPIC_OVERRIDE` value.
///
/// Unset, empty, or whitespace-only means the default `tick`. A set value is
/// trimmed and must be a plain MQTT topic: at most 64 printable ASCII
/// characters, no `+`/`#` wildcards, no leading `$`, and not an OTA topic
/// (the `on_message` handler routes those first, so the clock would never see
/// a tick).
pub fn resolve(raw: Option<&str>) -> Result<String, String> {
    let topic = match raw.map(str::trim).filter(|t| !t.is_empty()) {
        Some(t) => t,
        None => return Ok(DEFAULT_TICK_TOPIC.to_string()),
    };
    let valid = topic.len() <= 64
        && topic.chars().all(|c| c.is_ascii_graphic())
        && !topic.contains(['+', '#'])
        && !topic.starts_with('$')
        && topic != "ota/command"
        && topic != "ota/status";
    if valid {
        Ok(topic.to_string())
    } else {
        Err(format!(
            "TICK_TOPIC_OVERRIDE=\"{topic}\" must be a plain MQTT topic: at most 64 printable ASCII characters, no whitespace, no `+`/`#` wildcards, no leading `$`, and not an OTA topic"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unset_empty_and_blank_fall_back_to_default() {
        assert_eq!(resolve(None).unwrap(), "tick");
        assert_eq!(resolve(Some("")).unwrap(), "tick");
        assert_eq!(resolve(Some("  \t")).unwrap(), "tick");
    }

    #[test]
    fn valid_override_is_trimmed_and_kept() {
        assert_eq!(resolve(Some(" tick-deaf-c3 ")).unwrap(), "tick-deaf-c3");
        assert_eq!(resolve(Some("demo/c3/no-tick")).unwrap(), "demo/c3/no-tick");
        assert_eq!(resolve(Some(&"a".repeat(64))).unwrap().len(), 64);
    }

    #[test]
    fn wildcards_are_rejected() {
        assert!(resolve(Some("tick/#")).is_err());
        assert!(resolve(Some("tick/+/x")).is_err());
    }

    #[test]
    fn ota_topics_are_rejected() {
        assert!(resolve(Some("ota/command")).is_err());
        assert!(resolve(Some("ota/status")).is_err());
    }

    #[test]
    fn system_topics_whitespace_and_overlong_are_rejected() {
        assert!(resolve(Some("$SYS/tick")).is_err());
        assert!(resolve(Some("tick deaf")).is_err());
        assert!(resolve(Some("tické")).is_err());
        assert!(resolve(Some(&"a".repeat(65))).is_err());
    }
}
