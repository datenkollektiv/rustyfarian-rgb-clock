#![cfg_attr(not(test), no_std)]

//! Pure Rust clock utilities for 12-LED clock face.
//!
//! This crate provides hardware-independent time-to-LED-index mapping
//! and color manipulation utilities. It has no ESP or embedded dependencies,
//! making it fully testable on any platform.

/// RGB color representation as (r, g, b) tuple.
pub type Rgb = (u8, u8, u8);

/// Maps an hour value (0-23) to the corresponding LED index (0-11).
///
/// The clock has 12 LEDs arranged in a circle. LED 0 is at the 1 o'clock
/// position, LED 11 is at the 12 o'clock position.
///
/// # Mapping
/// - 12 or 0 (midnight/noon) -> LED 11 (12 o'clock position)
/// - 1 or 13 -> LED 0 (1 o'clock position)
/// - 2 or 14 -> LED 1 (2 o'clock position)
/// - etc.
///
/// # Example
///
/// ```
/// use clock_pure::hour_to_index;
///
/// assert_eq!(hour_to_index(12), 11); // 12 o'clock -> LED 11
/// assert_eq!(hour_to_index(1), 0);   // 1 o'clock -> LED 0
/// assert_eq!(hour_to_index(6), 5);   // 6 o'clock -> LED 5
/// ```
pub fn hour_to_index(hour: u8) -> usize {
    (hour as usize + 11) % 12
}

/// Maps a minute value (0-59) to the corresponding LED index (0-11).
///
/// Each LED represents a 5-minute segment. Minutes 0-4 map to 12 o'clock,
/// minutes 5-9 map to 1 o'clock, etc.
///
/// # Mapping
/// - 0-4 -> LED 11 (12 o'clock position)
/// - 5-9 -> LED 0 (1 o'clock position)
/// - 10-14 -> LED 1 (2 o'clock position)
/// - etc.
///
/// # Example
///
/// ```
/// use clock_pure::minute_to_index;
///
/// assert_eq!(minute_to_index(0), 11);  // :00 -> 12 o'clock
/// assert_eq!(minute_to_index(5), 0);   // :05 -> 1 o'clock
/// assert_eq!(minute_to_index(30), 5);  // :30 -> 6 o'clock
/// ```
pub fn minute_to_index(minute: u8) -> usize {
    (minute as usize + 55) % 60 / 5
}

/// Maps a second value (0-59) to the corresponding LED index (0-11).
///
/// Each LED represents a 5-second segment. Identical mapping to minutes.
///
/// # Mapping
/// - 0-4 -> LED 11 (12 o'clock position)
/// - 5-9 -> LED 0 (1 o'clock position)
/// - 10-14 -> LED 1 (2 o'clock position)
/// - etc.
///
/// # Example
///
/// ```
/// use clock_pure::second_to_index;
///
/// assert_eq!(second_to_index(0), 11);  // :00 -> 12 o'clock
/// assert_eq!(second_to_index(59), 10); // :59 -> 11 o'clock
/// ```
pub fn second_to_index(second: u8) -> usize {
    (second as usize + 55) % 60 / 5
}

/// Multiplies an RGB color by a brightness factor using saturating arithmetic.
///
/// # Example
///
/// ```
/// use clock_pure::scale_color;
///
/// let color = (10, 20, 30);
/// let scaled = scale_color(color, 2);
/// assert_eq!(scaled, (20, 40, 60));
///
/// // Saturates at 255
/// let bright = scale_color((100, 100, 100), 10);
/// assert_eq!(bright, (255, 255, 255));
/// ```
pub fn scale_color(color: Rgb, factor: u8) -> Rgb {
    (
        color.0.saturating_mul(factor),
        color.1.saturating_mul(factor),
        color.2.saturating_mul(factor),
    )
}

/// Adds two RGB colors together using saturating arithmetic.
///
/// # Example
///
/// ```
/// use clock_pure::add_colors;
///
/// let a = (100, 50, 25);
/// let b = (50, 100, 75);
/// assert_eq!(add_colors(a, b), (150, 150, 100));
///
/// // Saturates at 255
/// let c = (200, 200, 200);
/// let d = (100, 100, 100);
/// assert_eq!(add_colors(c, d), (255, 255, 255));
/// ```
pub fn add_colors(a: Rgb, b: Rgb) -> Rgb {
    (
        a.0.saturating_add(b.0),
        a.1.saturating_add(b.1),
        a.2.saturating_add(b.2),
    )
}

/// Number of LEDs on the clock face.
pub const LED_COUNT: usize = 12;

/// Default brightness factor applied to the hand colors.
pub const DEFAULT_BRIGHTNESS: u8 = 10;

/// Base colors of the three clock hands (before brightness scaling).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClockColors {
    /// Hour hand color.
    pub hour: Rgb,
    /// Minute hand color.
    pub minute: Rgb,
    /// Second hand color.
    pub second: Rgb,
}

impl Default for ClockColors {
    /// Hours blue (0, 0, 1), minutes green (0, 1, 0), seconds red (1, 0, 0).
    fn default() -> Self {
        Self {
            hour: (0, 0, 1),
            minute: (0, 1, 0),
            second: (1, 0, 0),
        }
    }
}

/// Renders a time onto the 12-LED clock face.
///
/// The hour hand is set first, the minute and second hands are blended on top
/// with saturating addition (so overlapping hands mix their colors), and the
/// result is scaled by `brightness`.
///
/// # Example
///
/// ```
/// use clock_pure::{render_time, ClockColors, DEFAULT_BRIGHTNESS};
///
/// let leds = render_time(12, 0, 0, &ClockColors::default(), DEFAULT_BRIGHTNESS);
/// assert_eq!(leds[11], (10, 10, 10)); // all three hands overlap -> white
/// assert_eq!(leds[0], (0, 0, 0));
/// ```
pub fn render_time(
    hour: u8,
    minute: u8,
    second: u8,
    colors: &ClockColors,
    brightness: u8,
) -> [Rgb; LED_COUNT] {
    let mut state = [(0, 0, 0); LED_COUNT];
    state[hour_to_index(hour)] = colors.hour;
    let minute_idx = minute_to_index(minute);
    state[minute_idx] = add_colors(state[minute_idx], colors.minute);
    let second_idx = second_to_index(second);
    state[second_idx] = add_colors(state[second_idx], colors.second);
    state.map(|c| scale_color(c, brightness))
}

/// Rainbow startup animation speed.
pub const RAINBOW_SPEED: u8 = 3;
/// Rainbow startup animation brightness.
pub const RAINBOW_BRIGHTNESS: u8 = 30;
/// Delay between rainbow animation frames, in milliseconds.
pub const RAINBOW_FRAME_DELAY_MS: u32 = 30;
/// Amber color of the provisioning ("pairing mode") pulse.
pub const PROVISION_COLOR: Rgb = (255, 120, 0);
/// Provisioning pulse speed.
pub const PROVISION_PULSE_SPEED: u8 = 3;
/// Provisioning pulse minimum brightness.
pub const PROVISION_MIN_BRIGHTNESS: u8 = 5;
/// Provisioning pulse maximum brightness.
pub const PROVISION_MAX_BRIGHTNESS: u8 = 60;
/// Delay between provisioning pulse frames, in milliseconds.
pub const PROVISION_FRAME_DELAY_MS: u32 = 30;

/// Maximum accepted tick payload length in bytes.
///
/// Longer payloads are rejected with [`TickError::TooLong`] before any
/// scanning, so a hostile publisher cannot make the parser do unbounded work.
pub const MAX_TICK_LEN: usize = 64;

/// A field of the tick payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    /// The `hour` field (0..=23).
    Hour,
    /// The `minute` field (0..=59).
    Minute,
    /// The `second` field (0..=59).
    Second,
}

impl Field {
    const fn name(self) -> &'static str {
        match self {
            Field::Hour => "hour",
            Field::Minute => "minute",
            Field::Second => "second",
        }
    }

    const fn max(self) -> u32 {
        match self {
            Field::Hour => 23,
            Field::Minute | Field::Second => 59,
        }
    }

    const fn slot(self) -> usize {
        match self {
            Field::Hour => 0,
            Field::Minute => 1,
            Field::Second => 2,
        }
    }
}

impl core::fmt::Display for Field {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.name())
    }
}

/// Reason a tick payload was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TickError {
    /// The payload is longer than [`MAX_TICK_LEN`].
    TooLong,
    /// The payload is not a flat JSON object of unsigned decimal integers.
    Syntax,
    /// The object contains a key other than `hour`, `minute` or `second`.
    UnknownField,
    /// A field appears more than once.
    DuplicateField(Field),
    /// A field is absent.
    MissingField(Field),
    /// A field is outside its range (hour 0..=23, minute/second 0..=59).
    OutOfRange(Field),
}

impl core::fmt::Display for TickError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            TickError::TooLong => write!(f, "tick payload longer than {MAX_TICK_LEN} bytes"),
            TickError::Syntax => f.write_str("tick payload is not a flat JSON object of integers"),
            TickError::UnknownField => f.write_str("tick payload has an unknown field"),
            TickError::DuplicateField(x) => write!(f, "tick field {x} appears twice"),
            TickError::MissingField(x) => write!(f, "tick field {x} is missing"),
            TickError::OutOfRange(x) => {
                write!(f, "tick field {x} is out of range (max {})", x.max())
            }
        }
    }
}

/// A validated wall-clock time received on the tick topic.
///
/// Fields are private: the only ways to obtain one are [`parse_tick`] and
/// `TryFrom<&[u8]>`, so an out-of-range value cannot exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TickTime {
    hour: u8,
    minute: u8,
    second: u8,
}

impl TickTime {
    /// Hour of day, 0..=23.
    #[must_use]
    pub const fn hour(&self) -> u8 {
        self.hour
    }

    /// Minute, 0..=59.
    #[must_use]
    pub const fn minute(&self) -> u8 {
        self.minute
    }

    /// Second, 0..=59.
    #[must_use]
    pub const fn second(&self) -> u8 {
        self.second
    }
}

impl TryFrom<&[u8]> for TickTime {
    type Error = TickError;

    fn try_from(payload: &[u8]) -> Result<Self, Self::Error> {
        parse_tick(payload)
    }
}

struct Scanner<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl Scanner<'_> {
    fn skip_ws(&mut self) {
        while matches!(self.bytes.get(self.pos), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    fn eat(&mut self, byte: u8) -> Result<(), TickError> {
        self.skip_ws();
        if self.bytes.get(self.pos) == Some(&byte) {
            self.pos += 1;
            Ok(())
        } else {
            Err(TickError::Syntax)
        }
    }

    fn key(&mut self) -> Result<Field, TickError> {
        self.eat(b'"')?;
        let rest = &self.bytes[self.pos..];
        let end = rest
            .iter()
            .position(|&b| b == b'"')
            .ok_or(TickError::Syntax)?;
        let field = match &rest[..end] {
            b"hour" => Field::Hour,
            b"minute" => Field::Minute,
            b"second" => Field::Second,
            k if k.contains(&b'\\') => return Err(TickError::Syntax),
            _ => return Err(TickError::UnknownField),
        };
        self.pos += end + 1;
        Ok(field)
    }

    /// Unsigned decimal integer without sign or leading zeros; saturates.
    fn number(&mut self) -> Result<u32, TickError> {
        self.skip_ws();
        let start = self.pos;
        let mut value: u32 = 0;
        while let Some(&b) = self.bytes.get(self.pos) {
            if !b.is_ascii_digit() {
                break;
            }
            value = value.saturating_mul(10).saturating_add(u32::from(b - b'0'));
            self.pos += 1;
        }
        let digits = &self.bytes[start..self.pos];
        match digits {
            [] => Err(TickError::Syntax),
            [b'0', _, ..] => Err(TickError::Syntax),
            _ => Ok(value),
        }
    }
}

/// Parses a tick payload of the form `{"hour":14,"minute":23,"second":45}`.
///
/// Accepted: a flat JSON object with exactly the keys `hour`, `minute` and
/// `second` (any order, each once), unsigned decimal integer values without
/// leading zeros, and ASCII whitespace between tokens. Anything else is
/// rejected, including out-of-range values (hour 0..=23, minute and second
/// 0..=59) and trailing bytes.
///
/// # Errors
///
/// Returns a [`TickError`] describing the first problem found.
///
/// # Example
///
/// ```
/// use clock_pure::{parse_tick, TickError, Field};
///
/// let t = parse_tick(br#"{"hour":14,"minute":23,"second":45}"#).unwrap();
/// assert_eq!((t.hour(), t.minute(), t.second()), (14, 23, 45));
/// assert_eq!(
///     parse_tick(br#"{"hour":24,"minute":0,"second":0}"#),
///     Err(TickError::OutOfRange(Field::Hour)),
/// );
/// ```
pub fn parse_tick(payload: &[u8]) -> Result<TickTime, TickError> {
    if payload.len() > MAX_TICK_LEN {
        return Err(TickError::TooLong);
    }
    let mut sc = Scanner {
        bytes: payload,
        pos: 0,
    };
    let mut values: [Option<u32>; 3] = [None; 3];
    sc.eat(b'{')?;
    for n in 0..3 {
        if n > 0 {
            sc.eat(b',')?;
        }
        let field = sc.key()?;
        sc.eat(b':')?;
        let value = sc.number()?;
        let slot = &mut values[field.slot()];
        if slot.is_some() {
            return Err(TickError::DuplicateField(field));
        }
        *slot = Some(value);
    }
    sc.eat(b'}')?;
    sc.skip_ws();
    if sc.pos != payload.len() {
        return Err(TickError::Syntax);
    }
    let mut out = [0u8; 3];
    for field in [Field::Hour, Field::Minute, Field::Second] {
        let v = values[field.slot()].ok_or(TickError::MissingField(field))?;
        if v > field.max() {
            return Err(TickError::OutOfRange(field));
        }
        // Checked above: v <= 59.
        out[field.slot()] = u8::try_from(v).map_err(|_| TickError::OutOfRange(field))?;
    }
    Ok(TickTime {
        hour: out[0],
        minute: out[1],
        second: out[2],
    })
}

/// Renders a validated tick onto the 12-LED face; see [`render_time`].
#[must_use]
pub fn render_tick(tick: &TickTime, colors: &ClockColors, brightness: u8) -> [Rgb; LED_COUNT] {
    render_time(tick.hour, tick.minute, tick.second, colors, brightness)
}

/// Encodes milliseconds since boot as a non-zero `u32` tick timestamp.
///
/// `0` is reserved for "no tick yet", so the result is at least 1; values
/// beyond `u32::MAX` saturate.
#[must_use]
pub fn encode_tick_ms(since_boot_ms: u64) -> u32 {
    u32::try_from(since_boot_ms.max(1)).unwrap_or(u32::MAX)
}

/// Whether the last tick (an [`encode_tick_ms`] value, `0` meaning never) is
/// no older than `window_ms` at `now_ms` milliseconds since boot.
#[must_use]
pub fn tick_is_fresh(now_ms: u64, last_tick_ms: u32, window_ms: u64) -> bool {
    last_tick_ms != 0 && now_ms.saturating_sub(u64::from(last_tick_ms)) <= window_ms
}

/// Returns the trimmed `value` when it is set and non-blank, else `fallback`.
///
/// Used for build-time portal prefills so a padded env var cannot ship
/// whitespace; the build-script guard must trim the same way.
#[must_use]
pub fn prefill_or<'a>(value: Option<&'a str>, fallback: &'a str) -> &'a str {
    match value.map(str::trim_ascii) {
        Some(v) if !v.is_empty() => v,
        _ => fallback,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===== hour_to_index tests =====

    #[test]
    fn test_hour_to_index_12_oclock() {
        // 12 o'clock (noon or midnight) -> LED 11
        assert_eq!(hour_to_index(0), 11);
        assert_eq!(hour_to_index(12), 11);
    }

    #[test]
    fn test_hour_to_index_1_oclock() {
        // 1 o'clock -> LED 0
        assert_eq!(hour_to_index(1), 0);
        assert_eq!(hour_to_index(13), 0);
    }

    #[test]
    fn test_hour_to_index_6_oclock() {
        // 6 o'clock -> LED 5
        assert_eq!(hour_to_index(6), 5);
        assert_eq!(hour_to_index(18), 5);
    }

    #[test]
    fn test_hour_to_index_all_hours() {
        // Verify all 24 hours map correctly
        let expected = [
            11, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10,
        ];
        for (hour, &expected_idx) in expected.iter().enumerate() {
            assert_eq!(
                hour_to_index(hour as u8),
                expected_idx,
                "hour {} should map to index {}",
                hour,
                expected_idx
            );
        }
    }

    // ===== minute_to_index tests =====

    #[test]
    fn test_minute_to_index_12_oclock() {
        // Minutes 0-4 -> LED 11 (12 o'clock)
        for m in 0..5 {
            assert_eq!(minute_to_index(m), 11, "minute {} should be 12 o'clock", m);
        }
    }

    #[test]
    fn test_minute_to_index_1_oclock() {
        // Minutes 5-9 -> LED 0 (1 o'clock)
        for m in 5..10 {
            assert_eq!(minute_to_index(m), 0, "minute {} should be 1 o'clock", m);
        }
    }

    #[test]
    fn test_minute_to_index_6_oclock() {
        // Minutes 30-34 -> LED 5 (6 o'clock)
        for m in 30..35 {
            assert_eq!(minute_to_index(m), 5, "minute {} should be 6 o'clock", m);
        }
    }

    #[test]
    fn test_minute_to_index_boundaries() {
        // Test the boundaries of each 5-minute segment
        assert_eq!(minute_to_index(4), 11); // Last minute of 12 o'clock segment
        assert_eq!(minute_to_index(5), 0); // First minute of 1 o'clock segment
        assert_eq!(minute_to_index(54), 9); // Last minute of 10 o'clock segment
        assert_eq!(minute_to_index(55), 10); // First minute of 11 o'clock segment
        assert_eq!(minute_to_index(59), 10); // Last minute of 11 o'clock segment
    }

    #[test]
    fn test_minute_to_index_all_minutes() {
        // Verify all 60 minutes produce valid indices
        for m in 0..60 {
            let idx = minute_to_index(m);
            assert!(idx < 12, "minute {} produced invalid index {}", m, idx);
        }
    }

    // ===== second_to_index tests =====

    #[test]
    fn test_second_to_index_same_as_minute() {
        // Seconds use the same mapping as minutes
        for s in 0..60 {
            assert_eq!(
                second_to_index(s),
                minute_to_index(s),
                "second {} should map same as minute {}",
                s,
                s
            );
        }
    }

    #[test]
    fn test_second_to_index_boundaries() {
        assert_eq!(second_to_index(0), 11);
        assert_eq!(second_to_index(5), 0);
        assert_eq!(second_to_index(59), 10);
    }

    // ===== scale_color tests =====

    #[test]
    fn test_scale_color_basic() {
        assert_eq!(scale_color((10, 20, 30), 2), (20, 40, 60));
        assert_eq!(scale_color((1, 2, 3), 10), (10, 20, 30));
    }

    #[test]
    fn test_scale_color_zero_factor() {
        assert_eq!(scale_color((255, 255, 255), 0), (0, 0, 0));
    }

    #[test]
    fn test_scale_color_one_factor() {
        assert_eq!(scale_color((100, 150, 200), 1), (100, 150, 200));
    }

    #[test]
    fn test_scale_color_saturates() {
        // Should saturate at 255, not overflow
        assert_eq!(scale_color((100, 100, 100), 10), (255, 255, 255));
        assert_eq!(scale_color((128, 64, 32), 3), (255, 192, 96));
    }

    // ===== add_colors tests =====

    #[test]
    fn test_add_colors_basic() {
        assert_eq!(add_colors((10, 20, 30), (5, 10, 15)), (15, 30, 45));
    }

    #[test]
    fn test_add_colors_with_zero() {
        assert_eq!(add_colors((100, 150, 200), (0, 0, 0)), (100, 150, 200));
        assert_eq!(add_colors((0, 0, 0), (100, 150, 200)), (100, 150, 200));
    }

    #[test]
    fn test_add_colors_saturates() {
        // Should saturate at 255, not overflow
        assert_eq!(
            add_colors((200, 200, 200), (100, 100, 100)),
            (255, 255, 255)
        );
        assert_eq!(add_colors((255, 0, 128), (1, 255, 200)), (255, 255, 255));
    }

    #[test]
    fn test_add_colors_partial_saturation() {
        // Only some channels saturate
        assert_eq!(add_colors((200, 50, 100), (100, 50, 100)), (255, 100, 200));
    }

    #[test]
    fn test_add_colors_commutative() {
        let a = (100, 50, 200);
        let b = (30, 180, 60);
        assert_eq!(add_colors(a, b), add_colors(b, a));
    }

    // ===== hour_to_index edge cases =====

    #[test]
    fn test_hour_to_index_last_hours() {
        assert_eq!(hour_to_index(11), 10); // 11 o'clock -> LED 10
        assert_eq!(hour_to_index(23), 10); // 23:xx -> LED 10
    }

    #[test]
    fn test_hour_to_index_am_pm_symmetry() {
        for h in 0..12 {
            assert_eq!(
                hour_to_index(h),
                hour_to_index(h + 12),
                "hour {} and {} should map to the same LED",
                h,
                h + 12
            );
        }
    }

    // ===== minute_to_index segment transitions =====

    #[test]
    fn test_minute_to_index_all_segment_boundaries() {
        // Verify the last minute of each segment and first minute of the next
        let transitions = [
            (4, 11, 5, 0),   // 12→1
            (9, 0, 10, 1),   // 1→2
            (14, 1, 15, 2),  // 2→3
            (19, 2, 20, 3),  // 3→4
            (24, 3, 25, 4),  // 4→5
            (29, 4, 30, 5),  // 5→6
            (34, 5, 35, 6),  // 6→7
            (39, 6, 40, 7),  // 7→8
            (44, 7, 45, 8),  // 8→9
            (49, 8, 50, 9),  // 9→10
            (54, 9, 55, 10), // 10→11
        ];
        for (last_min, last_idx, first_min, first_idx) in transitions {
            assert_eq!(
                minute_to_index(last_min),
                last_idx,
                "minute {} should be last in segment",
                last_min
            );
            assert_eq!(
                minute_to_index(first_min),
                first_idx,
                "minute {} should be first in next segment",
                first_min
            );
        }
    }

    #[test]
    fn test_minute_to_index_last_segment_wraps() {
        // Minutes 55-59 form the 11 o'clock segment (LED 10)
        // Minute 0 wraps back to 12 o'clock (LED 11)
        assert_eq!(minute_to_index(59), 10);
        assert_eq!(minute_to_index(0), 11);
    }

    // ===== scale_color edge cases =====

    #[test]
    fn test_scale_color_max_values() {
        assert_eq!(scale_color((255, 255, 255), 255), (255, 255, 255));
    }

    #[test]
    fn test_scale_color_asymmetric_channels() {
        assert_eq!(scale_color((1, 128, 255), 2), (2, 255, 255));
    }

    #[test]
    fn test_scale_color_black_input() {
        assert_eq!(scale_color((0, 0, 0), 255), (0, 0, 0));
    }

    // ===== Clock hand overlap scenarios =====
    //
    // These simulate the color blending logic from RGBClock::set_local_time
    // using the actual hand colors: hour=blue(0,0,1), minute=green(0,1,0), second=red(1,0,0).

    const HOUR_COLOR: Rgb = (0, 0, 1);
    const MINUTE_COLOR: Rgb = (0, 1, 0);
    const SECOND_COLOR: Rgb = (1, 0, 0);

    #[test]
    fn test_overlap_hour_and_minute_same_led() {
        // 12:00:15 — hour and minute both at 12 o'clock (LED 11)
        let hour_idx = hour_to_index(12);
        let minute_idx = minute_to_index(0);
        assert_eq!(hour_idx, minute_idx);

        let blended = add_colors(HOUR_COLOR, MINUTE_COLOR);
        assert_eq!(blended, (0, 1, 1)); // cyan
    }

    #[test]
    fn test_overlap_all_three_hands_same_led() {
        // 12:00:00 — all three hands at 12 o'clock (LED 11)
        let hour_idx = hour_to_index(0);
        let minute_idx = minute_to_index(0);
        let second_idx = second_to_index(0);
        assert_eq!(hour_idx, 11);
        assert_eq!(minute_idx, 11);
        assert_eq!(second_idx, 11);

        let blended = add_colors(add_colors(HOUR_COLOR, MINUTE_COLOR), SECOND_COLOR);
        assert_eq!(blended, (1, 1, 1)); // white
    }

    #[test]
    fn test_overlap_minute_and_second_same_led() {
        // 3:30:30 — minute and second both at 6 o'clock (LED 5), hour at 3 (LED 2)
        let hour_idx = hour_to_index(3);
        let minute_idx = minute_to_index(30);
        let second_idx = second_to_index(30);
        assert_eq!(hour_idx, 2);
        assert_eq!(minute_idx, 5);
        assert_eq!(second_idx, 5);
        assert_ne!(hour_idx, minute_idx);

        let blended = add_colors(MINUTE_COLOR, SECOND_COLOR);
        assert_eq!(blended, (1, 1, 0)); // yellow
    }

    #[test]
    fn test_no_overlap_all_hands_different() {
        // 3:30:05 — hour at 3 (LED 2), minute at 6 (LED 5), second at 1 (LED 0)
        let hour_idx = hour_to_index(3);
        let minute_idx = minute_to_index(30);
        let second_idx = second_to_index(5);
        assert_eq!(hour_idx, 2);
        assert_eq!(minute_idx, 5);
        assert_eq!(second_idx, 0);
    }

    #[test]
    fn test_overlap_with_brightness_scaling() {
        // Simulate full pipeline: blend then scale (brightness=10)
        let blended = add_colors(HOUR_COLOR, MINUTE_COLOR);
        let scaled = scale_color(blended, 10);
        assert_eq!(scaled, (0, 10, 10));
    }

    #[test]
    fn test_overlap_scaled_bright_colors_saturate() {
        // Bright base colors that saturate when blended and scaled
        let hour = (0, 0, 30);
        let minute = (0, 30, 0);
        let blended = add_colors(hour, minute);
        let scaled = scale_color(blended, 10);
        assert_eq!(scaled, (0, 255, 255));
    }

    // ===== render_time tests =====

    #[test]
    fn test_render_time_no_overlap() {
        // 3:30:05 -> hour LED 2 blue, minute LED 5 green, second LED 0 red
        let leds = render_time(3, 30, 5, &ClockColors::default(), 10);
        let mut expected = [(0, 0, 0); LED_COUNT];
        expected[2] = (0, 0, 10);
        expected[5] = (0, 10, 0);
        expected[0] = (10, 0, 0);
        assert_eq!(leds, expected);
    }

    #[test]
    fn test_render_time_hour_minute_overlap_is_cyan() {
        // 12:00:30 -> hour and minute on LED 11, second on LED 5
        let leds = render_time(12, 0, 30, &ClockColors::default(), 10);
        assert_eq!(leds[11], (0, 10, 10));
        assert_eq!(leds[5], (10, 0, 0));
    }

    #[test]
    fn test_render_time_all_three_overlap_is_white() {
        let leds = render_time(0, 0, 0, &ClockColors::default(), 10);
        assert_eq!(leds[11], (10, 10, 10));
        assert_eq!(leds.iter().filter(|c| **c != (0, 0, 0)).count(), 1);
    }

    #[test]
    fn test_render_time_boundaries() {
        // (hour, minute, second) -> expected (hour_idx, minute_idx, second_idx)
        let cases = [
            (0, 0, 0, 11, 11, 11),
            (12, 59, 59, 11, 10, 10),
            (23, 0, 59, 10, 11, 10),
            (23, 59, 0, 10, 10, 11),
        ];
        for (h, m, s, hi, mi, si) in cases {
            let colors = ClockColors::default();
            let leds = render_time(h, m, s, &colors, 1);
            let mut expected = [(0, 0, 0); LED_COUNT];
            expected[hi] = colors.hour;
            expected[mi] = add_colors(expected[mi], colors.minute);
            expected[si] = add_colors(expected[si], colors.second);
            assert_eq!(leds, expected, "time {h}:{m}:{s}");
        }
    }

    #[test]
    fn test_render_time_zero_brightness_is_dark() {
        let leds = render_time(12, 34, 56, &ClockColors::default(), 0);
        assert_eq!(leds, [(0, 0, 0); LED_COUNT]);
    }

    // ===== Out-of-range input behavior =====
    //
    // Input types are u8, so values are bounded 0–255. Hours > 23 and
    // minutes/seconds > 59 are not expected from valid MQTT messages but
    // the functions handle them gracefully via modulo — they never panic.

    #[test]
    fn test_hour_to_index_out_of_range_wraps() {
        // hour 24 wraps like hour 0 (midnight)
        assert_eq!(hour_to_index(24), hour_to_index(0));
        // hour 25 wraps like hour 1
        assert_eq!(hour_to_index(25), hour_to_index(1));
    }

    #[test]
    fn test_minute_to_index_out_of_range_wraps() {
        // minute 60 wraps like minute 0
        assert_eq!(minute_to_index(60), minute_to_index(0));
        // minute 65 wraps like minute 5
        assert_eq!(minute_to_index(65), minute_to_index(5));
    }

    #[test]
    fn test_all_index_functions_never_exceed_11() {
        // Exhaustive: no u8 input can produce an index >= 12
        for v in 0..=255u8 {
            assert!(hour_to_index(v) < 12, "hour_to_index({}) >= 12", v);
            assert!(minute_to_index(v) < 12, "minute_to_index({}) >= 12", v);
            assert!(second_to_index(v) < 12, "second_to_index({}) >= 12", v);
        }
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn add_colors_is_commutative(
            r1 in 0..=255u8, g1 in 0..=255u8, b1 in 0..=255u8,
            r2 in 0..=255u8, g2 in 0..=255u8, b2 in 0..=255u8,
        ) {
            let a = (r1, g1, b1);
            let b = (r2, g2, b2);
            prop_assert_eq!(add_colors(a, b), add_colors(b, a));
        }

        #[test]
        fn scale_color_zero_always_black(r in 0..=255u8, g in 0..=255u8, b in 0..=255u8) {
            prop_assert_eq!(scale_color((r, g, b), 0), (0, 0, 0));
        }

        #[test]
        fn scale_color_one_is_identity(r in 0..=255u8, g in 0..=255u8, b in 0..=255u8) {
            prop_assert_eq!(scale_color((r, g, b), 1), (r, g, b));
        }

        #[test]
        fn scale_color_never_decreases(
            r in 0..=255u8, g in 0..=255u8, b in 0..=255u8,
            factor in 1..=255u8,
        ) {
            let scaled = scale_color((r, g, b), factor);
            prop_assert!(scaled.0 >= r);
            prop_assert!(scaled.1 >= g);
            prop_assert!(scaled.2 >= b);
        }

        #[test]
        fn add_colors_black_is_identity(r in 0..=255u8, g in 0..=255u8, b in 0..=255u8) {
            let color = (r, g, b);
            prop_assert_eq!(add_colors(color, (0, 0, 0)), color);
            prop_assert_eq!(add_colors((0, 0, 0), color), color);
        }

        #[test]
        fn add_colors_never_decreases(
            r1 in 0..=255u8, g1 in 0..=255u8, b1 in 0..=255u8,
            r2 in 0..=255u8, g2 in 0..=255u8, b2 in 0..=255u8,
        ) {
            let a = (r1, g1, b1);
            let b = (r2, g2, b2);
            let result = add_colors(a, b);
            prop_assert!(result.0 >= r1);
            prop_assert!(result.1 >= g1);
            prop_assert!(result.2 >= b1);
        }

        #[test]
        fn hour_to_index_always_valid(hour in 0..=255u8) {
            prop_assert!(hour_to_index(hour) < 12);
        }

        #[test]
        fn minute_to_index_always_valid(minute in 0..=255u8) {
            prop_assert!(minute_to_index(minute) < 12);
        }

        #[test]
        fn hour_to_index_am_pm_equivalent(hour in 0..=11u8) {
            prop_assert_eq!(hour_to_index(hour), hour_to_index(hour + 12));
        }
    }
}

#[cfg(test)]
mod tick_tests {
    use super::*;
    use proptest::prelude::*;

    fn ok(p: &str) -> (u8, u8, u8) {
        let t = parse_tick(p.as_bytes()).unwrap();
        (t.hour(), t.minute(), t.second())
    }

    #[test]
    fn valid_table() {
        let cases = [
            (r#"{"hour":14,"minute":23,"second":45}"#, (14, 23, 45)),
            (r#"{"hour": 14, "minute": 23, "second": 45}"#, (14, 23, 45)),
            (r#"{"second":5,"hour":0,"minute":59}"#, (0, 59, 5)),
            (
                " \t\r\n{ \"hour\" : 23 , \"minute\" : 0 , \"second\" : 59 }\n",
                (23, 0, 59),
            ),
            (r#"{"hour":0,"minute":0,"second":0}"#, (0, 0, 0)),
        ];
        for (p, want) in cases {
            assert_eq!(ok(p), want, "{p}");
        }
    }

    #[test]
    fn boundaries() {
        let e =
            |h, m, s| parse_tick(format!(r#"{{"hour":{h},"minute":{m},"second":{s}}}"#).as_bytes());
        assert!(e(23, 59, 59).is_ok());
        assert_eq!(e(24, 0, 0), Err(TickError::OutOfRange(Field::Hour)));
        assert_eq!(e(0, 60, 0), Err(TickError::OutOfRange(Field::Minute)));
        assert_eq!(e(0, 0, 60), Err(TickError::OutOfRange(Field::Second)));
        assert_eq!(e(0, 0, 256), Err(TickError::OutOfRange(Field::Second)));
        assert_eq!(
            e(0, 0, 99_999_999_999_999u64),
            Err(TickError::OutOfRange(Field::Second))
        );
    }

    #[test]
    fn malformed() {
        let cases: [(&str, TickError); 14] = [
            ("", TickError::Syntax),
            ("garbage", TickError::Syntax),
            ("{}", TickError::Syntax),
            (r#"{"hour":1,"minute":2}"#, TickError::Syntax),
            (r#"{"hour":1,"minute":2,"second":3"#, TickError::Syntax),
            (r#"{"hour":1,"minute":2,"second":3}x"#, TickError::Syntax),
            (r#"{"hour":-1,"minute":2,"second":3}"#, TickError::Syntax),
            (r#"{"hour":1.0,"minute":2,"second":3}"#, TickError::Syntax),
            (r#"{"hour":01,"minute":2,"second":3}"#, TickError::Syntax),
            (r#"{"hour":"1","minute":2,"second":3}"#, TickError::Syntax),
            (r#"{"hour":1,"minute":2,"second":3,}"#, TickError::Syntax),
            (r#"{"hour":1,"minute":2,"day":3}"#, TickError::UnknownField),
            (
                r#"{"hour":1,"hour":2,"second":3}"#,
                TickError::DuplicateField(Field::Hour),
            ),
            (
                r#"{"h\u006fur":1,"minute":2,"second":3}"#,
                TickError::Syntax,
            ),
        ];
        for (p, want) in cases {
            assert_eq!(parse_tick(p.as_bytes()), Err(want), "{p}");
        }
    }

    #[test]
    fn hostile_payloads() {
        assert_eq!(
            parse_tick(&[b' '; MAX_TICK_LEN + 1]),
            Err(TickError::TooLong)
        );
        assert_eq!(parse_tick(&[0xff; 4096]), Err(TickError::TooLong));
        assert_eq!(parse_tick(&[0xff, 0xfe, 0x00]), Err(TickError::Syntax));
        assert_eq!(parse_tick(b"\0"), Err(TickError::Syntax));
        let mut max = [b' '; MAX_TICK_LEN];
        let body = br#"{"hour":1,"minute":2,"second":3}"#;
        max[..body.len()].copy_from_slice(body);
        assert!(parse_tick(&max).is_ok());
    }

    #[test]
    fn try_from_and_render_equivalence() {
        let p: &[u8] = br#"{"hour":12,"minute":34,"second":56}"#;
        let t = TickTime::try_from(p).unwrap();
        let c = ClockColors::default();
        assert_eq!(render_tick(&t, &c, 10), render_time(12, 34, 56, &c, 10));
    }

    #[test]
    fn display_is_nonempty() {
        use core::fmt::Write;
        let mut s = heapless_free::Buf::default();
        write!(s, "{}", TickError::OutOfRange(Field::Hour)).unwrap();
        assert!(s.0.contains("hour"));
    }

    mod heapless_free {
        #[derive(Default)]
        pub struct Buf(pub std::string::String);
        impl core::fmt::Write for Buf {
            fn write_str(&mut self, s: &str) -> core::fmt::Result {
                self.0.push_str(s);
                Ok(())
            }
        }
    }

    #[test]
    fn freshness_and_encoding() {
        assert_eq!(encode_tick_ms(0), 1);
        assert_eq!(encode_tick_ms(5), 5);
        assert_eq!(encode_tick_ms(u64::MAX), u32::MAX);
        assert!(!tick_is_fresh(100, 0, 10_000));
        assert!(tick_is_fresh(10_001, 1, 10_000));
        assert!(!tick_is_fresh(10_002, 1, 10_000));
        assert!(tick_is_fresh(5, 10, 0));
    }

    #[test]
    fn prefill() {
        assert_eq!(prefill_or(None, "d"), "d");
        assert_eq!(prefill_or(Some(""), "d"), "d");
        assert_eq!(prefill_or(Some("  \t"), "d"), "d");
        assert_eq!(prefill_or(Some(" 1883 "), "d"), "1883");
        assert_eq!(prefill_or(Some("a b"), "d"), "a b");
    }

    fn oracle(bytes: &[u8]) -> Option<(u64, u64, u64)> {
        let v: serde_json::Value = serde_json::from_slice(bytes).ok()?;
        let o = v.as_object()?;
        if o.len() != 3 {
            return None;
        }
        Some((
            o.get("hour")?.as_u64()?,
            o.get("minute")?.as_u64()?,
            o.get("second")?.as_u64()?,
        ))
    }

    proptest! {
        #[test]
        fn never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..200)) {
            let _ = parse_tick(&bytes);
        }

        #[test]
        fn accepted_implies_oracle_agrees(
            bytes in proptest::collection::vec(
                prop::sample::select(b" {}\":,0123456789hourminutesecnd\n".to_vec()), 0..64)
        ) {
            if let Ok(t) = parse_tick(&bytes) {
                prop_assert_eq!(
                    oracle(&bytes),
                    Some((u64::from(t.hour()), u64::from(t.minute()), u64::from(t.second())))
                );
            }
        }

        #[test]
        fn structured_matches_oracle(
            h in 0u64..100, m in 0u64..100, s in 0u64..100,
            order in 0usize..6, sp in prop::sample::select(vec!["", " ", "\n"]),
        ) {
            let parts = [("hour", h), ("minute", m), ("second", s)];
            let perm = [[0,1,2],[0,2,1],[1,0,2],[1,2,0],[2,0,1],[2,1,0]][order];
            let mut out = std::string::String::from("{");
            for (n, &i) in perm.iter().enumerate() {
                if n > 0 { out.push(','); }
                out.push_str(&format!("{sp}\"{}\"{sp}:{sp}{}{sp}", parts[i].0, parts[i].1));
            }
            out.push('}');
            let valid = h < 24 && m < 60 && s < 60;
            prop_assert_eq!(oracle(out.as_bytes()), Some((h, m, s)));
            match parse_tick(out.as_bytes()) {
                Ok(t) => {
                    prop_assert!(valid);
                    prop_assert_eq!((u64::from(t.hour()), u64::from(t.minute()), u64::from(t.second())), (h, m, s));
                }
                Err(e) => {
                    prop_assert!(!valid || e == TickError::TooLong);
                }
            }
        }
    }
}
