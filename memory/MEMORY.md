# Project Memory

This file is the persistent memory for the rustyfarian-rgb-clock project.
Claude reads it automatically before starting any task.

Keep all project-related context here:
- Key architectural decisions and the reasoning behind them
- Conventions and patterns established in this codebase
- Non-obvious facts that caused surprising failures (cross-reference `docs/project-lore.md`)
- Active constraints (performance budgets, compatibility requirements, known limitations)
- In-progress work and current state when a session ends mid-task

## Conventions

- `just verify` is the standard pre-task check (read-only: format-check + clippy + clock-pure tests)
- `just pre-commit` runs the modifying version (formats files before checking)
- Never invoke `cargo` directly for build/check/test — use `just` recipes
- Error handling: `anyhow::Result` + `.context()` in firmware; manual `core::fmt::Display` in `clock-pure`
- `clock-pure` is `#![no_std]` — no heap allocation, no `std` types

## Key Context

- ESP32-C6 (RISC-V) target: `riscv32imac-esp-espidf` — standard RISC-V toolchain, no Xtensa fork needed
- 12 WS2812 NeoPixels on a clock face; LED 0 = 1 o'clock position, LED 11 = 12 o'clock position
- Time arrives via MQTT `tick` topic as `{"hour":H,"minute":M,"second":S}` (24-hour)
- Local development uses path patches in `.cargo/config.toml` to point at sibling repos
  (rustyfarian-ws2812, rustyfarian-network) — comment out patches to build against git refs
- `AGENTS.md` is the committed, public cross-tool guide

## Lore Index

`memory/MEMORY.md` is an index only — `docs/project-lore.md` is the source of truth for each entry.

- MQTT subscribe-in-callback deadlock (`esp-idf-svc 0.52+`) — see `docs/project-lore.md` "MQTT & Networking"
- Wokwi `save-to` path resolves relative to the scenario file — see `docs/project-lore.md` "Wokwi Simulation"
