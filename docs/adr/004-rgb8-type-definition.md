# ADR-004: RGB8 Type Definition in ws2812-pure

## Status

Proposed

## Context

RGB color values are used throughout the Rustyfarian LED ecosystem.
Currently, different crates use different representations:

| Crate         | Current Representation                    |
|---------------|-------------------------------------------|
| `clock-core`  | `(u8, u8, u8)` tuple                      |
| `ws2812-pure` | `(u8, u8, u8)` tuple                      |
| `led-effects` | `RGB8` from `smart_leds` crate (optional) |

This inconsistency causes:
- Type conversion boilerplate between crates
- Unclear ownership of the "canonical" RGB type
- Optional dependency on external `smart_leds` crate

Options considered:
1. **Use `smart_leds::RGB8`** everywhere - adds external dependency
2. **Keep tuples** - no type safety, unclear field order (RGB vs BGR)
3. **Define `RGB8` in ws2812-pure** - own the type, re-export everywhere

## Decision

Define a canonical `RGB8` struct in `ws2812-pure` and re-export it from `led-effects`.

```rust
// In ws2812-pure/src/lib.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RGB8 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl RGB8 {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}
```

The `led-effects` crate will re-export:

```rust
// In led-effects/src/lib.rs
pub use ws2812_pure::RGB8;
```

## Consequences

### Positive

- Single canonical RGB type across all Rustyfarian crates
- Type safety: `RGB8` is distinct from arbitrary tuples
- Named fields prevent RGB vs BGR confusion
- No external dependencies for basic color type
- `const fn` constructor enables compile-time color definitions
- Derives enable easy comparison, debugging, and copying

### Negative

- Breaking change for code using tuples (migration required)
- `clock-core` must depend on `ws2812-pure` or `led-effects`
- Slight increase in API surface for `ws2812-pure`

### Neutral

- Follows pattern used by `smart_leds` and similar crates
- `ws2812-pure` is the natural home (lowest level, protocol-focused)
- Future: could add `From<(u8, u8, u8)>` for easy migration
