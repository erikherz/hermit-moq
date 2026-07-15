# moq-asym-auth — multi-algorithm JWT verifier for the Hermit moq-relay

A self-contained, **pure-Rust** JWT verifier supporting **HS256 + EdDSA (Ed25519) + ES256
(ECDSA P-256)**, dispatched by the JWT header `alg`. It exists to add the **asymmetric** verify
path the relay needs for arms-length multi-tenant customers: the customer keeps the private signing
key, the relay holds only the public verify key and **cannot forge** the customer's tokens.

> Status: **LIVE.** Grafted into `moq-relay/src/auth.rs`, built for `x86_64-unknown-hermit`, and
> deployed to the production fleet (`moq-relay.wakefix` md5 `918c3bbe…`, 2026-06-21). Verified
> **10/10** on a scratch node (HS256 no-regression + EdDSA + ES256, all alg-confusion cases) plus a
> production smoke test through the live `cdn` autoscaler. Source is versioned in the `moq-hermit`
> git repo (commit `ed97e8c`, parent `2b6abe8` = pre-graft baseline). Integration spec:
> `hermit/patches/moq-relay-asym-auth.patch`.

## Why a separate crate
The relay's auth lives in `moq-relay/src/auth.rs` (carried as `hermit/patches/moq-relay-tinymoq-auth.patch`;
the full body lives in the box build tree, not vendored here). This crate develops + unit-tests the
crypto core in isolation so it can be reviewed and proven on the host (`cargo test`) before it's
grafted in and built for `x86_64-unknown-hermit` on the box.

## Pure-Rust constraint (why these crates)
The Hermit relay verifies tokens inline precisely because `aws-lc-rs`/`ring` don't build for
`x86_64-unknown-hermit`. This crate keeps that property: `hmac`+`sha2` (HS256, already in the relay),
`ed25519-dalek`/`curve25519-dalek` (EdDSA), and RustCrypto `p256` (ES256) are all pure Rust with no
C / no aws-lc / no ring — they cross-compile for Hermit like the existing HS256 path. (Host
`cargo test` proves correctness; the Hermit cross-build is verified on the box at integration time.)

## API
```rust
let keys: VerifyKeys = keys_from_jwk_bytes(&fs::read("/certs/auth.jwk")?)?; // oct | OKP/Ed25519 | EC/P-256
let claims: Claims  = verify(token, &keys, now_unix)?;                       // dispatch by header alg
```
- `VerifyKeys` holds any subset of `{hs256, ed25519, es256}` — usually exactly one per relay
  (per-stream keying maps one key), but several may coexist (e.g. HS256→EdDSA migration).
- `keys_from_jwk_bytes` accepts a single JWK or a JWKS (`{"keys":[...]}`), raw JSON or
  base64url-wrapped (uhyve fs quirk), and takes only the **public** half of an asymmetric JWK.

## Algorithm-confusion safety (tested)
Dispatch is strictly by `alg`, and each key kind is only used for its own algorithm — a public key
is **never** fed into HMAC. Covered by tests:
- `alg_confusion_hs256_token_on_asymmetric_relay_rejected` — the classic "HMAC with the public key
  bytes" forgery → `NoKeyForAlg("HS256")`.
- `es256_token_on_eddsa_relay_rejected`, `alg_none_rejected`, tampered/expired/wrong-key/malformed.

Run: `cargo test` (17 tests).
