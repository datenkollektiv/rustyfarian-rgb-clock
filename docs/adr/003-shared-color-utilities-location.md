# ADR-003: Shared Color Utilities in led-effects Crate

## Status

Proposed

## Context

Color manipulation functions like `scale_color()` and `add_colors()` are currently implemented in the `clock-core` crate of `rustyfarian-rgb-clock`.
These functions are generic utilities not specific to clock functionality:

```rust
pub fn scale_color(color: RGB8, brightness: f32) -> RGB8;
pub fn add_colors(a: RGB8, b: RGB8) -> RGB8;  // saturating add
```

As more devices join the Rustyfarian family (Knob, future projects), each would need these same utilities.
Duplicating them violates DRY and creates maintenance burden.

Candidate locations:
1. **ws2812-pure** - Low-level WS2812 protocol utilities
2. **led-effects** - LED animation and display effects
3. **New crate** - Dedicated color math crate

## Decision

Move shared color utilities to the `led-effects` crate in `rustyfarian-ws2812`.

The `clock-core` crate will either:
- Depend on `led-effects` and re-export the functions, or
- Remove the functions and update callers to use `led-effects` directly

New color utilities (like `hsv_to_rgb`) will also live in `led-effects`.

## Consequences

### Positive

- Single source of truth for color math
- All Rustyfarian LED projects can share the same utilities
- `led-effects` becomes a cohesive "LED toolkit" crate
- Easier to test and maintain color functions in one place

### Negative

- `clock-core` loses some self-containment (may need dependency on `led-effects`)
- Slightly larger dependency graph for projects that only need basic color math
- Migration effort required for existing code

### Neutral

- `ws2812-pure` remains focused on protocol-level concerns (RGB to GRB, bit encoding)
- Clear separation: `ws2812-pure` = protocol, `led-effects` = visual effects and color math
- `clock-core` remains focused on time-to-LED-index mapping (truly clock-specific logic)
