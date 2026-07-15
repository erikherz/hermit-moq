// Mint test tokens + public JWKs for the asymmetric relay verifier (scratch testing only).
// Deterministic keys (fixed seeds) so runs are reproducible. Prints shell-eval-able lines.
use base64::{engine::general_purpose::URL_SAFE_NO_PAD as B64, Engine};
use ed25519_dalek::{Signer, SigningKey};
use p256::ecdsa::{signature::Signer as _, Signature, SigningKey as EcSigningKey};

fn b64(b: &[u8]) -> String {
    B64.encode(b)
}
fn signing_input(h: &str, c: &str) -> String {
    format!("{}.{}", b64(h.as_bytes()), b64(c.as_bytes()))
}
fn jwt(h: &str, c: &str, sig: &[u8]) -> String {
    format!("{}.{}.{}", b64(h.as_bytes()), b64(c.as_bytes()), b64(sig))
}

fn main() {
    // Broad scope (matches every path) so the test isolates signature/alg verification from scope logic.
    let claims = r#"{"put":[""],"get":[""],"exp":2000000000}"#;

    // ---- Ed25519 (EdDSA) ----
    let sk = SigningKey::from_bytes(&[7u8; 32]);
    let vk = sk.verifying_key();
    let h = r#"{"typ":"JWT","alg":"EdDSA"}"#;
    let sig = sk.sign(signing_input(h, claims).as_bytes());
    println!(
        "ED_JWK={{\"kty\":\"OKP\",\"crv\":\"Ed25519\",\"x\":\"{}\"}}",
        b64(&vk.to_bytes())
    );
    println!("ED_TOKEN={}", jwt(h, claims, &sig.to_bytes()));

    // a token signed by a DIFFERENT ed25519 key (must be rejected by the relay)
    let bad = SigningKey::from_bytes(&[9u8; 32]);
    let bsig = bad.sign(signing_input(h, claims).as_bytes());
    println!("ED_TOKEN_WRONGKEY={}", jwt(h, claims, &bsig.to_bytes()));

    // ---- ES256 (ECDSA P-256) ----
    let esk = EcSigningKey::from_bytes(&[3u8; 32].into()).unwrap();
    let evk = esk.verifying_key();
    let pt = evk.to_encoded_point(false);
    let h2 = r#"{"typ":"JWT","alg":"ES256"}"#;
    let esig: Signature = esk.sign(signing_input(h2, claims).as_bytes());
    println!(
        "ES_JWK={{\"kty\":\"EC\",\"crv\":\"P-256\",\"x\":\"{}\",\"y\":\"{}\"}}",
        b64(pt.x().unwrap()),
        b64(pt.y().unwrap())
    );
    println!("ES_TOKEN={}", jwt(h2, claims, &esig.to_bytes()));
}
