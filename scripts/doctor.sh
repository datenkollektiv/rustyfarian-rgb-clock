#!/usr/bin/env bash
set -euo pipefail
# doctor.sh — report development tooling status for rustyfarian-rgb-clock
# Usage: scripts/doctor.sh (no arguments)
#
# rgb-clock targets ESP32-C6/C3 (RISC-V) via ESP-IDF and a pinned nightly
# toolchain — no espup/Xtensa fork and no RAM disk are involved.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

# Pinned toolchain from rust-toolchain.toml (fallback if the file is missing).
pinned="$(sed -n 's/^channel *= *"\(.*\)"/\1/p' "$ROOT_DIR/rust-toolchain.toml" 2>/dev/null)"
pinned="${pinned:-nightly}"

# status <name> <state> <detail>
status() { printf '  %-15s %-9s %s\n' "$1" "$2" "$3"; }

printf 'rustyfarian-rgb-clock — tooling status\n\n'

# --- Rust toolchain -------------------------------------------------------
if command -v rustc >/dev/null 2>&1; then
    status "rustc" "ok" "$(rustc --version 2>/dev/null)"
else
    status "rustc" "MISSING" "install Rust via https://rustup.rs"
fi

if command -v cargo >/dev/null 2>&1; then
    status "cargo" "ok" "$(cargo --version 2>/dev/null)"
else
    status "cargo" "MISSING" "install Rust via https://rustup.rs"
fi

if command -v rustup >/dev/null 2>&1 && rustup toolchain list 2>/dev/null | grep -q "$pinned"; then
    status "nightly" "ok" "$pinned (with rust-src + clippy)"
else
    status "nightly" "MISSING" "run: rustup toolchain install $pinned && rustup component add rust-src clippy --toolchain $pinned"
fi

# --- Build / flash tools --------------------------------------------------
if command -v just >/dev/null 2>&1; then
    status "just" "ok" "$(just --version 2>/dev/null)"
else
    status "just" "MISSING" "install just (the task runner running this)"
fi

if command -v ldproxy >/dev/null 2>&1; then
    status "ldproxy" "ok" "$(command -v ldproxy)"
else
    status "ldproxy" "MISSING" "run: cargo install ldproxy  (the ESP-IDF linker wrapper)"
fi

if command -v espflash >/dev/null 2>&1; then
    status "espflash" "ok" "$(espflash --version 2>/dev/null | head -1)"
else
    status "espflash" "MISSING" "run: cargo install espflash  (needed for: just flash/run/monitor)"
fi

# --- Optional dependency-hygiene tools ------------------------------------
if command -v cargo-deny >/dev/null 2>&1; then
    status "cargo-deny" "ok" "$(cargo-deny --version 2>/dev/null)"
else
    status "cargo-deny" "optional" "run: cargo install cargo-deny  (needed for: just deny)"
fi

if command -v cargo-audit >/dev/null 2>&1; then
    status "cargo-audit" "ok" "$(cargo-audit --version 2>/dev/null)"
else
    status "cargo-audit" "optional" "run: cargo install cargo-audit  (needed for: just audit)"
fi

if command -v cargo-watch >/dev/null 2>&1; then
    status "cargo-watch" "ok" "$(cargo-watch --version 2>/dev/null)"
else
    status "cargo-watch" "optional" "run: cargo install cargo-watch  (needed for: just watch)"
fi

# --- Project configuration ------------------------------------------------
if [ -f "$ROOT_DIR/.cargo/config.toml" ]; then
    status "cargo config" "ok" ".cargo/config.toml present (local dev override)"
else
    status "cargo config" "MISSING" "run: just setup-cargo-config  (copies .cargo/config.toml.dist)"
fi

if [ -f "$ROOT_DIR/.env" ]; then
    status ".env" "ok" "optional non-secret portal prefill values present"
else
    status ".env" "ok" "not present — optional; portal defaults to 1883/rgb-clock, rest typed in the portal"
fi
