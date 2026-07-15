# REVIEW NEEDED before merging to `main` / making the repo's face public

This branch (`import/hermit-moq-oss`) folds in the **full buildable tree** — the relay workspace
(`relay/`), its forked dependencies (`vendor/`), and the HermitOS kernel/std fork (`hermit-rs/`) — so
the unikernel can be built from a clone (`scripts/build-hermit.sh`). It was assembled from a
sanitized source snapshot; the items below need your sign-off before this becomes the public default.

## 1. License review of the vendored forks  ⚠ blocking
Each vendored crate keeps its **own** `LICENSE*` files; nothing was re-licensed. Confirm before publishing:

- **`vendor/ring-hermit`** — `ring` is under its own OpenSSL/ISC/BoringSSL-derived terms, **not**
  MIT/Apache. Redistribution is allowed *with its notices intact* but it **cannot** be relicensed
  under this repo's Apache-2.0/MIT. **Decide:** (a) keep it vendored with its own LICENSE, or (b) gate
  the relay's crypto behind a hermit-friendly backend and drop the fork. See `upstreaming/UPSTREAM.md` #2.
- **`vendor/{noq,noq-udp,quinn,quinn-udp,netdev,netwatch,rpv,iroh-dns,web-transport-quinn,socket2}-hermit`**
  — forks of MIT/Apache upstreams (iroh/quinn/etc.). Fine to ship. Note these are **name-forks**
  (`noq` = renamed iroh); upstreaming (`UPSTREAM.md` #11) requires de-forking into `cfg(hermit)` arms.
- **`vendor/tokio-fork`, `hermit-rs/`** — MIT/Apache upstreams. Fine.

## 2. The build has NOT been compile-verified in this layout  ⚠ blocking
I could not run the nightly `-Zbuild-std` hermit cross-compile in this environment. What I did:
- Rewrote absolute `/root/iroh-spike/...` paths → **relative** in `relay/Cargo.toml` and
  `relay/rs/moq-relay/Cargo.toml` (kernel dep → `../../../hermit-rs/hermit`). The original is kept as
  `relay/Cargo.toml.orig-paths` for diffing.
- `tokio` dep was `git = "file:///root/iroh-spike/tokio-fork", branch = "hermit-1.52"` → rewritten to
  `path = "../vendor/tokio-fork"`. The vendored tree is whatever was checked out at snapshot time
  (no `.git`); **confirm it's the `hermit-1.52` content**.
- **First real build:** run `scripts/build-hermit.sh` on a Linux host with a recent nightly +
  `rust-src`. If Cargo complains about the lockfile after the `[patch]` path change, delete
  `relay/Cargo.lock` and let it re-resolve (or `cargo update -w`).

## 3. Toolchain pin
`relay/rust-toolchain.toml` has no channel pin. `-Zbuild-std` needs a recent **nightly** + `rust-src`.
If you hit build-std ABI errors, pin a known-good nightly here.

## 4. Not secrets (so you don't flag them)
- `192.0.2.1` in `relay/rs/.../config.rs` is RFC 5737 TEST-NET (example config).
- `hermit-rs/examples/tls/src/sample.pem` is an upstream hermit-rs **example** cert (regenerable via
  that example's `refresh-certificates.sh`) — not our key.

## 5. Confirmed excluded (secret hygiene)
All of these were **dropped** during the fold and are NOT in the tree: every `*.env`, `*.key`,
`*.jwk`, `*.pem` (except the upstream example above), `customers.json`, `ams.env`, and the app/player
layers (`js/`, `go/`, `demo/`, `web/`, `infra/`) from the original monorepo. A full secret scan was
run before commit (see the commit message).
