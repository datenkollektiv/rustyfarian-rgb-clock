# ESP32 C6 RGB Clock

An ESP32-C6 RGB LED clock that displays time using 12 WS2812 NeoPixel LEDs arranged in a clock face. Time is received via MQTT from an external source.

> Note: Parts of this library were developed with the assistance of AI tools.
> All generated code has been reviewed and curated by the maintainer.

## Quick Start

Build

```shell
cargo build --release
```

Flash and monitor (requires espflash)

```shell
cargo run --release
```

Requires a `.env` file with Wi-Fi and MQTT credentials (see `.env.example`).

Since the default flash size of 1MB may be insufficient:

```shell
cargo espflash flash --partition-table partitions.csv --monitor
```

## Dependencies

This project uses external crates from companion repositories:

| Crate | Repository | Description |
|-------|------------|-------------|
| `led-effects` | [rustyfarian-ws2812](https://github.com/datenkollektiv/rustyfarian-ws2812) | LED animation effects (no_std) |
| `ws2812-core` | [rustyfarian-ws2812](https://github.com/datenkollektiv/rustyfarian-ws2812) | WS2812 color utilities (no_std) |
| `esp32-ws2812-rmt` | [rustyfarian-ws2812](https://github.com/datenkollektiv/rustyfarian-ws2812) | ESP32 RMT driver for WS2812 |
| `esp32-wifi-manager` | [rustyfarian-network](https://github.com/datenkollektiv/rustyfarian-network) | WiFi connection management |
| `esp32-mqtt-manager` | [rustyfarian-network](https://github.com/datenkollektiv/rustyfarian-network) | MQTT client with callbacks |

## Project Structure

```text
rustyfarian-rgb-clock/           # This repository
├── src/                         # Application code
│   ├── main.rs                  # Entry point, Wi-Fi/MQTT setup
│   └── rgb_clock.rs             # Clock display logic
└── crates/
    └── clock-core/              # Pure Rust clock utilities (testable)
```

### Local Development

For developing alongside the external crates, `.cargo/config.toml` contains `[patch]` sections that redirect git dependencies to sibling directories:

```toml
[patch."https://github.com/datenkollektiv/rustyfarian-ws2812"]
led-effects = { path = "../rustyfarian-ws2812/crates/led-effects" }
# ... etc
```

Comment out the patches to build against the published GitHub repos.

## License

MIT
