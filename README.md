# hermit-moq

A **Media-over-QUIC (MoQ) relay running inside a [HermitOS](https://hermit-os.org) unikernel**, using
**[iroh](https://github.com/n0-computer/iroh)** for the relay↔relay transport and the **BitTorrent
Mainline DHT** for peer discovery — **no DNS, no host OS, no open TCP control plane.**

This repository is the **open-source core**: a self-contained, buildable tree for the
`moq-relay-hermit-with-iroh` unikernel (`scripts/build-hermit.sh`), plus the patch sets and PR drafts
to upstream each change back to its origin project (see [`UPSTREAM.md`](UPSTREAM.md)).

## Why this is interesting

- **iroh/QUIC inside a unikernel.** iroh's async QUIC stack (quinn + magicsock) runs inside a
  HermitOS/uhyve microVM — a single-address-space, single-binary guest with no Unix, no DNS resolver,
  no shell. Proven with a two-guest bidirectional handshake + 200 KB stream
  ([`docs/CAPSTONE-iroh-in-hermit.md`](docs/CAPSTONE-iroh-in-hermit.md)).
- **DNS-free discovery.** iroh's default discovery resolves peers via `*.dns.iroh.link` (DNS), which a
  unikernel can't do. This stack discovers peers over the **Mainline DHT via pure UDP**, so a NAT'd
  origin with no public hostname is still reachable ([`docs/DHT-DISCOVERY.md`](docs/DHT-DISCOVERY.md)).
- **NAT-friendly by design.** An origin advertises an operator-declared `external_addr` into the DHT; a
  host-side UDP forward makes it reachable. Edges dial the origin by its iroh `EndpointId`.
- **Tiny attack surface.** The relay is a single static unikernel image; the media/control plane is
  QUIC only. Auth is a portable, dependency-light BYOK JWT verifier
  ([`crates/moq-asym-auth`](crates/moq-asym-auth)).

## What's in here

**Buildable tree** — clone and run `scripts/build-hermit.sh`:

| Path | What |
|---|---|
| `relay/` | the `moq-relay` Cargo workspace (targets `x86_64-unknown-hermit`). The iroh transport + BYOK-auth source is `relay/rs/moq-native/` and `relay/rs/moq-relay/`. |
| `vendor/` | the forked deps that make it cross-compile: `noq*`/`quinn*` (iroh's QUIC stack), `socket2`, `ring`, `netdev`, `netwatch`, `rpv`, `iroh-dns`, `tokio-fork`. |
| `hermit-rs/` | the HermitOS kernel + libstd fork (the 3 kernel fixes are applied here). |
| `crates/moq-asym-auth/` | standalone pure-Rust EdDSA/ES256/HS256 JWT verifier (also independently publishable). |
| `scripts/build-hermit.sh` | one-command cross-compile → unikernel image. |

**Upstreaming material** — the "give back" story, each change mapped to its upstream project:

| Path | What |
|---|---|
| [`UPSTREAM.md`](UPSTREAM.md) | **the PR inventory** — every change grouped by target upstream + readiness. |
| [`prs/DRAFT-PRS.md`](prs/DRAFT-PRS.md) | ready-to-adapt draft PR descriptions, ordered by readiness. |
| `patches/hermit-kernel/` | the 3 kernel fixes as standalone diffs (async self-wake, UDP `O_NONBLOCK`, block-on timer). |
| `patches/{hermit-ecosystem,tokio}/` | the iroh-stack + tokio hermit-port diffs (for upstreaming, vs. the vendored `vendor/`). |
| `patches/uhyve/` | note only — directory `--file-mapping` is already upstream in stock uhyve ≥ 0.9.1 (no patch). |
| `docs/` | `ARCHITECTURE`, `BUILD`, the capstone + DHT-discovery write-ups; `docs/reference/` keeps the original graft + build scripts. |
| `harness/` | sanitized two-guest QUIC reproducer for the capstone. |

## Building

This tree is **self-contained** — the relay workspace, its forked deps (`vendor/`), and the HermitOS
kernel/std fork (`hermit-rs/`) are vendored with relative paths, so one cross-compile builds the
kernel + std + relay into a single static unikernel:

```sh
scripts/build-hermit.sh      # nightly + rust-src; x86_64-unknown-hermit via -Zbuild-std
```

Then boot it under **stock uhyve ≥ 0.9.1** — [`docs/BUILD.md`](docs/BUILD.md) has the full recipe
(RUSTFLAGS, the uhyve boot command, the `HERMIT_*` / `MOQ_IROH_*` env). `UPSTREAM.md` maps each
vendored change back to the upstream tag it derives from.

> **Heads-up:** the folded-in tree has not been compile-verified in this exact layout yet — see
> [`REVIEW-NEEDED.md`](REVIEW-NEEDED.md) (mechanical path rewrites + a license note on the `ring` fork).

## How this fits — the wider ecosystem

`hermit-moq` is the **relay core only**. Two layers are built *on top of* it and live in their own
repos:

- **TinyMoQ** — the **CDN wrapper** around this kernel: the fleet autoscaler (spawns a Hermit
  micro-relay per broadcast), the `cdnadmin` control plane / edge broker, multi-tenant token
  authority, and partner installers. TinyMoQ is what turns a single unikernel relay into an operated
  CDN. *(Private.)*
- **Apps / players** — how people *actually watch and publish*: **moqplay**, **wallflower**, and
  **vivoh.earth** are viewer/broadcaster applications that consume a TinyMoQ (or any MoQ) endpoint.
  These are separate products, not part of the relay.

Open-sourcing this core does **not** open-source the product layers above — that line is drawn
deliberately (see *Scope* below).

## Scope — what's here and what isn't

**In scope (this repo):** the reusable, upstreamable *infrastructure* — running iroh/QUIC + DHT
discovery inside a unikernel, the moq-rs transport features, and the portable auth crate. These
benefit the wider Rust / iroh / HermitOS / MoQ communities and are what a PR would contain.

**Intentionally excluded (proprietary product layer → TinyMoQ / the apps):** the fleet autoscaler,
per-broadcast relay orchestration, the multi-tenant control plane, the token-authority service, the
`cdnadmin` fleet manager, and every web/player application. None of that is here.

## Status

Working and deployed in production (as a proprietary fleet), reproducible **4/4** on the two-guest
harness. The vendored forks are **real and applied**, but before upstreaming they need: de-forking
into `cfg(hermit)` arms, rebasing onto current upstream tags, and stripping the timing instrumentation
left in the kernel patches (see the capstone notes). `UPSTREAM.md` tracks that per target.

## License

Dual **Apache-2.0 OR MIT** (the Rust-ecosystem norm, matching most upstreams this derives from) — see
[`LICENSE-APACHE`](LICENSE-APACHE) and [`LICENSE-MIT`](LICENSE-MIT). Each vendored crate under
`vendor/` and `hermit-rs/` retains **its own** upstream license. **Note:** `vendor/ring-hermit`
derives from [ring](https://github.com/briansmith/ring), which is **not** MIT/Apache — see
[`REVIEW-NEEDED.md`](REVIEW-NEEDED.md) before publishing.
