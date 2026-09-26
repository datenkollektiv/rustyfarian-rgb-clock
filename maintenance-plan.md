# Maintenance Plan

Regular maintenance workbook for `rustyfarian-rgb-clock` — the ESP32-C6 RGB clock firmware and the `clock-pure` host-testable library.
Because the clock is the **integration test fixture** of the rustyfarian workspace, most cycles are triggered by an upstream release of `rustyfarian-ws2812` or `rustyfarian-network` rather than by the calendar.

Covers: build verification, the upstream release-wave runbook, toolchain pins, security scanning, CI/CD, and hardware validation.

<details>
<summary><strong>Build &amp; Test</strong></summary>

### Primary build gate

- `just verify` — fmt-check, clippy, and `clock-pure` tests (non-modifying).
- `just ci` — the CI-equivalent chain: fmt-check, deny, check, lint, test.

### Firmware compile checks

- `just check` / `just build` — firmware for `idf_c6_rgb_clock` (ESP-IDF, `riscv32imac-esp-espidf`).
- `just clippy` — clippy on the firmware target.
- `just lock-ci` — resolve dependencies the way CI does, without the local `[patch]` redirects in `.cargo/config.toml`.
  Run this after every pin change: local builds use the sibling working trees, CI uses the published crates and the git pin.

### Hardware test

`just run` flashes the C6 and opens the monitor.
Pass criteria:

- Boot reaches Wi-Fi + MQTT (or the SoftAP portal on a blank NVS) without panic or watchdog reset.
- The clock shows hour, minute, and second hands after the first MQTT time sync.
- A 60-second run shows no flicker or random flashes (see lore "Hardware" before blaming firmware).

</details>

<details>
<summary><strong>Dependency Updates</strong></summary>

### Where versions live

- Root `Cargo.toml` `[workspace.dependencies]` — every shared version, with a comment on each coordination constraint.
- `rust-toolchain.toml` **and** `.github/workflows/rust.yml` — the pinned nightly; both sites move together.
- `Cargo.lock` is git-ignored; `just update` refreshes in-range transitives.

### Upstream release-wave runbook

The rustyfarian crates share `pennant` (`StatusLed`).
`rustyfarian-esp-idf-ws2812`, `ferriswheel`, and `rustyfarian-esp-idf-network` must resolve the **same** `pennant` version, so move them as one wave:

1. Read the upstream `CHANGELOG.md` and the `rust-version` of each new release.
2. Bump `esp-idf-hal` / `esp-idf-svc` to what the upstream crates require.
3. Bump the ws2812 crates and the network pin (git `rev` while unreleased, crates.io version once released).
4. Raise the nightly pin if any crate's `rust-version` exceeds the pinned nightly's number (both pin sites).
5. `just clean-idf` if `esp-idf-sys` changed, then `just check`, `just clippy`, `just lock-ci`, `just verify`.
6. Confirm a single `pennant` with `cargo tree -i pennant --target riscv32imac-esp-espidf`.

### Security scanning

- `just audit` — `cargo audit` against RustSec.
- `just deny` — advisories, licences, bans, sources; `deny.toml` holds the justified ignores.
- Re-evaluate every ignore in `deny.toml` each quarter.

</details>

<details>
<summary><strong>CI/CD</strong></summary>

GitHub Actions workflows in `.github/workflows/`:

- `rust.yml` — host tests, ESP-IDF firmware build (pinned nightly), Wokwi simulation.
- `clippy.yml`, `fmt.yml` — lint and format gates.
- `audit.yml` — `cargo-deny` advisories.

Keep action major versions aligned with the sibling rustyfarian repos.
Local equivalents: `just act-ci`, `just act-audit`, `just act-fmt`, `just act-all`.

</details>

<details>
<summary><strong>Scheduled Maintenance Cadence</strong></summary>

### Monthly

- [ ] `just verify` and `just ci` pass.
- [ ] `just audit` and `just deny` show nothing new.
- [ ] Check for new releases of `rustyfarian-ws2812` and `rustyfarian-network`; if present, run the release-wave runbook.

### Quarterly

- [ ] Everything in the monthly checklist.
- [ ] Audit `[workspace.dependencies]` against crates.io.
- [ ] Review the nightly pin and `ESP_IDF_VERSION` against what the sibling repos use.
- [ ] Review GitHub Actions versions.
- [ ] Hardware re-test on the C6.
- [ ] Review `docs/project-lore.md` for resolved entries.

</details>

## Maintenance Protocol

Each cycle produces three files in `audit/` (git-ignored, local logs):

1. `YYYY-MM-DD-<cadence>-audit.md` — read-only assessment.
2. `YYYY-MM-DD-<cadence>-plan.md` — executable plan derived from the audit.
3. `YYYY-MM-DD-<cadence>-maintenance.md` — record of what was applied.

Behavioural changes land in `CHANGELOG.md ## [Unreleased]`, deferred work in `docs/ROADMAP.md`, non-obvious insights in `docs/project-lore.md`.
