# ADR-002: Integer-Only HSV to RGB Conversion

## Status

Proposed

## Context

Rainbow and other hue-based effects require HSV (Hue, Saturation, Value) to RGB conversion.
The standard algorithm uses floating-point arithmetic.

ESP32 chips have varying floating-point support:
- ESP32-S3: Hardware FPU for single precision
- ESP32-C6: No hardware FPU (RISC-V without F extension)
- ESP32-C3: No hardware FPU

Floating-point on chips without FPU requires software emulation, which is slow and increases binary size.

## Decision

Implement HSV to RGB conversion using integer-only arithmetic.

The function signature will be:

```rust
/// Convert HSV to RGB using integer math.
///
/// - `hue`: 0-359 degrees
/// - `saturation`: 0-255
/// - `value`: 0-255 (brightness)
pub fn hsv_to_rgb(hue: u16, saturation: u8, value: u8) -> RGB8;
```

The implementation will use:
- Fixed-point arithmetic with appropriate scaling
- Lookup tables where beneficial
- `u16` or `u32` intermediate values to prevent overflow

## Consequences

### Positive

- Consistent performance across all ESP32 variants
- Smaller binary size (no soft-float library needed)
- Predictable execution time (important for timing-sensitive LED protocols)
- Works correctly on RISC-V chips like ESP32-C6

### Negative

- Slightly more complex implementation than floating-point version
- Minor precision loss compared to floating-point (imperceptible for 8-bit RGB)
- Requires careful overflow handling in intermediate calculations

### Neutral

- Standard technique in embedded graphics and LED libraries
- Well-documented algorithms available (e.g., from FastLED, Adafruit NeoPixel)
