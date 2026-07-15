#!/usr/bin/env bash
source $HOME/.cargo/env
cd /root/moq-hermit
export RUSTFLAGS="-C target-feature=+crt-static -C relocation-model=pic --cfg tokio_unstable"
: > /root/hbuild.log
echo "START $(date -u +%H:%M:%S)" >> /root/hbuild.log
cargo +nightly build --release --target x86_64-unknown-hermit -Zbuild-std=std,panic_abort \
  -p moq-relay --no-default-features --features quinn,iroh >> /root/hbuild.log 2>&1
echo "HBUILD_EXIT=$? $(date -u +%H:%M:%S)" >> /root/hbuild.log
ls -la /root/moq-hermit/target/x86_64-unknown-hermit/release/moq-relay >> /root/hbuild.log 2>&1
