# Upstream PR inventory

Every change needed to run a MoQ/iroh relay inside a HermitOS unikernel, grouped by the upstream
project it targets. Use this to decide **what to upstream, in what order, and to whom.** Ordering
below is roughly bottom-up (a PR depends on the ones above it).

Legend — **Readiness:** 🟢 patch is clean & self-contained · 🟡 needs rebase/cleanup · 🔴 needs
regeneration or upstream design work.

| # | Upstream | Change | Artifact | Readiness | License |
|---|---|---|---|---|---|
| 1 | **rust-lang/socket2** | `x86_64-unknown-hermit` socket support | `patches/hermit-ecosystem/socket2-hermit.patch` (+258) | 🟡 | MIT/Apache-2.0 |
| 2 | **briansmith/ring** | hermit build target | `patches/hermit-ecosystem/ring-hermit.patch` (+4) | 🟡 | ⚠️ ring license (not MIT/Apache) |
| 3 | **tokio-rs/tokio** | hermit runtime (1.52) | `patches/tokio/hermit-tokio-1.52.3.patch` (+57) | 🟡 | MIT |
| 4 | **hermit-os/kernel** | 3 async-networking fixes | `patches/hermit-kernel/000{1,2,3}-*.patch` | 🟡 (strip instrumentation) | MIT/Apache-2.0 |
| 5 | **hermit-os/uhyve** | directory `--file-mapping` | — | ✅ already upstream (≥ 0.9.1) — no PR | MIT/Apache-2.0 |
| 6 | **n0-computer/iroh** (+ deps) | port iroh QUIC stack to hermit | `patches/hermit-ecosystem/{noq,noq-udp,netdev,netwatch,rpv,iroh-dns}-hermit.patch` | 🔴 needs `cfg(hermit)` design | MIT/Apache-2.0 |
| 7 | **kixelated/moq** | iroh `external_addr`/DHT env, cluster-connect by `EndpointId`, hermit BYOK auth | `relay/moq-native/iroh.rs`, `relay/moq-relay/{auth,config}.rs`, `relay/graft_auth.py` | 🟡 | MIT/Apache-2.0 (confirm) |
| 8 | **new crate → crates.io** | `moq-asym-auth` portable JWT verifier | `crates/moq-asym-auth/` | 🟢 | your choice |

Build recipe common to all: `x86_64-unknown-hermit`, nightly `-Zbuild-std=std,panic_abort`,
`RUSTFLAGS="-C target-feature=+crt-static -C relocation-model=pic --cfg tokio_unstable"`, kernel
built with the `fs` feature (required for uhyve file-mapping). See `relay/build_hermit.sh`.

---

## 1. socket2 — hermit target support
`socket2-0.6.4` → hermit. Foundational: quinn/magicsock and much of the async net stack sit on
socket2. The largest single patch (+258). **PR:** add `target_os = "hermit"` arms alongside the
existing unix/wasi paths. **Readiness 🟡:** header carries local vendor paths; normalize `a/…`,`b/…`
and rebase on current socket2.

## 2. ring — hermit build
`ring-0.17.14` `build.rs` tweak so it compiles for hermit (+4). **⚠️ License:** ring is under its own
(OpenSSL/ISC/BoringSSL-derived) terms, **not** MIT/Apache — a `ring` PR and any redistribution must
respect that. Alternative worth evaluating: `aws-lc-rs`/`rustls` crypto backends, or gating so the
relay's TLS uses a hermit-friendly provider. Flag for legal review before publishing.

## 3. tokio — hermit runtime
Patch to run tokio 1.52 on hermit (current-thread, `enable_all`, mio 1.2.1 poll(2) selector),
`--cfg tokio_unstable` (+57). **PR:** feature/cfg-gated hermit support to tokio + mio. **Readiness
🟡:** pinned to 1.52; forward-port to a current tokio before submitting.

## 4. HermitOS kernel — three async-networking fixes  ⭐ highest-value, most novel
All three concern the single-core two-executor race between the kernel's `network_run` (virtio-net /
smoltcp) and the tokio runtime. Full analysis in `docs/CAPSTONE-iroh-in-hermit.md`.

- **0001 — `network_run` conditional self-wake** (`kernel/src/executor/network.rs`). Only self-wake on
  real progress (`SocketStateChanged`), else register waker + poll-delay timer. The unconditional
  self-wake under `idle-poll` monopolized the core and starved tokio.
- **0002 — UDP recv honors `O_NONBLOCK`** (`kernel/src/fd/socket/udp.rs`) — *the linchpin.* `recvfrom`/
  `read` ignored the socket's `nonblocking` flag and always parked on `!can_recv()`, so tokio's
  non-blocking `try_io()` recv blocked in-kernel and froze the runtime mid-handshake. Fix: return
  `EAGAIN` immediately when `nonblocking && !can_recv()`.
- **0003 — block-on timer fix** (`kernel-blockon-timer-fix.patch`).

**Readiness 🟡:** the two generated patches still contain timing/debug instrumentation
(`eprintln!`/atomic counters); strip and re-run the two-guest harness to confirm no regression (the
race is real and timing-sensitive). These are genuine correctness bugs for *any* async UDP workload,
not iroh-specific — the strongest standalone upstream contribution here.

## 5. uhyve — directory `--file-mapping`  ✅ nothing to upstream
The relay maps a real TLS cert + auth key into the guest via a **directory** mapping. This is a
first-class feature of **stock uhyve ≥ 0.9.1** (`--file-mapping host_dir:guest_dir`, per `uhyve
--help`). The earlier "single-file mapping ENOENTs" was a **0.8.0 limitation fixed by upgrading**,
not a patch — the binary once called `uhyve-patched` is verified unmodified stock 0.9.1 from
crates.io. **No PR needed**; just require uhyve ≥ 0.9.1. See `patches/uhyve/README.md`.

## 6. iroh + ecosystem — port the QUIC stack to hermit
The "noq*" crates are iroh 1.0.2 (+ deps) vendored and patched for `x86_64-unknown-hermit`:
`noq`/`noq-hermit` (iroh + endpoint driver), `noq-udp`/`noq-udp-hermit` (the async UDP socket carrying
magicsock **and** the Mainline-DHT datapath), plus `netdev`, `netwatch`, `rustls-platform-verifier`
(`rpv`), and `iroh-dns`. **PR shape:** rather than forks, land `cfg(target_os="hermit")` arms in each
n0 crate (mostly build.rs + platform shims; the endpoint-driver change in `noq-hermit.patch` is the
one with runtime logic — a quinn-idiomatic single recv-cycle + conditional self-wake, see capstone
Fix 2). **Readiness 🔴:** needs coordination with n0; today these are name-forked (`noq*`) which must
be de-forked to real feature-gated support.

## 7. moq-rs — relay features
Three independent, cleanly-scoped additions to `kixelated/moq`:
- **iroh transport env** (`relay/moq-native/iroh.rs`): `MOQ_IROH_EXTERNAL_ADDR` (declare a
  DHT-advertised reachable addr for NAT'd origins) and `MOQ_IROH_DHT_BOOTSTRAP` (no-DNS mainline-DHT
  bootstrap via `DhtAddressLookup` + `presets::Minimal`).
- **cluster-connect by `EndpointId`**: dial an upstream relay by iroh id (`iroh://<id>/`) instead of
  host:port — the discovery-plane/transport-plane split.
- **hermit BYOK auth** (`relay/moq-relay/{auth,config}.rs`, recorded exactly in
  `relay/graft_auth.py`): verify inline `?jwt=` with `moq-asym-auth` when a key is mapped, and read
  `/certs/upstream` for cluster pull. All `cfg(target_os="hermit")`-gated → zero effect off-hermit.

**Readiness 🟡:** self-contained and behind env/cfg; rebase on current moq-rs and split into 3 PRs.

## 8. moq-asym-auth — new standalone crate
Pure-Rust multi-alg JWT verifier (HS256 + EdDSA/Ed25519 + ES256/P-256), **no aws-lc / no ring**, so it
cross-compiles for hermit. Deps: `base64`, `hmac`, `sha2`, `serde`, `ed25519-dalek`, `p256`. Exports
`verify(token, &VerifyKeys, now)`, `keys_from_jwk_bytes(&[u8])`, `Claims`, `VerifyKeys`. **Readiness
🟢:** `cargo check` passes clean on stable (verified). Publish to crates.io as-is (pick a license,
add CI + test vectors). Useful well beyond this project.

---

## Suggested sequencing
1. **Publish `moq-asym-auth`** (independent, 🟢, immediate win).
2. **socket2 + tokio + kernel** hermit support (the reusable foundation; benefits any async-net
   unikernel user, not just MoQ). The kernel UDP `O_NONBLOCK` fix is the flagship.
3. **moq-rs** three feature PRs (independent of the hermit story — the iroh `external_addr`/DHT/
   EndpointId dialing is useful on Linux too).
4. **iroh hermit support** last — largest surface, needs n0 buy-in.

(uhyve needs nothing — just require ≥ 0.9.1.)

## Before publishing — checklist
- [ ] Confirm each patch's upstream license; special-case **ring** (see #2).
- [ ] Normalize patch headers (strip local `/root/iroh-spike`, `/root/.cargo` paths → `a/`,`b/`).
- [ ] Strip kernel debug instrumentation (#4) and re-verify 4/4 on the two-guest harness.
- [ ] Forward-port tokio (#3) and iroh (#6) off the pinned versions.
- [ ] Add CI + a minimal reproducer harness (the two-guest script, sanitized).
