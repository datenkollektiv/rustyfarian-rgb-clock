---
gate: hardware
gate-note: Judge the gamma table and soft-mode base brightness by eye on the real C6 and C3 rings.
desk-work: available
---

# Feature: Soft Hands v1

Each hand spreads its brightness across the two LEDs that bracket its true position, instead of lighting one LED.
Hours advance per minute, minutes per minute, seconds per second.
Example: minute 7 lights the 1 o'clock LED at 60% and the 2 o'clock LED at 40%.
Justified against the `VISION.md` non-goal as richer pure logic for `clock-pure` and a harder exercise of the ws2812 path.

## Decisions

| Decision | Reason | Rejected Alternative |
|:---------|:-------|:---------------------|
| One render entry point with `RenderMode { Sharp, Soft }` | One code path for both modes, no drift | Separate `render_time_soft` function |
| All three hands spill over | Consistent look; each hand at its finest step | Sharp second hand |
| Split perceived brightness through a small gamma lookup table | 40/60 must look like 40/60 at low values | Linear split of raw values |
| Soft mode uses a higher base brightness than `DEFAULT_BRIGHTNESS` | Brightness 10 leaves ~10 levels to share across two LEDs | Keep brightness 10 |
| Mode selected by a build-time `.env` key, like the portal prefills | No new runtime surface, no extra CI build variant | MQTT command; cargo feature |
| Overlapping hands keep saturating colour addition | Identical blending rule to sharp mode | Hand priority or masking |
| One frame per tick | Pure function of `(h, m, s)`, no frame loop | Cross-fade between ticks (open question) |
| Own gamma 2.0 table in `clock-pure`, built by a `const fn` | No dependency; one function to retune at the hardware gate | `smart-leds` gamma |
| A share below one output step rounds to 0 | A clamp to 1 leaves a permanent ghost LED beside the hour hand | Clamp non-zero shares to 1 |
| `.env` key `CLOCK_HANDS` = `sharp` or `soft`, parsed by `clock_pure::parse_hands` | build.rs guards (build-dependency) and firmware share one parser | `build_support/` `#[path]` copy |
| `SOFT_BRIGHTNESS = 24`, provisional | At 10 the 5-step hands quantise to 0/1/2/4/6 | Reuse `DEFAULT_BRIGHTNESS` |

## Constraints

- `clock-pure` stays `no_std`, integer-only, and allocation-free.
- Sharp mode stays the default and byte-identical, so Wokwi, bench and `render_time` expectations do not change.
- Both tiers (IDF C6 and esp-hal C3) get the mode through the shared `clock-app`.
- A hand's total perceived brightness stays constant across its sweep.
- The wrap from LED 11 (12 o'clock) to LED 0 (1 o'clock) is seamless.
- No further clock features ride on this one.

## Open Questions

- [ ] Should the light cross-fade between ticks, which needs a frame loop between MQTT ticks?
- [ ] Is `SOFT_BRIGHTNESS = 24` right on the real rings, given a lone soft hand peaks at 2.4× a sharp one?
- [ ] Is the mid-sweep dip acceptable (50/50 gives about 6 per LED against 24 at the endpoints)?
- [ ] Do a gliding hour hand and blended overlaps still read clearly (see lore "Clock Display")?
- [ ] Does soft mode deserve its own Wokwi scenario?

## State

- [x] Design approved
- [x] Core implementation
- [x] Tests passing
- [ ] Documentation updated

## Session Log

- 2026-11-15 — Feature doc created via /feature dialog
- 2026-11-15 — Implemented via workflow; `verify`, `verify-fw`, `verify-hal` green; hardware gate open
