# ADR-001: Caller-Provided Buffer for Animation Output

## Status

Proposed

## Context

The `led-effects` crate needs to output RGB color data for multiple LEDs.
Different devices have different LED counts (RGB Clock: 12, Knob: 5, future devices: unknown).
The crate must remain `no_std` compatible for embedded use.

Two approaches were considered:

1. **Internal buffer** using `heapless::Vec<RGB8, N>` with a compile-time maximum size
2. **Caller-provided buffer** where the caller passes a mutable slice

Internal buffers would simplify the API but require choosing a maximum size constant.
This creates awkward trade-offs: too small limits future devices, too large wastes memory on small devices.

## Decision

Animation effects will use caller-provided buffers.

The API pattern will be:

```rust
fn update(&mut self, buffer: &mut [RGB8]) -> Result<(), EffectError>;
```

The caller is responsible for:
- Allocating a buffer of the correct size (stack or static)
- Passing the buffer to each `update()` call
- Handling the `EffectError` if buffer size mismatches

## Consequences

### Positive

- No heap allocation required, fully `no_std` compatible
- No arbitrary maximum size constants
- Matches existing `rustyfarian-esp-idf-ws2812` API (`set_pixels_slice(&[RGB8])`)
- Caller has full control over memory layout
- Works efficiently with any LED count

### Negative

- Slightly more verbose API (caller must declare buffer)
- Runtime error possible if buffer size mismatches effect configuration
- Caller must ensure buffer lifetime spans the update call

### Neutral

- Consistent with Rust embedded ecosystem conventions
- Similar to how `std::io::Read` works with caller-provided buffers
