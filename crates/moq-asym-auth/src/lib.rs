//! Multi-algorithm JWT verifier for the Hermit moq-relay — HS256 (symmetric, existing),
//! plus **EdDSA (Ed25519)** and **ES256 (ECDSA P-256)** asymmetric verification.
//!
//! # Why asymmetric matters (the multi-tenant keystone)
//! With HS256 the key the relay holds both **verifies and signs** — a relay (or a malicious
//! operator) could *forge* a customer's tokens while the key is resident. For arms-length
//! third-party customers that's unacceptable. With EdDSA/ES256 the **customer keeps the private
//! signing key**; the relay is handed only the **public verify key** — it can verify but **cannot
//! forge**. Then TinyMoQ holds no customer signing secret, even ephemerally.
//! See `tinymoq-cdn-service.md` §8 and `hermit/autoscale/MULTI-TENANT.md` §6.
//!
//! # Pure-Rust only
//! Everything here is pure-Rust (hmac/sha2/ed25519-dalek/p256). NO aws-lc-rs, NO ring — that's the
//! constraint that forced inline verification on Hermit in the first place, and these RustCrypto
//! crates build for `x86_64-unknown-hermit` exactly like the existing HS256 path.
//!
//! # Algorithm-confusion safety
//! The verifier dispatches **strictly by the JWT header `alg`** and only ever uses the matching key
//! kind: `HS256` -> the oct secret; `EdDSA` -> the Ed25519 public key; `ES256` -> the P-256 public
//! key. A public key is *never* fed into HMAC, so the classic "sign HS256 using the public key
//! bytes as the HMAC secret" attack cannot validate. `alg: none` and any unknown alg are rejected.
//! If the relay holds no key for the token's alg, verification fails (`NoKeyForAlg`).

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use hmac::{Hmac, Mac};
use p256::ecdsa::signature::Verifier as _;
use serde::{Deserialize, Deserializer};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, PartialEq, Eq)]
pub enum AuthError {
    /// malformed token / base64 / JSON / wrong number of segments
    Decode,
    /// `alg: none` or an algorithm this verifier does not implement
    UnsupportedAlg(String),
    /// the token's alg is supported but no verifying key of that kind is configured
    /// (also the guard against alg-confusion: e.g. an HS256 token on an asymmetric-only relay)
    NoKeyForAlg(String),
    /// signature did not verify against the configured key
    BadSignature,
    /// `exp` is in the past
    Expired,
}

#[derive(Debug, Deserialize)]
struct Header {
    alg: String,
}

/// Minimal local copy of moq_token::Claims (we can't depend on moq-token — its signing path pulls
/// aws-lc-rs). serde renames match the wire format, so stock tokens verify unchanged. `put`/`get`
/// accept a single string OR an array (matching upstream's `OneOrMany`).
#[derive(Debug, Default, Deserialize, PartialEq, Eq)]
pub struct Claims {
    #[serde(default)]
    pub root: String,
    #[serde(default, rename = "put", deserialize_with = "string_or_seq")]
    pub publish: Vec<String>,
    #[serde(default, rename = "get", deserialize_with = "string_or_seq")]
    pub subscribe: Vec<String>,
    #[serde(default, rename = "exp")]
    pub expires: Option<i64>,
}

fn string_or_seq<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum SoS {
        One(String),
        Many(Vec<String>),
    }
    Ok(match SoS::deserialize(d)? {
        SoS::One(s) => vec![s],
        SoS::Many(v) => v,
    })
}

/// The set of verifying keys a relay holds. Typically exactly ONE (per-stream keying maps one key
/// per relay), but several may coexist (e.g. during an HS256 -> EdDSA migration). Each key kind is
/// only ever used for its own `alg`.
#[derive(Clone, Default)]
pub struct VerifyKeys {
    pub hs256: Option<Vec<u8>>,
    pub ed25519: Option<ed25519_dalek::VerifyingKey>,
    pub es256: Option<p256::ecdsa::VerifyingKey>,
}

impl VerifyKeys {
    pub fn is_empty(&self) -> bool {
        self.hs256.is_none() && self.ed25519.is_none() && self.es256.is_none()
    }
}

fn b64(s: &str) -> Result<Vec<u8>, AuthError> {
    URL_SAFE_NO_PAD.decode(s).map_err(|_| AuthError::Decode)
}

/// Verify a compact JWS (`header.payload.signature`) against `keys`, dispatching by header `alg`,
/// then parse claims and enforce `exp` against `now_unix` (seconds). Returns the claims on success.
pub fn verify(token: &str, keys: &VerifyKeys, now_unix: i64) -> Result<Claims, AuthError> {
    let mut parts = token.split('.');
    let (h_b64, p_b64, s_b64) = match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some(h), Some(p), Some(s), None) if !h.is_empty() && !p.is_empty() && !s.is_empty() => (h, p, s),
        _ => return Err(AuthError::Decode),
    };

    let header: Header = serde_json::from_slice(&b64(h_b64)?).map_err(|_| AuthError::Decode)?;
    let signing_input = format!("{h_b64}.{p_b64}");
    let sig = b64(s_b64)?;

    match header.alg.as_str() {
        "HS256" => {
            // symmetric: ONLY the oct secret is ever used here — a public key is never fed to HMAC.
            let secret = keys.hs256.as_deref().ok_or_else(|| AuthError::NoKeyForAlg("HS256".into()))?;
            let mut mac = HmacSha256::new_from_slice(secret).map_err(|_| AuthError::BadSignature)?;
            mac.update(signing_input.as_bytes());
            mac.verify_slice(&sig).map_err(|_| AuthError::BadSignature)?;
        }
        "EdDSA" => {
            let vk = keys.ed25519.as_ref().ok_or_else(|| AuthError::NoKeyForAlg("EdDSA".into()))?;
            let signature = ed25519_dalek::Signature::from_slice(&sig).map_err(|_| AuthError::Decode)?;
            // verify_strict rejects small-order/malleable keys & signatures
            vk.verify_strict(signing_input.as_bytes(), &signature)
                .map_err(|_| AuthError::BadSignature)?;
        }
        "ES256" => {
            let vk = keys.es256.as_ref().ok_or_else(|| AuthError::NoKeyForAlg("ES256".into()))?;
            // JWT ES256 signature is the raw fixed-width r||s (64 bytes), NOT ASN.1/DER.
            let signature = p256::ecdsa::Signature::from_slice(&sig).map_err(|_| AuthError::Decode)?;
            vk.verify(signing_input.as_bytes(), &signature)
                .map_err(|_| AuthError::BadSignature)?;
        }
        // "none" and everything else are refused before any signature work.
        other => return Err(AuthError::UnsupportedAlg(other.to_string())),
    }

    let claims: Claims = serde_json::from_slice(&b64(p_b64)?).map_err(|_| AuthError::Decode)?;
    if let Some(exp) = claims.expires {
        if now_unix >= exp {
            return Err(AuthError::Expired);
        }
    }
    Ok(claims)
}

/// Load verifying keys from the uhyve-mapped JWK at `/certs/auth.jwk`. Accepts either a single JWK
/// object or a JWKS (`{"keys":[...]}`), and either raw JSON or base64url-wrapped JSON (uhyve fs
/// quirks / existing tooling). Recognized:
///   - `{"kty":"oct","k":"<b64url>"}`                         -> HS256 secret
///   - `{"kty":"OKP","crv":"Ed25519","x":"<b64url 32B>"}`     -> EdDSA public key
///   - `{"kty":"EC","crv":"P-256","x":"<32B>","y":"<32B>"}`   -> ES256 public key
/// A private JWK (one carrying `d`) is accepted for its PUBLIC half only — the relay never needs,
/// and should never be handed, the private scalar for asymmetric keys.
pub fn keys_from_jwk_bytes(raw: &[u8]) -> Result<VerifyKeys, AuthError> {
    // raw JSON, else base64url-wrapped JSON
    let val: serde_json::Value = serde_json::from_slice(raw)
        .or_else(|_| {
            let inner = URL_SAFE_NO_PAD
                .decode(raw.strip_suffix(b"\n").unwrap_or(raw))
                .map_err(|_| AuthError::Decode)?;
            serde_json::from_slice(&inner).map_err(|_| AuthError::Decode)
        })?;

    let mut keys = VerifyKeys::default();
    let entries: Vec<&serde_json::Value> = match val.get("keys").and_then(|k| k.as_array()) {
        Some(arr) => arr.iter().collect(),
        None => vec![&val],
    };
    for jwk in entries {
        add_jwk(&mut keys, jwk)?;
    }
    if keys.is_empty() {
        return Err(AuthError::Decode);
    }
    Ok(keys)
}

fn jwk_b64(jwk: &serde_json::Value, field: &str) -> Result<Vec<u8>, AuthError> {
    let s = jwk.get(field).and_then(|v| v.as_str()).ok_or(AuthError::Decode)?;
    b64(s)
}

fn add_jwk(keys: &mut VerifyKeys, jwk: &serde_json::Value) -> Result<(), AuthError> {
    let kty = jwk.get("kty").and_then(|v| v.as_str()).unwrap_or("");
    let crv = jwk.get("crv").and_then(|v| v.as_str()).unwrap_or("");
    match (kty, crv) {
        ("oct", _) => {
            keys.hs256 = Some(jwk_b64(jwk, "k")?);
        }
        ("OKP", "Ed25519") => {
            let x: [u8; 32] = jwk_b64(jwk, "x")?.try_into().map_err(|_| AuthError::Decode)?;
            keys.ed25519 = Some(ed25519_dalek::VerifyingKey::from_bytes(&x).map_err(|_| AuthError::Decode)?);
        }
        ("EC", "P-256") => {
            let x = jwk_b64(jwk, "x")?;
            let y = jwk_b64(jwk, "y")?;
            if x.len() != 32 || y.len() != 32 {
                return Err(AuthError::Decode);
            }
            let mut sec1 = Vec::with_capacity(65);
            sec1.push(0x04); // uncompressed point
            sec1.extend_from_slice(&x);
            sec1.extend_from_slice(&y);
            keys.es256 =
                Some(p256::ecdsa::VerifyingKey::from_sec1_bytes(&sec1).map_err(|_| AuthError::Decode)?);
        }
        _ => return Err(AuthError::Decode), // unknown key type
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;

    const NOW: i64 = 1_000_000;
    const FUTURE: i64 = NOW + 3600;
    const PAST: i64 = NOW - 1;

    fn enc(b: &[u8]) -> String {
        B64.encode(b)
    }
    fn jwt(header: &str, claims: &str, sig: &[u8]) -> String {
        format!("{}.{}.{}", enc(header.as_bytes()), enc(claims.as_bytes()), enc(sig))
    }
    fn signing_input(header: &str, claims: &str) -> String {
        format!("{}.{}", enc(header.as_bytes()), enc(claims.as_bytes()))
    }
    fn claims_json(exp: i64) -> String {
        format!(r#"{{"put":["vivoh.earth/s.hang"],"get":["vivoh.earth/s.hang"],"exp":{exp}}}"#)
    }

    // ---- HS256 ----
    fn hs256_sign(secret: &[u8], input: &str) -> Vec<u8> {
        let mut mac = HmacSha256::new_from_slice(secret).unwrap();
        mac.update(input.as_bytes());
        mac.finalize().into_bytes().to_vec()
    }

    #[test]
    fn hs256_valid() {
        let secret = b"super-secret-key";
        let keys = VerifyKeys { hs256: Some(secret.to_vec()), ..Default::default() };
        let h = r#"{"typ":"JWT","alg":"HS256"}"#;
        let c = claims_json(FUTURE);
        let tok = jwt(h, &c, &hs256_sign(secret, &signing_input(h, &c)));
        let got = verify(&tok, &keys, NOW).expect("valid HS256");
        assert_eq!(got.publish, vec!["vivoh.earth/s.hang"]);
    }

    #[test]
    fn hs256_tampered_payload_rejected() {
        let secret = b"super-secret-key";
        let keys = VerifyKeys { hs256: Some(secret.to_vec()), ..Default::default() };
        let h = r#"{"alg":"HS256"}"#;
        let c = claims_json(FUTURE);
        let sig = hs256_sign(secret, &signing_input(h, &c));
        // swap the payload for one granting publish:[""] (everything) but keep the old signature
        let evil = r#"{"put":[""],"get":[""],"exp":9999999}"#;
        let tok = format!("{}.{}.{}", enc(h.as_bytes()), enc(evil.as_bytes()), enc(&sig));
        assert_eq!(verify(&tok, &keys, NOW), Err(AuthError::BadSignature));
    }

    #[test]
    fn hs256_wrong_key_rejected() {
        let keys = VerifyKeys { hs256: Some(b"the-real-key".to_vec()), ..Default::default() };
        let h = r#"{"alg":"HS256"}"#;
        let c = claims_json(FUTURE);
        let tok = jwt(h, &c, &hs256_sign(b"attacker-key", &signing_input(h, &c)));
        assert_eq!(verify(&tok, &keys, NOW), Err(AuthError::BadSignature));
    }

    // ---- EdDSA ----
    fn ed_keypair() -> (ed25519_dalek::SigningKey, ed25519_dalek::VerifyingKey) {
        let sk = ed25519_dalek::SigningKey::from_bytes(&[7u8; 32]);
        let vk = sk.verifying_key();
        (sk, vk)
    }

    #[test]
    fn eddsa_valid() {
        use ed25519_dalek::Signer;
        let (sk, vk) = ed_keypair();
        let keys = VerifyKeys { ed25519: Some(vk), ..Default::default() };
        let h = r#"{"typ":"JWT","alg":"EdDSA"}"#;
        let c = claims_json(FUTURE);
        let sig = sk.sign(signing_input(h, &c).as_bytes());
        let tok = jwt(h, &c, &sig.to_bytes());
        assert!(verify(&tok, &keys, NOW).is_ok());
    }

    #[test]
    fn eddsa_wrong_key_rejected() {
        use ed25519_dalek::Signer;
        let attacker = ed25519_dalek::SigningKey::from_bytes(&[9u8; 32]);
        let (_, real_vk) = ed_keypair();
        let keys = VerifyKeys { ed25519: Some(real_vk), ..Default::default() };
        let h = r#"{"alg":"EdDSA"}"#;
        let c = claims_json(FUTURE);
        let sig = attacker.sign(signing_input(h, &c).as_bytes());
        let tok = jwt(h, &c, &sig.to_bytes());
        assert_eq!(verify(&tok, &keys, NOW), Err(AuthError::BadSignature));
    }

    // ---- ES256 ----
    fn es_keypair() -> (p256::ecdsa::SigningKey, p256::ecdsa::VerifyingKey) {
        let sk = p256::ecdsa::SigningKey::from_bytes(&[3u8; 32].into()).unwrap();
        let vk = *sk.verifying_key();
        (sk, vk)
    }

    #[test]
    fn es256_valid() {
        use p256::ecdsa::signature::Signer;
        let (sk, vk) = es_keypair();
        let keys = VerifyKeys { es256: Some(vk), ..Default::default() };
        let h = r#"{"typ":"JWT","alg":"ES256"}"#;
        let c = claims_json(FUTURE);
        let sig: p256::ecdsa::Signature = sk.sign(signing_input(h, &c).as_bytes());
        let tok = jwt(h, &c, &sig.to_bytes());
        assert!(verify(&tok, &keys, NOW).is_ok());
    }

    #[test]
    fn es256_wrong_key_rejected() {
        use p256::ecdsa::signature::Signer;
        let attacker = p256::ecdsa::SigningKey::from_bytes(&[4u8; 32].into()).unwrap();
        let (_, real_vk) = es_keypair();
        let keys = VerifyKeys { es256: Some(real_vk), ..Default::default() };
        let h = r#"{"alg":"ES256"}"#;
        let c = claims_json(FUTURE);
        let sig: p256::ecdsa::Signature = attacker.sign(signing_input(h, &c).as_bytes());
        let tok = jwt(h, &c, &sig.to_bytes());
        assert_eq!(verify(&tok, &keys, NOW), Err(AuthError::BadSignature));
    }

    // ---- exp / alg / confusion ----
    #[test]
    fn expired_rejected() {
        let secret = b"k";
        let keys = VerifyKeys { hs256: Some(secret.to_vec()), ..Default::default() };
        let h = r#"{"alg":"HS256"}"#;
        let c = claims_json(PAST);
        let tok = jwt(h, &c, &hs256_sign(secret, &signing_input(h, &c)));
        assert_eq!(verify(&tok, &keys, NOW), Err(AuthError::Expired));
    }

    #[test]
    fn alg_none_rejected() {
        let keys = VerifyKeys { hs256: Some(b"k".to_vec()), ..Default::default() };
        let h = r#"{"alg":"none"}"#;
        let c = claims_json(FUTURE);
        let tok = jwt(h, &c, b""); // empty sig segment -> Decode (segments must be non-empty)...
        // use a non-empty bogus sig so we reach the alg switch:
        let tok2 = jwt(h, &c, b"x");
        assert!(matches!(verify(&tok, &keys, NOW), Err(AuthError::Decode)));
        assert_eq!(verify(&tok2, &keys, NOW), Err(AuthError::UnsupportedAlg("none".into())));
    }

    #[test]
    fn alg_confusion_hs256_token_on_asymmetric_relay_rejected() {
        // THE attack: relay holds ONLY an Ed25519 public key. Attacker crafts an HS256 token,
        // signing with the public key bytes as the HMAC secret. Must NOT validate.
        let (_, vk) = ed_keypair();
        let pub_bytes = vk.to_bytes(); // the "secret" the attacker would try
        let keys = VerifyKeys { ed25519: Some(vk), ..Default::default() };
        let h = r#"{"alg":"HS256"}"#;
        let c = claims_json(FUTURE);
        let tok = jwt(h, &c, &hs256_sign(&pub_bytes, &signing_input(h, &c)));
        assert_eq!(verify(&tok, &keys, NOW), Err(AuthError::NoKeyForAlg("HS256".into())));
    }

    #[test]
    fn es256_token_on_eddsa_relay_rejected() {
        use p256::ecdsa::signature::Signer;
        let (sk, _) = es_keypair();
        let (_, ed_vk) = ed_keypair();
        let keys = VerifyKeys { ed25519: Some(ed_vk), ..Default::default() };
        let h = r#"{"alg":"ES256"}"#;
        let c = claims_json(FUTURE);
        let sig: p256::ecdsa::Signature = sk.sign(signing_input(h, &c).as_bytes());
        let tok = jwt(h, &c, &sig.to_bytes());
        assert_eq!(verify(&tok, &keys, NOW), Err(AuthError::NoKeyForAlg("ES256".into())));
    }

    #[test]
    fn malformed_segments_rejected() {
        let keys = VerifyKeys { hs256: Some(b"k".to_vec()), ..Default::default() };
        assert_eq!(verify("only.two", &keys, NOW), Err(AuthError::Decode));
        assert_eq!(verify("a.b.c.d", &keys, NOW), Err(AuthError::Decode));
        assert_eq!(verify("", &keys, NOW), Err(AuthError::Decode));
    }

    // ---- JWK loading ----
    #[test]
    fn jwk_oct_roundtrip() {
        let jwk = format!(r#"{{"kty":"oct","alg":"HS256","k":"{}"}}"#, enc(b"my-secret"));
        let keys = keys_from_jwk_bytes(jwk.as_bytes()).unwrap();
        assert_eq!(keys.hs256.as_deref(), Some(&b"my-secret"[..]));
    }

    #[test]
    fn jwk_ed25519_roundtrip_and_verify() {
        use ed25519_dalek::Signer;
        let (sk, vk) = ed_keypair();
        let jwk = format!(r#"{{"kty":"OKP","crv":"Ed25519","x":"{}"}}"#, enc(&vk.to_bytes()));
        let keys = keys_from_jwk_bytes(jwk.as_bytes()).unwrap();
        assert!(keys.ed25519.is_some());
        let h = r#"{"alg":"EdDSA"}"#;
        let c = claims_json(FUTURE);
        let sig = sk.sign(signing_input(h, &c).as_bytes());
        assert!(verify(&jwt(h, &c, &sig.to_bytes()), &keys, NOW).is_ok());
    }

    #[test]
    fn jwk_p256_roundtrip_and_verify() {
        use p256::ecdsa::signature::Signer;
        let (sk, vk) = es_keypair();
        let pt = vk.to_encoded_point(false); // uncompressed 0x04||x||y
        let jwk = format!(
            r#"{{"kty":"EC","crv":"P-256","x":"{}","y":"{}"}}"#,
            enc(pt.x().unwrap()),
            enc(pt.y().unwrap())
        );
        let keys = keys_from_jwk_bytes(jwk.as_bytes()).unwrap();
        assert!(keys.es256.is_some());
        let h = r#"{"alg":"ES256"}"#;
        let c = claims_json(FUTURE);
        let sig: p256::ecdsa::Signature = sk.sign(signing_input(h, &c).as_bytes());
        assert!(verify(&jwt(h, &c, &sig.to_bytes()), &keys, NOW).is_ok());
    }

    #[test]
    fn jwk_base64url_wrapped() {
        let inner = format!(r#"{{"kty":"oct","k":"{}"}}"#, enc(b"wrapped"));
        let wrapped = enc(inner.as_bytes());
        let keys = keys_from_jwk_bytes(wrapped.as_bytes()).unwrap();
        assert_eq!(keys.hs256.as_deref(), Some(&b"wrapped"[..]));
    }

    #[test]
    fn claims_accept_single_string_scope() {
        // upstream OneOrMany: put/get may be a bare string
        let secret = b"k";
        let keys = VerifyKeys { hs256: Some(secret.to_vec()), ..Default::default() };
        let h = r#"{"alg":"HS256"}"#;
        let c = format!(r#"{{"put":"a/b","get":"a/b","exp":{FUTURE}}}"#);
        let tok = jwt(h, &c, &hs256_sign(secret, &signing_input(h, &c)));
        let got = verify(&tok, &keys, NOW).unwrap();
        assert_eq!(got.publish, vec!["a/b"]);
        assert_eq!(got.subscribe, vec!["a/b"]);
    }
}
