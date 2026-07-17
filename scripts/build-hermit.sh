#!/usr/bin/env bash
# Build the moq-relay HermitOS unikernel (iroh transport + Mainline-DHT discovery) from this repo.
#
# This tree is self-contained: the relay workspace (relay/), its forked deps (vendor/), and the
# HermitOS kernel/std fork (hermit-rs/) are all vendored, wired with relative paths. A single
# cross-compile builds the kernel + std + relay into one static unikernel image.
#
# Prereqs:
#   - rustup with a recent **nightly** toolchain + the `rust-src` component (for -Zbuild-std).
#   - Linux x86_64 host. To *run* the image you also need stock **uhyve >= 0.9.1** (`cargo install uhyve`)
#     and /dev/kvm. See docs/BUILD.md for the boot command.
set -euo pipefail

# Ensure cargo/rustup are on PATH (needed in detached/CI/non-login shells).
source "$HOME/.cargo/env" 2>/dev/null || true

HERE="$(cd "$(dirname "$0")/.." && pwd)"
cd "$HERE/relay"

# Toolchain (nightly + rust-src + the hermit target) is pinned in rust-toolchain.toml,
# so plain `cargo` here resolves to that exact nightly. rustup auto-installs the
# pinned components on first use.
rustup component add rust-src >/dev/null 2>&1 || \
  echo "note: could not auto-add rust-src; run: rustup component add rust-src"

export RUSTFLAGS="-C target-feature=+crt-static -C relocation-model=pic --cfg tokio_unstable"

echo "== building moq-relay -> x86_64-unknown-hermit (pinned nightly, -Zbuild-std) =="
cargo build --release --target x86_64-unknown-hermit -Zbuild-std=std,panic_abort \
  -p moq-relay --no-default-features --features quinn,iroh

BIN="$HERE/relay/target/x86_64-unknown-hermit/release/moq-relay"
ls -la "$BIN"
echo
echo "Built unikernel: $BIN"
echo "Run it under stock uhyve >= 0.9.1 — see docs/BUILD.md for the boot command + HERMIT_*/MOQ_IROH_* env."
