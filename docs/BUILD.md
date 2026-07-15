# Building

> This repo is a **staging area of patches + source**, not a turnkey tree. These are the moving parts
> and the recipe used to produce the working artifacts. Expect to apply `patches/` onto pinned
> upstream checkouts (see `UPSTREAM.md`) — the exact upstream tags are noted per patch.

## Toolchain

- Rust **nightly** with `-Zbuild-std` (the `x86_64-unknown-hermit` std is built from source).
- Target: `x86_64-unknown-hermit`.
- Runtime: a **tokio 1.52 fork** (current-thread, `enable_all`) + `mio 1.2.1` (poll(2) selector on
  hermit). See `patches/tokio/`.
- iroh **1.0.2** with the n0 crates ported to hermit (`patches/hermit-ecosystem/`).

```sh
RUSTFLAGS="-C target-feature=+crt-static -C relocation-model=pic --cfg tokio_unstable" \
cargo +nightly build --release --target x86_64-unknown-hermit \
      -Zbuild-std=std,panic_abort
```

`relay/build_hermit.sh` is the exact script used for the relay unikernel
(`moq-relay --no-default-features --features quinn,iroh`).

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
