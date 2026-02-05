# Testing Strategy: Wokwi Simulation and Hardware-in-the-Loop

This document outlines the implementation plan for a three-tier testing pyramid for the rustyfarian-rgb-clock project.

## Overview

```
┌─────────────────────────────────────────────────────────────────────┐
│                         Testing Pyramid                              │
├─────────────────────────────────────────────────────────────────────┤
│  Tier 3: Hardware-in-the-Loop (HIL)     [minutes]                   │
│  - Self-hosted runner on Raspberry Pi                               │
│  - Real ESP32-C6 with physical NeoPixel ring                        │
│  - RMT timing verification, integration tests                       │
├─────────────────────────────────────────────────────────────────────┤
│  Tier 2: Wokwi Simulation (WITL)        [seconds]                   │
│  - Simulated ESP32-C6 + NeoPixel ring                               │
│  - Boot verification, WiFi/MQTT connection                          │
│  - Visual screenshot capture                                         │
├─────────────────────────────────────────────────────────────────────┤
│  Tier 1: Host Unit Tests                [milliseconds]              │
│  - clock-core (time→LED mapping)                                    │
│  - ferriswheel effects (rainbow animation)                          │
│  - Pure Rust, no hardware dependencies                              │
└─────────────────────────────────────────────────────────────────────┘
```

## Implementation Phases

### Phase 1: Tier 1 Enhancement (Host Tests)

**Status:** Partially complete (clock-core exists)

**Tasks:**
1. Ensure `clock-core` has comprehensive unit tests for time-to-LED mapping
2. Add property-based tests for edge cases (hour 0/23, minute 59, etc.)
3. Add tests for color blending when hands overlap

**Files to create/modify:**
- `crates/clock-core/src/lib.rs` - Add more unit tests

### Phase 2: Wokwi Simulation (Tier 2)

**Goal:** Automated firmware testing in a simulated ESP32 environment.

#### 2.1 Wokwi Configuration Files

Create project root files:

**`wokwi.toml`**
```toml
[wokwi]
version = 1
firmware = "target/riscv32imac-esp-espidf/release/rustyfarian-rgb-clock"
elf = "target/riscv32imac-esp-espidf/release/rustyfarian-rgb-clock"
```

**`diagram.json`**
```json
{
  "version": 1,
  "author": "datenkollektiv",
  "editor": "wokwi",
  "parts": [
    {
      "type": "board-esp32-c6-devkitc-1",
      "id": "esp",
      "top": 0,
      "left": 0,
      "attrs": {}
    },
    {
      "type": "wokwi-neopixel-ring",
      "id": "ring1",
      "top": 200,
      "left": 50,
      "attrs": { "pixels": "12" }
    }
  ],
  "connections": [
    ["esp:10", "ring1:DIN", "green", ["h0"]],
    ["esp:GND.1", "ring1:GND", "black", ["h0"]],
    ["esp:3V3", "ring1:VCC", "red", ["h0"]]
  ],
  "serialMonitor": { "display": "auto" }
}
```

Note: Connection uses GPIO10 (the clock's NeoPixel pin from main.rs).

#### 2.2 Test Scenarios

Create `wokwi/` directory with automation scenarios:

**`wokwi/test-startup.yaml`**
```yaml
name: Clock Startup Test
version: 1
steps:
  - wait-serial: "rustyfarian-rgb-clock"
    timeout: 10000
  - wait-serial: "Starting rainbow startup animation"
    timeout: 5000
  - screenshot:
      file: screenshots/rainbow-startup.png
  - wait-serial: "Setup complete"
    timeout: 60000
```

**`wokwi/test-rainbow-animation.yaml`**
```yaml
name: Rainbow Animation Test
version: 1
steps:
  - wait-serial: "Starting rainbow startup animation"
    timeout: 10000
  - delay: 2000
  - screenshot:
      file: screenshots/rainbow-frame-1.png
  - delay: 1000
  - screenshot:
      file: screenshots/rainbow-frame-2.png
```

#### 2.3 Firmware Logging Enhancement

Add strategic log messages for Wokwi scenario triggers.
Current logging is mostly enough, but may need:
- Add the "WS2812 driver initialized" log after driver creation
- Add "Display updated" log after `set_local_time()` completes

#### 2.4 GitHub Actions Workflow

Create `.github/workflows/wokwi.yml`:

```yaml
name: Wokwi Simulation

on:
  push:
    branches: [main, develop]
  pull_request:
    branches: [main]

jobs:
  wokwi-test:
    name: "Wokwi: Firmware Simulation"
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install ESP-RS toolchain
        uses: esp-rs/xtensa-toolchain@v1.5
        with:
          default: true
          ldproxy: true

      - name: Build firmware
        run: cargo build --release

      - name: Run Wokwi startup test
        uses: wokwi/wokwi-ci-action@v1
        with:
          token: ${{ secrets.WOKWI_CLI_TOKEN }}
          timeout: 120000
          scenario: wokwi/test-startup.yaml

      - name: Upload screenshots
        uses: actions/upload-artifact@v4
        if: always()
        with:
          name: wokwi-screenshots
          path: screenshots/
```

**Required secrets:**
- `WOKWI_CLI_TOKEN` - Obtain from https://wokwi.com/dashboard/ci

### Phase 3: Hardware-in-the-Loop (Tier 3)

**Goal:** Test on real hardware via self-hosted GitHub runner.

#### 3.1 Hardware Requirements

| Component               | Quantity  | Purpose                |
|:------------------------|:----------|:-----------------------|
| Raspberry Pi 4/5 (4GB+) | 1         | Self-hosted runner     |
| Powered USB 3.0 Hub     | 1         | Reliable MCU power     |
| ESP32-C6-DevKitC-1      | 1+        | Target device          |
| 12-LED NeoPixel Ring    | 1         | Test fixture           |
| Ethernet cable          | 1         | Reliable CI connection |

#### 3.2 Raspberry Pi Setup

1. Install Raspberry Pi OS Lite (64-bit)
2. Install Rust toolchain with ESP32 targets
3. Install probe-rs for flashing/debugging
4. Configure udev rules for stable device naming
5. Install GitHub Actions self-hosted runner

See `inbox/hil-testbed/README.md` for detailed setup instructions.

#### 3.3 udev Rules

Create `/etc/udev/rules.d/99-esp32-rgb-clock.rules`:

```
# ESP32-C6 DevKitC for rgb-clock testing
SUBSYSTEM=="tty", ATTRS{idVendor}=="303a", ATTRS{idProduct}=="1001", \
    SYMLINK+="esp-c6-clock", MODE="0666", GROUP="plugdev"

# probe-rs debug interface
SUBSYSTEM=="usb", ATTRS{idVendor}=="303a", MODE="0666", GROUP="plugdev"
```

#### 3.4 Integration Tests

Create `tests/integration.rs` using `embedded-test` crate:

```rust
#![no_std]
#![no_main]

use embedded_test::test;

#[test]
fn test_led_driver_init() {
    // Test that WS2812 driver initializes without error
}

#[test]
fn test_rainbow_effect_renders() {
    // Test that rainbow animation produces valid RGB output
}

#[test]
fn test_time_display() {
    // Test that set_local_time correctly lights LEDs
}
```

#### 3.5 HIL Workflow

Create `.github/workflows/hil.yml`:

```yaml
name: Hardware-in-the-Loop

on:
  workflow_run:
    workflows: ["Wokwi Simulation"]
    types: [completed]
    branches: [main]
  workflow_dispatch:

concurrency:
  group: hil-testbed
  cancel-in-progress: false

jobs:
  hil-tests:
    name: "HIL: ESP32-C6"
    runs-on: [self-hosted, rpi, esp32]
    if: ${{ github.event.workflow_run.conclusion == 'success' || github.event_name == 'workflow_dispatch' }}
    steps:
      - uses: actions/checkout@v4

      - name: Check device availability
        run: probe-rs list

      - name: Build test firmware
        run: cargo build --release

      - name: Flash and run integration tests
        run: |
          probe-rs run --chip esp32c6 \
            target/riscv32imac-esp-espidf/release/rustyfarian-rgb-clock
        timeout-minutes: 5

      - name: Run embedded tests
        run: |
          cargo test --target riscv32imac-esp-espidf \
            --test integration -- --test-threads=1
```

## Implementation Checklist

### Phase 1: Host Tests

- [ ] Review and expand clock-core unit tests
- [ ] Add edge case tests for time conversion
- [ ] Ensure all tests pass: `cargo test -p clock-core`

### Phase 2: Wokwi

- [ ] Create `wokwi.toml` configuration
- [ ] Create `diagram.json` circuit definition
- [ ] Create `wokwi/test-startup.yaml` scenario
- [ ] Create `wokwi/test-rainbow-animation.yaml` scenario
- [ ] Add WOKWI_CLI_TOKEN secret to the repository
- [ ] Create `.github/workflows/wokwi.yml`
- [ ] Add strategic log messages for test triggers
- [ ] Test locally with Wokwi VS Code extension
- [ ] Verify CI pipeline runs successfully

### Phase 3: HIL

- [ ] Set up Raspberry Pi with required software
- [ ] Configure udev rules for ESP32-C6
- [ ] Install and configure GitHub Actions runner
- [ ] Connect ESP32-C6 + NeoPixel ring test fixture
- [ ] Add `embedded-test` dependency
- [ ] Create `tests/integration.rs`
- [ ] Create `.github/workflows/hil.yml`
- [ ] Test runner connectivity
- [ ] Verify full CI pipeline

## Dependencies to Add

```toml
# Cargo.toml (dev-dependencies for HIL tests)
[dev-dependencies]
embedded-test = "0.4"
defmt = "0.3"
defmt-rtt = "0.4"
```

## CI Pipeline Flow

```
Push/PR
   │
   ▼
┌─────────────────────┐
│  Tier 1: Host Tests │  ubuntu-latest, ~30s
│  - cargo fmt        │
│  - cargo clippy     │
│  - cargo test       │
└─────────────────────┘
   │ (pass)
   ▼
┌─────────────────────┐
│  Tier 2: Wokwi      │  ubuntu-latest, ~2min
│  - Build firmware   │
│  - Run scenarios    │
│  - Capture screens  │
└─────────────────────┘
   │ (pass, main branch only)
   ▼
┌─────────────────────┐
│  Tier 3: HIL        │  self-hosted rpi, ~5min
│  - Flash device     │
│  - Integration test │
│  - Collect logs     │
└─────────────────────┘
```

## Resources

- [Wokwi CI Documentation](https://docs.wokwi.com/wokwi-ci/getting-started)
- [embedded-test crate](https://github.com/probe-rs/embedded-test)
- [probe-rs documentation](https://probe.rs/docs/)
- [GitHub Self-hosted Runners](https://docs.github.com/en/actions/hosting-your-own-runners)

## Notes

- Wokwi simulation requires a CLI token (free tier available)
- HIL testing requires physical hardware investment
- Consider starting with Wokwi (Phase 2) as it requires no hardware
- HIL tests should only run on the main branch to limit hardware contention
