# Rustyfarian RGB Clock Roadmap

## Planned

### Three-Tier Testing Strategy

Continue expanding the testing pyramid.
See [testing-strategy.md](testing-strategy.md) for the full implementation plan.

**Remaining phases:**
- **Tier 1: Host Tests** — Expand clock-core unit tests (low effort)
- **Tier 3: Hardware-in-the-Loop** — Self-hosted runner with real hardware (high effort)

## Ideas

- Support MQTT messages with 12 RGB (plus hue) values to show raw data on the clock.

<details>
<summary><strong>Completed</strong></summary>

- **Tier 2: Wokwi Simulation** — Automated firmware testing in simulated ESP32 (v0.1.0)

</details>
