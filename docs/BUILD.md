# Building

> This tree is **self-contained and buildable from a clean clone** — the relay workspace (`relay/`),
> its forked deps (`vendor/`), and the HermitOS kernel/std fork (`hermit-rs/`) are vendored with
> relative paths. `scripts/build-hermit.sh` cross-compiles the kernel + std + relay into one static
> unikernel. (The `patches/` set is the *upstreaming* view of the same changes — see `UPSTREAM.md`.)

## Prerequisites (fresh Ubuntu)

```sh
sudo apt update && sudo apt install -y build-essential pkg-config libssl-dev git curl
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
. "$HOME/.cargo/env"
rustup toolchain install nightly --profile minimal -c rust-src
```
Needs **~4 GB+ RAM** and **~15 GB disk**. `-Zbuild-std` requires the `nightly` toolchain **and** the
`rust-src` component (both above).

## Build

```sh
git clone https://github.com/erikherz/hermit-moq.git
cd hermit-moq
scripts/build-hermit.sh      # ~10 min -> relay/target/x86_64-unknown-hermit/release/moq-relay (~32 MB)
```

## Toolchain (what the script does)

- Rust **nightly** with `-Zbuild-std` (the `x86_64-unknown-hermit` std is built from source).
- Target: `x86_64-unknown-hermit`.
- Runtime: a **tokio 1.52 fork** (current-thread, `enable_all`) + `mio 1.2.1` (poll(2) selector on
  hermit). See `patches/tokio/`.
- iroh **1.0.2** with the n0 crates ported to hermit (`patches/hermit-ecosystem/`).

```sh
RUSTFLAGS="-C target-feature=+crt-static -C relocation-model=pic --cfg tokio_unstable" \
cargo +nightly build --release --target x86_64-unknown-hermit -Zbuild-std=std,panic_abort \
      -p moq-relay --no-default-features --features quinn,iroh
```

`scripts/build-hermit.sh` runs exactly the above (`docs/reference/build_hermit.sh` is the original
box-side script kept for provenance).

## Kernel

Build the HermitOS kernel with the **`fs`** feature (required for uhyve file-mapping of the TLS cert +
auth key) and apply `patches/hermit-kernel/`. Representative feature set:
`["acpi","pci","smp","udp","tcp","virtio-net","idle-poll","fs"]`.

## uhyve

Use **stock uhyve ≥ 0.9.1** — directory `--file-mapping` is a built-in upstream feature (no patch;
see `patches/uhyve/README.md`). Boot maps a host dir to the guest `/certs`:
```sh
uhyve --net tap:<tap> --file-mapping <hostdir>:/certs \
      -e HERMIT_IP=<ip> -e HERMIT_GATEWAY=<gw> -e HERMIT_MASK=255.255.255.0 \
      -e HERMIT_TLS_CERT=/certs/fullchain.pem -e HERMIT_TLS_KEY=/certs/privkey.pem \
      -e HERMIT_IROH_SECRET=<hex> \
      -e MOQ_IROH_EXTERNAL_ADDR=<pubip:port> -e MOQ_IROH_DHT_BOOTSTRAP=<ip:port,...> \
      -c 2 -m 512MiB  moq-relay-unikernel
```

## Reproduce the capstone

`harness/two-guest-quic.sh` boots two guests and runs a QUIC handshake + 200 KB stream. Set `UHYVE`
and `PROBE` to your build outputs; PASS prints `SERVER_SENT_BYTES=200000` /
`CLIENT_RECEIVED_BYTES=200000`. For DHT discovery see `docs/DHT-DISCOVERY.md`.

## Order of operations

Follow the bottom-up sequence in `UPSTREAM.md` (socket2 → tokio → kernel → uhyve → iroh → moq-rs).
Each layer must cross-compile before the next is useful.
