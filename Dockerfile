# Reproducible **build** environment for GostUI.
#
# Scope, deliberately narrow: this image compiles the project and runs the test
# suite. It does not run the compositor. A Wayland compositor needs DRM/KMS
# nodes, a seat, and raw input devices; running one in a container is possible
# only by handing it so much of the host that the result no longer tells you
# anything about whether the code works. Development happens nested in the host
# session (see docs/01-strategia-dev-test.md §2.2), not here.
#
# What this image is for:
#   * a contributor on any distribution getting the right -dev headers in one step
#   * CI running the same thing the maintainer runs
#   * cross-compiling for Raspberry Pi and phones without polluting the host
#
#   docker build -t gostui-build .
#   docker run --rm -v "$PWD:/src" gostui-build cargo test --workspace
#
# Debian is the deployment target for PC (docs/01 §2.1), so the build image
# tracks it rather than the maintainer's Ubuntu.
FROM debian:trixie-slim

ENV DEBIAN_FRONTEND=noninteractive

# Split into two layers so the (large, slow-changing) system dependencies are
# cached independently of the Rust toolchain.
RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential \
        pkg-config \
        ca-certificates \
        curl \
        git \
    && rm -rf /var/lib/apt/lists/*

# Build-time dependencies of the compositor. Keep in step with docs/zaleznosci.md;
# that file is the human-readable list, this is the executable one.
RUN apt-get update && apt-get install -y --no-install-recommends \
        libwayland-dev \
        libxkbcommon-dev \
        libinput-dev \
        libudev-dev \
        libseat-dev \
        libgbm-dev \
        libdrm-dev \
        libsystemd-dev \
        libegl1-mesa-dev \
        libdbus-1-dev \
    && rm -rf /var/lib/apt/lists/*

# Pinned so the image cannot drift from what CI and the maintainer use.
ARG RUST_VERSION=1.96.0
# --component is a repeatable flag, not a list: "--component rustfmt clippy" makes
# rustup-init treat "clippy" as an unknown positional argument and exit non-zero.
# --no-modify-path because PATH is set explicitly below; letting rustup edit
# ~/.profile would add a second source of truth that RUN never reads anyway.
RUN curl -fsSL https://sh.rustup.rs \
      | sh -s -- -y --no-modify-path --profile minimal \
        --default-toolchain "${RUST_VERSION}" -c rustfmt -c clippy
ENV PATH=/root/.cargo/bin:$PATH

# Cross-compilation to ARM64: Raspberry Pi (D-002) and phones (D-026).
# Building on an RPi3 itself is too slow to iterate on.
RUN apt-get update && apt-get install -y --no-install-recommends \
        gcc-aarch64-linux-gnu libc6-dev-arm64-cross \
    && rm -rf /var/lib/apt/lists/* \
    && rustup target add aarch64-unknown-linux-gnu
ENV CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc

WORKDIR /src

# Keeping the build cache out of the bind-mounted source tree means a container
# build never invalidates the host's target/ directory, and vice versa.
ENV CARGO_TARGET_DIR=/build

CMD ["cargo", "test", "--workspace"]
