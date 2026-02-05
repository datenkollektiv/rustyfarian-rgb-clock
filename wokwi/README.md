# Wokwi Test Scenarios

This directory contains automation scenarios for Wokwi CI testing.

## Scenarios

### test-startup.yaml

Verifies the firmware boots correctly:
- ESP-IDF initialization completes
- Rainbow startup animation begins
- Main setup completes without errors

### test-rainbow-frames.yaml

Captures multiple frames of the rainbow animation:
- Verifies the animation is actually progressing
- Produces visual artifacts for inspection

## Running Locally

### VS Code Extension

1. Install the [Wokwi for VS Code](https://marketplace.visualstudio.com/items?itemName=Wokwi.wokwi-vscode) extension
2. Build the firmware: `cargo build --release`
3. Press F1 and select "Wokwi: Start Simulator"
4. The simulation uses `diagram.json` in the project root

### Wokwi CLI

```shell
# Install Wokwi CLI
curl -L https://wokwi.com/ci/install.sh | sh

# Set your token (get from https://wokwi.com/dashboard/ci)
export WOKWI_CLI_TOKEN=your_token_here

# Run a scenario
wokwi-cli --scenario wokwi/test-startup.yaml
```

## Screenshots

Test runs produce screenshots in the `screenshots/` directory.
These are uploaded as GitHub Actions artifacts for inspection.

## Circuit Description

The `diagram.json` defines:
- ESP32-C6-DevKitC-1 board
- 12-LED NeoPixel ring on GPIO10 (clock display)
- Status LED on GPIO8 (onboard RGB LED position)

This matches the physical hardware configuration.
