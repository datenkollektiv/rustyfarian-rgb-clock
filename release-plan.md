# Release Plan

Release process for `rustyfarian-rgb-clock`, the firmware that serves as the rustyfarian integration test fixture.
Draft of 2026-11-12, derived from the `v0.2.0` release; confirm before the first use.

## Versioning

- **Scheme:** SemVer, pre-1.0 (minor bumps may break).
- **Source of truth:** `version` in the root `Cargo.toml`, which `build.rs` bakes into the image as `FIRMWARE_VERSION` for OTA.
- **Snapshot convention:** none; `[Unreleased]` in `CHANGELOG.md` collects changes between releases.
- **Not versioned with the release:** `clock-pure`, `clock-app` and `rustyfarian-esp-hal-rgb-clock` keep their own versions and are not published.

## Branch and Tag Convention

- **Release branch:** `main`; feature branches merge first.
- **Tag format:** `vX.Y.Z`, annotated and SSH-signed (`tag.gpgSign = true`), message `Release vX.Y.Z`.
- **Tagging:** manual; CI runs on pushes to `main`, not on tags.

## Pre-flight Checklist

- [ ] Every rustyfarian dependency resolves from crates.io (`deny.toml` `allow-git = []`, no `[patch]` active in `.cargo/config.toml`).
- [ ] `just ci` passes (format, partitions, flash-script tests, deny, check, clippy, tests).
- [ ] `just build idf_c6_rgb_clock`, `just build idf_c3_rgb_clock`, `just build-unhealthy`, `just build-hal c3` and `just image-check` pass.
- [ ] Hardware run on every image the release ships, per the open feature docs' `gate: hardware` notes.
- [ ] `CHANGELOG.md` `[Unreleased]` covers every commit since the last tag.
- [ ] `pre-release-checklist.md` security and hygiene items pass.
- [ ] Working tree clean, release branch merged to `main`.

## Version Bump

- `Cargo.toml` — `[package] version`.

## Publish

No registry publish; the release is the signed tag plus a GitHub release page.

## Changelog

- **Location:** `CHANGELOG.md`, Keep a Changelog format, written by hand.
- **Finalize:** add `## [X.Y.Z] - YYYY-MM-DD` below `## [Unreleased]`, leaving `[Unreleased]` empty.

## GitHub Release

- [ ] Create the release for tag `vX.Y.Z` at `https://github.com/datenkollektiv/rustyfarian-rgb-clock/releases/new`.
- [ ] Paste the changelog entry as the description.

## Rollback Procedure

1. Delete the remote tag: `git push --delete origin vX.Y.Z`.
2. Delete the local tag: `git tag -d vX.Y.Z`.
3. Revert the release commit: `git revert <sha>`.
4. Delete the GitHub release page if one was created.

## Release Record Location

Each release produces three files in `release/`:

1. `YYYY-MM-DD-<version>-preflight.md` — pre-flight assessment.
2. `YYYY-MM-DD-<version>-plan.md` — ordered execution plan.
3. `YYYY-MM-DD-<version>-record.md` — what was released and what remains.
