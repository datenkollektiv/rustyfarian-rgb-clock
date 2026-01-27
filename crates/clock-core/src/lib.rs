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
/// use clock_core::hour_to_index;
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
/// use clock_core::minute_to_index;
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
/// use clock_core::second_to_index;
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
/// use clock_core::scale_color;
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
/// use clock_core::add_colors;
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
}
