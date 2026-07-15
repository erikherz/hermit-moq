#!/usr/bin/env python3
# Graft moq-asym-auth (path dep) + /certs/upstream into /root/moq-hermit so the iroh
# unikernel also enforces BYOK EdDSA/ES256/HS256 tokens (parity with .wakefix).
import re, sys

ROOT = "/root/moq-hermit"
errors = []

def patch(path, checks):
    s = open(path).read()
    orig = s
    for name, old, new, need in checks:
        if need and need in s:
            print(f"  [{path}] {name}: already present, skip"); continue
        if old not in s:
            errors.append(f"{path}: anchor for '{name}' NOT FOUND"); continue
        s = s.replace(old, new, 1)
        print(f"  [{path}] {name}: applied")
    if s != orig:
        open(path, "w").write(s)

# 1) workspace members
wf = f"{ROOT}/Cargo.toml"
patch(wf, [(
    "workspace-member",
    '    "rs/moq-native",',
    '    "rs/moq-asym-auth",\n    "rs/moq-native",',
    '"rs/moq-asym-auth"',
)])

# 2) moq-relay hermit-only dependency
rc = f"{ROOT}/rs/moq-relay/Cargo.toml"
patch(rc, [(
    "asym-dep",
    '[target.\'cfg(target_os = "hermit")\'.dependencies]\n',
    '[target.\'cfg(target_os = "hermit")\'.dependencies]\nmoq-asym-auth = { path = "../moq-asym-auth" }\n',
    "moq-asym-auth",
)])

# 3) auth.rs — OnceLock cache + verify() hermit branch
au = f"{ROOT}/rs/moq-relay/src/auth.rs"
helper = '''
// tinymoq BYOK: on Hermit the relay verifies EdDSA/ES256/HS256 JWTs inline with the pure-Rust
// moq-asym-auth crate (jsonwebtoken/aws-lc won't build for the hermit target). Keys come from the
// uhyve file-mapped /certs/auth.jwk; absent => no enforcement (fall through to existing behavior).
#[cfg(target_os = "hermit")]
static ASYM_KEYS: std::sync::OnceLock<Option<std::sync::Arc<moq_asym_auth::VerifyKeys>>> =
\tstd::sync::OnceLock::new();

#[cfg(target_os = "hermit")]
fn asym_keys() -> Option<std::sync::Arc<moq_asym_auth::VerifyKeys>> {
\tASYM_KEYS
\t\t.get_or_init(|| {
\t\t\tstd::fs::read("/certs/auth.jwk")
\t\t\t\t.ok()
\t\t\t\t.and_then(|b| moq_asym_auth::keys_from_jwk_bytes(&b).ok())
\t\t\t\t.filter(|k| !k.is_empty())
\t\t\t\t.map(std::sync::Arc::new)
\t\t})
\t\t.clone()
}

'''
verify_branch = '''\t\tif let Some((base, client)) = &self.auth_api {
\t\t\treturn self.verify_via_api(base, client, params).await;
\t\t}

\t\t// tinymoq BYOK (Hermit): if a verify key is mapped, enforce the inline ?jwt= here (EdDSA/
\t\t// ES256/HS256) before the resolver path (which needs moq_token crypto that isn't built on
\t\t// Hermit). No key mapped => fall through unchanged.
\t\t#[cfg(target_os = "hermit")]
\t\tif let (Some(token), Some(keys)) = (params.jwt.as_deref(), asym_keys()) {
\t\t\tlet now = std::time::SystemTime::now()
\t\t\t\t.duration_since(std::time::UNIX_EPOCH)
\t\t\t\t.map(|d| d.as_secs() as i64)
\t\t\t\t.unwrap_or(0);
\t\t\tlet c = moq_asym_auth::verify(token, &keys, now).map_err(|_| AuthError::DecodeFailed)?;
\t\t\tlet claims = moq_token::Claims {
\t\t\t\troot: c.root,
\t\t\t\tpublish: c.publish,
\t\t\t\tsubscribe: c.subscribe,
\t\t\t\texpires: c.expires,
\t\t\t\t..Default::default()
\t\t\t};
\t\t\treturn Self::finalize(&params.path, claims);
\t\t}'''
patch(au, [
    ("asym-helper", "impl Auth {\n\tpub async fn new(config: AuthConfig)",
     helper + "impl Auth {\n\tpub async fn new(config: AuthConfig)", "fn asym_keys()"),
    ("verify-branch",
     '\t\tif let Some((base, client)) = &self.auth_api {\n\t\t\treturn self.verify_via_api(base, client, params).await;\n\t\t}',
     verify_branch, "tinymoq BYOK (Hermit)"),
])

# 4) config.rs — read /certs/upstream (cross-cluster pull), inert unless mapped
cf = f"{ROOT}/rs/moq-relay/src/config.rs"
upstream = '''\t\t\tif let Ok(u) = std::fs::read_to_string("/certs/upstream") {
\t\t\t\tlet u = u.trim().to_string();
\t\t\t\tif !u.is_empty() {
\t\t\t\t\tconfig.cluster.connect = vec![u];
\t\t\t\t\tconfig.client.tls.disable_verify = Some(true);
\t\t\t\t}
\t\t\t}
\t\t\tconfig.auth.public = Some(crate::auth::PublicConfig::Simple(vec!["".to_string()]));'''
patch(cf, [(
    "upstream-read",
    '\t\t\tconfig.auth.public = Some(crate::auth::PublicConfig::Simple(vec!["".to_string()]));',
    upstream, '/certs/upstream',
)])

if errors:
    print("\nFAILED anchors:"); [print("  -", e) for e in errors]; sys.exit(1)
print("\nAll grafts applied OK")
