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
