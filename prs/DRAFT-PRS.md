# Draft PR descriptions

Ready-to-adapt PR text for the upstreamable pieces, ordered by readiness. Each references the
artifact in this repo. Rebase onto current upstream `main` and re-run the harness before opening.

---

## PR-1 · Publish `moq-asym-auth` (new crate → crates.io)  🟢
**Repo:** new crate (yours) · **Artifact:** `crates/moq-asym-auth/`

**Summary.** A small, dependency-light, multi-algorithm JWT *verifier* for MoQ-style bearer tokens:
HS256 (symmetric) + EdDSA/Ed25519 + ES256/P-256. No `aws-lc`, no `ring` — so it cross-compiles to
constrained targets (e.g. `x86_64-unknown-hermit`) where the usual JWT stacks don't build.

**API.**
```rust
verify(token: &str, keys: &VerifyKeys, now_unix: i64) -> Result<Claims>
keys_from_jwk_bytes(&[u8]) -> Result<VerifyKeys>   // parse a JWK / JWK-set
// Claims { root, publish, subscribe, expires: Option<i64> }
// VerifyKeys { hs256, ed25519, es256 }
```
**Deps:** `base64 0.22`, `hmac 0.12`, `sha2 0.10`, `serde`, `serde_json`, `ed25519-dalek 2`, `p256 0.13`.
**Before publish:** pick license (dual MIT/Apache suggested), add CI + a few RFC-7515 test vectors,
`#![forbid(unsafe_code)]`, docs.rs metadata.

---

## PR-2 · hermit kernel: UDP `recvfrom`/`read` must honor `O_NONBLOCK`  🟡  ⭐
**Repo:** hermit-os/kernel · **Artifact:** `patches/hermit-kernel/0002-udp-recv-honor-o-nonblock.patch`

**Summary.** A UDP socket set non-blocking (via `set_status_flags`) still parked on an empty receive:
`recvfrom`/`read` ignored the `nonblocking` flag and, on `!can_recv()`, always registered a waker and
returned `Poll::Pending`. Driven by `block_on(…, None)` at the syscall boundary, a caller doing a
deliberately non-blocking `try_io()` recv (e.g. tokio/mio) therefore **blocked in-kernel**, spinning
the network executor and starving the userspace runtime — its timers and heartbeats stop.

**Fix.** When `nonblocking && !can_recv()`, return `Errno::Again` (EWOULDBLOCK) immediately instead of
parking, matching POSIX and letting the caller arm readiness and park normally.

**Impact.** Correctness bug for **any** async UDP workload on Hermit, not iroh-specific. This is the
linchpin that let a full QUIC handshake + bulk stream complete between two guests.

**Testing.** Two-guest QUIC harness (`harness/`, see repo): `SERVER_SENT_BYTES=200000` /
`CLIENT_RECEIVED_BYTES=200000`, reproducible 4/4. **Note:** strip the debug counters in the patch
before submitting and re-run (the underlying race is timing-sensitive).

---

## PR-3 · hermit kernel: `network_run` conditional self-wake  🟡
**Repo:** hermit-os/kernel · **Artifact:** `patches/hermit-kernel/0001-network-run-conditional-self-wake.patch`

**Summary.** Under the `idle-poll` feature the network executor's `network_run` unconditionally
re-woke itself every poll, so on a single core it stayed perpetually ready and monopolized the CPU
once the userspace task parked — the userspace runtime stopped making progress a few seconds in.

**Fix.** Self-wake only on real progress (`SocketStateChanged`); otherwise register the waker + a
poll-delay timer and go idle, so the two executors alternate fairly.

**Testing.** Same harness as PR-2. Pairs with PR-2 and PR-4 (block-on timer). Strip instrumentation
+ re-verify before submitting.

---

## PR-4 · hermit kernel: block-on timer fix  🟡
**Repo:** hermit-os/kernel · **Artifact:** `patches/hermit-kernel/0003-blockon-timer-fix.patch`
Small companion to PR-2/PR-3 on the same executor-fairness path. See patch + capstone doc.

---

## PR-5 · moq-rs: iroh `external_addr` + no-DNS DHT bootstrap env  🟡
**Repo:** kixelated/moq · **Artifact:** `relay/moq-native/iroh.rs`

**Summary.** Two env-gated knobs on the iroh endpoint builder, both no-ops when unset:
- `MOQ_IROH_EXTERNAL_ADDR=<ip:port>[,…]` → `builder.external_addr(addr)`, so a NAT'd origin using the
  minimal preset advertises a **reachable** address into discovery (operator declares the host's
  public forward).
- `MOQ_IROH_DHT_BOOTSTRAP=<ip:port>[,…]` → discovery via `iroh-mainline-address-lookup`
  (`DhtAddressLookup`) + `presets::Minimal` instead of the default `*.dns.iroh.link` DNS path — for
  environments with no DNS resolver (bootstrap given as IPs).

**Motivation.** Enables relay↔relay federation for NAT'd origins and DNS-less hosts. Useful on Linux
too, not only unikernels.

---

## PR-6 · moq-rs: cluster-connect to an upstream relay by iroh `EndpointId`  🟡
**Repo:** kixelated/moq

**Summary.** Allow `cluster.connect = iroh://<endpoint-id>/` so an edge dials an origin by identity
(resolved via discovery/DHT) rather than a routable `host:port`. Splits the discovery plane from the
transport plane; the browser↔edge hop is unaffected (still WebTransport). Carries the existing inline
`?jwt=` for authenticated pulls.

---

## PR-7 · moq-rs: Hermit BYOK inline-JWT auth  🟡
**Repo:** kixelated/moq · **Artifacts:** `relay/moq-relay/{auth,config}.rs`, `relay/graft_auth.py`

**Summary.** `cfg(target_os = "hermit")`-gated auth path: when a verify key is file-mapped
(`/certs/auth.jwk`), verify the inline `?jwt=` with `moq-asym-auth` (PR-1) and map its claims to
`moq_token::Claims`, before the resolver path (whose crypto stack doesn't build on hermit). Also read
`/certs/upstream` for the cluster-pull URL. Zero effect off-hermit. `graft_auth.py` is the exact,
reviewable source graft.

---

## PR-8 · iroh + ecosystem: `x86_64-unknown-hermit` support  🔴
**Repo:** n0-computer/iroh (+ socket2, netdev, netwatch, rustls-platform-verifier, iroh-dns)
**Artifacts:** `patches/hermit-ecosystem/*.patch`, `patches/tokio/*.patch`

**Summary.** Port the iroh QUIC stack (quinn + magicsock) and its dependency tree to Hermit. Mostly
`build.rs` + platform-shim `cfg(target_os="hermit")` arms; the one with runtime logic is the endpoint
driver (`noq-hermit.patch`): a quinn-idiomatic single recv-cycle per poll with conditional self-wake
(capstone Fix 2). **This is the largest surface and needs n0 coordination** — today the crates are
name-forked (`noq*`) and must be de-forked into feature-gated upstream support. Do this **last**.
