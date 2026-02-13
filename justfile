# Rustyfarian RGB Clock — development tasks
#
# The workspace defaults to the ESP32-C6 target (riscv32imac-esp-espidf) via
# .cargo/config.toml, so recipes that touch platform-independent crates
# explicitly pass --target to override it.

host_target := `rustc -vV | sed -n 's/^host: //p'`
esp_target  := "riscv32imac-esp-espidf"

# list available recipes (default)
_default:
    @just --list

# --- Build --------------------------------------------------------------------

# build firmware (release)
build:
    cargo build --release

# check the entire workspace
check:
    cargo check

# --- Flash & Monitor ----------------------------------------------------------

# build, flash, and open serial monitor
flash: build
    cargo espflash flash --release --partition-table partitions.csv --monitor

# open serial monitor (no flash)
monitor:
    espflash monitor

# erase ESP32 flash (needed after sdkconfig changes)
erase-flash:
    espflash erase-flash

# --- Code Quality -------------------------------------------------------------

# run clippy on the entire workspace
clippy:
    cargo clippy -- -D warnings

# run clock-core unit tests on host
test:
    cargo test -p clock-core --target {{ host_target }}

# run tests with stdout/stderr visible
test-verbose:
    cargo test -p clock-core --target {{ host_target }} -- --nocapture

# format all code
fmt:
    cargo fmt

# check formatting without modifying files
fmt-check:
    cargo fmt -- --check

# --- Documentation ------------------------------------------------------------

# build rustdoc for clock-core
doc:
    cargo doc -p clock-core --target {{ host_target }} --no-deps

# build and open docs in browser
doc-open:
    cargo doc -p clock-core --target {{ host_target }} --no-deps --open

# --- Maintenance --------------------------------------------------------------

# update dependencies
update:
    cargo update

# clean build artifacts
clean:
    cargo clean

# set up local cargo config from the template
setup-cargo-config:
    cp .cargo/config.toml.dist .cargo/config.toml

# --- Composite ----------------------------------------------------------------

# full pre-commit verification: format, check, lint, test
verify: fmt check clippy test

# CI-equivalent verification (non-modifying): format check, check, lint, test
ci: fmt-check check clippy test
