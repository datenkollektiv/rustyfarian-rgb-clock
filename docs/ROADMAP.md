# Rustyfarian RGB Clock Roadmap

## Planned

### Three-Tier Testing Strategy

Implement a comprehensive testing pyramid for the project.
See [testing-strategy.md](testing-strategy.md) for the full implementation plan.

**Phases:**
1. **Tier 1: Host Tests** - Expand clock-core unit tests (low effort)
2. **Tier 2: Wokwi Simulation** - Automated firmware testing in simulated ESP32 (medium effort)
3. **Tier 3: Hardware-in-the-Loop** - Self-hosted runner with real hardware (high effort)

**Benefits:**
- Catch regressions early with fast host tests
- Validate firmware behavior without physical hardware (Wokwi)
- Verify real-world timing and hardware integration (HIL)

## Ideas

- Support MQTT messages with 12 RGB (plus hue) values to show raw data on the clock.
