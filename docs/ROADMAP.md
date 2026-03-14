# Roadmap

*Last updated: March 2026*

This project validates a three-tier embedded testing pyramid for the rustyfarian ecosystem.
The RGB clock is a stable test fixture — feature work lives in other projects.
Following a vision review (March 2026), the focus is on completing all three testing tiers
with documentation good enough for other projects to adopt the approach.

```mermaid
%%{init: {
  "theme": "base",
  "themeVariables": {
    "cScale0": "#c8f7c5",
    "cScaleLabel0": "#1b5e20",
    "cScale1": "#fff3cd",
    "cScaleLabel1": "#7a5a00",
    "cScale2": "#e3f2fd",
    "cScaleLabel2": "#0d47a1"
  }
}}%%

timeline
    title Rustyfarian RGB Clock Roadmap

    Near term : Tier 1 — Expand clock-pure host tests (done)
              : CI pipeline split and cargo-deny (done)
              : Dependency alignment — esp-idf-hal 0.46, MqttBuilder (done)
              : Tier 2 — Wokwi simulation (done)

    Mid term  : Tier 3 — Hardware-in-the-Loop on Raspberry Pi (after Tier 1)
              : Testing playbook documentation

    Long term : Cross-project adoption validation
              : Raw RGB data via MQTT (testing vehicle)
```

See [testing-strategy.md](testing-strategy.md) for the full three-tier implementation plan.
