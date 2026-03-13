# Project Vision

## North Star

Validate a replicable three-tier embedded testing pyramid (host tests, Wokwi simulation, hardware-in-the-loop) using a working RGB clock as the test fixture, so that future rustyfarian projects can adopt the approach with confidence.

## Long-Term Goals

- All three testing tiers running green in CI for the RGB clock firmware.
- Documentation and templates good enough that setting up the same testing pyramid in a new rustyfarian project is straightforward.
- Drive maturation of the shared rustyfarian crates (`led-effects`, `ferriswheel`, `ws2812-pure`, `rustyfarian-esp-idf-mqtt`, etc.) through real usage and test coverage.
- Build transferable knowledge in ESP32/Rust embedded development — patterns, toolchain setup, and workflows that carry forward to future projects.

## Target Beneficiaries

- **Ourselves** — learning and preparation for future embedded Rust projects.
- **Future rustyfarian projects** — they inherit a proven testing approach and mature shared crates.
- **Other hobbyists** (secondary) — the crates and documentation may be useful to others building ESP32 projects in Rust.

## Non-Goals

- This project is not a feature-rich clock product. New features and applications belong in other rustyfarian projects.
- Chasing exhaustive hardware compatibility (other boards, LED types) is out of scope.
- Building a general-purpose embedded testing framework — the goal is a replicable *approach*, not a library.

## Success Signals

- A CI run exercises all three tiers and produces a green badge.
- A new rustyfarian project can stand up its own testing pyramid by following documentation from this project, without reverse-engineering the setup.
- The shared crates used by the clock are well-tested enough that upstream changes are caught before they break dependent projects.

## Open Questions

- What is the right level of documentation for the HIL tier — a step-by-step setup guide, or a more general playbook with decision points?
- Should the testing playbook live in this repo or in a shared location (e.g., a rustyfarian-docs repo)?

## Vision History

- 2026-03-13 — Initial vision created. Established that the project's primary purpose is validating the three-tier testing pyramid for ecosystem-wide adoption, not building clock features.
