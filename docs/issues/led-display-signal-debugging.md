# LED Display / WS2812 Signal Debugging (in progress)

A working scratchpad for an unresolved LED-display issue.
This lives outside `docs/project-lore.md` on purpose: lore is for **settled** facts,
and this investigation is still open.
Promote confirmed findings into `docs/project-lore.md` once they are proven, then
trim them from here.

_Status: open. Last updated during a debugging session; continue from "Open question" below._

## Hardware setup

- ESP32-C6, firmware in this repo. 12 WS2812 NeoPixels on a ring, DIN on the rustyfarian standard data pin per chip: **GPIO 18** on the ESP32-C6, **GPIO 4** on the ESP32-C3.
- Historical setup: firmware before the 2026-09-27 pin change drove DIN on **GPIO 10** on both chips; the observations below were made with that wiring.
- The LED strip is powered from the **ESP32 5 V pin**; DIN is driven directly by the 3.3 V GPIO.
- Onboard status LED on GPIO 8 (separate `Ws2812Rmt`).
- `DEFAULT_BRIGHTNESS = 10` (`src/rgb_clock.rs`) — colors are very dim.
- Hand colors: hour = blue `(0,0,1)`, minute = green `(0,1,0)`, second = red `(1,0,0)`.
- Time arrives via MQTT `tick` as `{"hour":H,"minute":M,"second":S}`, ~every 5 s.

## Reference facts (verified, reusable)

- **Display logic is correct and not the cause.** `RGBClock::set_local_time()` calls
  `clear()` (all 12 pixels → black) then sets the 3 hands then `show()`, which transmits
  the **full 12-pixel buffer every tick**. The previous second-LED is always explicitly
  rewritten to `(0,0,0)`. So any *stale* or *wrong* color is a transmit/reception/electrical
  failure, not firmware state.
- **Wire order is G-R-B.** The driver (`rustyfarian-esp-idf-ws2812`, via `bunting::rgb_to_grb`)
  sends bytes Green, Red, Blue per pixel. This lets you localize a failing channel to one byte
  by comparing a standalone hand against its blends.
- **Index mapping** (`clock-pure`): `hour_to_index(h) = (h+11)%12`,
  `minute_to_index(m) = (m+55)%60/5`, `second_to_index` identical to minutes.
- **RMT timing in the driver is spec-correct** (T0H 350 / T0L 800, T1H 700 / T1L 600 ns;
  default `memory_block_symbols: 48`, indirect/ping-pong refill — 288 symbols/frame).

## Timeline of observations

1. **First report** — ticks `10:10:20→35` (hour 10 → LED9 blue, minute 10 → LED1 green,
   second 20/25/30/35 → LED 3/4/5/6 red). User saw "two red LEDs + one very dim red."
   → The red second-hand left a 1–2 tick **trailing ghost** on the LED it just left.
2. **Second report** — tick `10:26:55` (LED9 blue, LED4 green, LED10 red); just before it,
   seconds 50–54 → LED9, i.e. the red second-hand swept *onto* the blue hour LED → "violet
   collision," then left a leftover red on LED9 at :55 ("two reds"). Same trailing-ghost
   mechanism, more visible because the trailed LED is the blue hour.
3. **Hypothesis at that point:** sub-spec data level. ESP drives DIN at 3.3 V, but a 5 V WS2812
   needs `V_IH ≥ 0.7 × VDD = 3.5 V`. 3.3 V is below spec → intermittent mis-latch = ghosting.
4. **After a hardware change (this is the gap — confirm what was changed):** ghosting/instability
   **gone** ("stable"). New steady symptom instead: **standalone red vanished**, "duplicated blue
   and green," and **yellow appears where the red second-hand overlaps green**.

5. **2026-09-27, ESP32-C3 on GPIO 4, WS2812 strip, same firmware branch as the C6** — ticks
   verified clean on the broker (one publisher, `{"hour":21,"minute":24,"second":20}` every 5 s).
   Rainbow rendered correctly, then the clock showed the green minute hand **twice, two LEDs
   apart with an unlit LED between** (LED 3 real, LED 5 phantom); both turned yellow when the
   red second-hand walked over them. The phantom vanished for exactly one 5 s tick whenever
   the red sat two LEDs *before* the real green, reproducibly across a reset, and after a few
   minutes the display settled into a correct, stable clock with no phantom — and a few
   minutes after that the **green minute hand vanished entirely** while red and blue kept
   rendering, so the fault wanders between phantom and dropped pixels on the same rig.
   → Answers question 2 below: literally two LEDs at once, offset by two positions, not
   adjacent and not an R-byte loss; content-dependent and self-healing, which fits a marginal
   data level / edge at the first LED (3.3 V DIN into a 5 V-powered strip) rather than firmware.
   A phantom two pixels later is 48 bits, the same size as the RMT `memory_block_symbols`
   refill chunk — coincidence or not, worth keeping in view if it recurs on a level-shifted rig.

## Open question (resume here)

The new steady symptom fits an **R (middle) byte not latching cleanly**:
- standalone red `(10,0,0)` → GRB `[0,10,0]` → R byte lost → `[0,0,0]` = **dark** ("red vanished")
- blue `(0,0,10)` → `[0,0,10]` and green `(0,10,0)` → `[10,0,0]` → R byte already 0 → unaffected
- red over green `(10,10,0)` → `[10,10,0]` → R survives when G≠0 → **yellow** still shows

This points at a **per-pixel bit-timing / signal-edge** problem (e.g. slow edges from a newly
added level shifter, or a too-large DIN series resistor rounding the data edge), not firmware.
Still unconfirmed.

### Two questions for the user
1. **What exactly was changed** in the hardware round that made it "stable" (series diode on the
   strip +5 V, a level shifter — which chip? — a DIN resistor, rewiring)?
2. **"Duplicated blue and green"** — literally two blue LEDs and two green LEDs at once? At which
   positions (adjacent vs opposite)? This separates an "R-byte failure" from a "one-pixel shift."

### Next diagnostic step (proposed, not yet built)
Add a **static boot self-test**, gated behind a `just`-flashable flag so it never ships in normal
builds: light LED0 = pure red, LED1 = green, LED2 = blue, LED3 = white, rest off, hold a few seconds
before starting the clock. This decouples channel/position questions from the clock + MQTT logic:
- LED0 dark while LED3 shows no red tint → confirms the R-byte/timing theory.
- Colors on the wrong LEDs → one-pixel data shift instead.

## Candidate fixes (electrical — no firmware fix exists for a sub-spec logic level)
- Series silicon diode on the strip's +5 V (`1N4001` → ~4.3 V; `V_IH` ≈ 3.0 V, under 3.3 V data).
  Use silicon (~0.6–0.7 V drop), not Schottky (~0.3 V leaves 4.7 V, still marginal).
- Proper 3.3→5 V level shifter on DIN (`74AHCT125` / `SN74LVC1T45`).
- ~330 Ω series resistor at DIN tames edge reflections (pair with either of the above; too large
  rounds the edges and can itself cause mis-latch).
- Keep the data wire short; ensure a solid common ground between ESP and strip.
- At higher brightness, power the strip from a separate 5 V supply (12 LEDs full-white ≈ 700 mA,
  more than the ESP 5 V/USB pin should source).
