# Local stand-in for GitHub's `ubuntu-latest` runner, used by `just act`.
# The act image matches the runner's Ubuntu release but lacks rustup, which the
# firmware jobs rely on: they call `cargo binstall` before installing a toolchain.
FROM catthehacker/ubuntu:act-26.04

ENV RUSTUP_HOME=/root/.rustup \
    CARGO_HOME=/root/.cargo \
    PATH=/root/.cargo/bin:$PATH

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --profile minimal --default-toolchain stable

# The runner preinstalls Clang 20, 21 and 22 (runner-images Ubuntu2604-Readme.md),
# and bindgen's clang-sys loads the newest libclang it finds, not `libclang-dev`'s.
RUN apt-get update \
    && apt-get install -y --no-install-recommends clang-20 clang-21 clang-22 \
    && rm -rf /var/lib/apt/lists/*
