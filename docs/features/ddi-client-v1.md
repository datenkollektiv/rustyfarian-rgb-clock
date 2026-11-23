---
gate: hardware
gate-note: The C6 bench run against a DDI lab server is open; the C3 passed the fleet test on 2026-11-26.
desk-work: available
---

# Feature: DDI Client v1

The clock takes firmware updates from the fleet's DDI server through the upstream DDI client (`rustyfarian-esp-idf-network` 0.8.0, fleet profile 0.4.0).
The MQTT command and manifest flow leaves the firmware ([ADR-007](../adr/007-ddi-is-the-only-ota-path.md)).
Requested by the fleet on 2026-11-23; ESP-IDF tier only.

## Decisions

| Decision | Reason | Rejected Alternative |
|:---------|:-------|:---------------------|
| DDI replaces MQTT OTA in the tree now | One OTA path; no install authority over MQTT | MQTT OTA stays the default image |
| Feature `ddi-lab` = `ota-ddi` + `ota-ddi-insecure-http` | The lab opt-in is a build-time choice only | A runtime or portal switch |
| Default build carries no OTA code | No TLS yet, and the image is at 89% of its slot | `ota-ddi` in every image |
| `DDI_BASE_URL` compiled in; `build.rs` requires it for `ddi-lab` | Upstream has no portal field for it | Portal extra |
| `DeviceName::MacSuffixed("rgb-clock")` in every build | One id per clock | Operator-typed names |
| Token read from the portal extra `ddi_token` | One token per device, handed over at enrolment | Compile-time `DDI_TOKEN` |
| Lab image without a valid token opens the portal | An image that cannot update must not run silently | Run the clock with OTA off |
| Any lab startup error restarts the device after 5 s | Returning from `main` leaves a pending image unverified | Park with OTA off |
| `build.rs` refuses a `DDI_BASE_URL` that is not `http://` | An `https://` URL would ship in a bundle that cannot start | Validate only at boot |
| Runtime starts at link-up or the failure deadline | A poll before link-up costs a back-off of 5 min or more | Start right after boot |
| Nudge topic fed from `IdfHooks::other_message` | Bench deployments do not wait out the poll sleep | No nudge |
| `unhealthy` implies `ddi-lab`, keeps the 15 s fail-fast | The rollback demo stays short | Upstream's 120 s default |
| `just ddi-bundle` replaces the `ota-*` recipes | Fleet ingest takes `.bin` plus three sidecars | Keep `manifest.json` staging |
| Network 0.8.0 from crates.io, no `[patch]` | Upstream released the DDI client in v0.8.0 (3425bc4, 2026-11-24) | Git pin to `ba1dbf5` (not in v0.8.0) |
| Root `espressif/mqtt` component declaration removed | Upstream declares it; two declarations break the IDF configure | Keep the exact 1.1.0 pin |
| The pre-swap version check goes to the hardened path | Upstream rolls back a mislabelled image at boot | Local check before the pointover |

## Constraints

- Upstream makes `ota-ddi` next to `ota-wire` a compile error.
- No runtime input may switch the plain-HTTP opt-in on.
- The lab image is never a release image.
- The firmware hands base URL, controller id and token to the upstream transport and builds no DDI request itself.
- Nothing publishes from an MQTT callback ([ADR-005](../adr/005-no-publishing-from-mqtt-callbacks.md)); the nudge hook only sets a flag.
- The token and the `Authorization` header are never logged, so `CONFIG_LOG_MAXIMUM_LEVEL` stays at INFO.
- Both tiers pin one upstream revision, so the graph has one `juggler` ([ADR-006](../adr/006-one-clock-app-on-both-tiers.md) item 6).
- An offer carries a `MAJOR.MINOR.PATCH` label that differs from the running version.
- Boards on MQTT OTA images move over by serial flash, erase and re-provisioning.

## Open Questions

- [x] Does the lab image fit the 0x1c0000 slot on the C6 and the C3? Yes: C6 1,708,128 B (93%, 123 KiB free), C3 1,627,984 B (88%, 202 KiB free); the default image shrinks to 1,421,424 B and 1,338,304 B.
- [x] Which revision does upstream name for the pin? v0.8.0 (3425bc4), from crates.io.
- [ ] Which `board` names do the C6 and C3 images report?
- [ ] Does the portal still require the `ota_url` field under the DDI identity rule?
- [ ] How many seconds pass from `deployment admitted` to `the action is closed` on the C6?
- [ ] What does `nvs_get_stats` report with the token and the DDI action record stored?
- [ ] A bundle built with a version override keeps the package version in `esp_app_desc`; must `ddi-bundle` set it before the fleet checks it?
- [ ] Upstream's `ddi-publish` looks for `<example>-<controller-id>-v<version>.bin`; keep the runbook's copy step or align the names?

## State

- [x] Design approved
- [x] Size gate: the lab image passes `just image-check` on C6 and C3
- [x] Core implementation
- [x] `just verify`, `just verify-fw` (default, `ddi-lab`, `unhealthy`) and `just verify-hal c3` green (2026-11-23, path patch)
- [ ] Bench run on the C6: enrolment, forced update, unhealthy rollback, measured interval
- [x] Pin moved to the upstream release (network 0.8.0 from crates.io), patch dropped
- [x] Documentation updated (README, `AGENTS.md`, runbook, CHANGELOG)
- [x] Fleet test passed on the C3 `rgb-clock-dbbd` (2026-11-26)
- [ ] Adoption reply sent to the fleet: the pinned version, the gates, the image size, the measured figure

## Session Log

- 2026-11-23 — Feature doc created; implementation done against the path patch, all gates green, lab image 1,708,128 B on C6 (93%) and 1,627,984 B on C3 (88%); reviewed (code, SSDLC, justfile); bench run and pin open.
- 2026-11-25 — Network 0.8.0 from crates.io, no source change; `verify`, `verify-fw`, `verify-hal c3`, C3 check and clippy, `image-check`, `ci-test`, `deny` green; C3 lab image 1,629,248 B (88%).
- 2026-11-26 — Fleet test passed on the C3 `rgb-clock-dbbd` with the 0.8.0 lab image; feature doc cleared of fleet-internal ids.
