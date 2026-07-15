# Architecture

How a MoQ relay built from these pieces federates media across clusters using iroh + the DHT, while
staying reachable behind NAT and exposing no host OS.

## The two hops

```
  browser ──WebTransport──▶ EDGE relay ──iroh (or host:port)──▶ ORIGIN relay ◀──WebTransport── browser
  (viewer)                  (unikernel)   discovered via DHT     (unikernel)                    (publisher)
```

1. **browser ↔ relay** is always **WebTransport/HTTP-3 over QUIC**. Unchanged by any of this; a
   browser has no iroh.
2. **relay ↔ relay** (origin ↔ edge, cross-cluster) is the hop this project makes pluggable:
   - **host:port** — the edge dials `https://<origin-ip>:<port>/` (classic, needs a routable origin).
   - **iroh** — the edge dials the origin by its **`EndpointId`** (`iroh://<id>/`); iroh's magicsock
     handles the path, and the origin is located via the **Mainline DHT**, not DNS.

Only the relay↔relay hop changes; the browser experience is identical either way.

## Discovery: Mainline DHT, no DNS

iroh's default discovery publishes/resolves peer records under `*.dns.iroh.link` (DNS + an HTTP
pkarr relay). A unikernel has **no DNS resolver**, so that path can't work. Instead:

```
iroh-mainline-address-lookup ──▶ n0-mainline (BEP-44/pkarr) ──▶ UDP socket (the same one magicsock uses)
```

- The origin **publishes** its `EndpointId → address` record into the DHT over pure UDP.
- The edge **resolves** the origin's `EndpointId` from the DHT, gets an address, and dials.
- Bootstrap nodes are supplied as **IP:port** (`MOQ_IROH_DHT_BOOTSTRAP`) because the guest can't
  resolve the usual bootstrap hostnames.

See `DHT-DISCOVERY.md` for the reproducible proof (98-node routing table built inside the guest,
value published by a host and resolved by the guest).

## NAT traversal for origins

A NAT'd origin using iroh's minimal preset (no relay/STUN) only knows its private bound address, so
its DHT record would be unreachable. The operator declares the real reachable address with
`MOQ_IROH_EXTERNAL_ADDR=<public-ip>:<port>`; the guest binds iroh on a fixed internal port and a
host-side UDP forward maps `public:port → guest:port`. The DHT then advertises a **reachable**
address. (Carrier-grade NAT still needs a real relay for hole-punching — out of scope here.)

## Why a unikernel

The relay is a single statically-linked HermitOS image booted in a uhyve/KVM microVM:

- **Minimal attack surface** — one address space, no shell, no package manager, no Unix sockets, no
  DNS resolver. The only ingress is QUIC.
- **Fast, dense** — sub-second boot, small memory footprint, one relay per broadcast is cheap.
- **Self-contained** — the TLS cert + auth key are file-mapped into the guest at boot; nothing else.

The cost is that the entire async stack (tokio, quinn, magicsock, the DHT) has to run with no OS
underneath it — which is what the kernel/ecosystem patches in `patches/` make possible. The
single-core two-executor race and its three fixes are documented in `CAPSTONE-iroh-in-hermit.md`.

## Auth (BYOK)

The relay verifies inline `?jwt=` tokens with `moq-asym-auth` — a dependency-light multi-algorithm
verifier (HS256 / EdDSA / ES256) that builds for hermit (no aws-lc/ring). Tenants bring their own
key (the relay holds only the **public** verify key, file-mapped as `/certs/auth.jwk`); a token's
`publish`/`subscribe` claims scope what it may do. Cross-cluster pulls carry the same inline token,
so federation is authenticated end to end without the relay ever holding a signing secret.

## Selecting the transport

The relay↔relay transport is chosen per stream at the control plane and is fully backward
compatible: absent any selector, everything behaves exactly as the classic host:port path. In the
reference deployment the choice is surfaced all the way to the viewer (a URL parameter → the origin
transport shown as a stats line), but that plumbing is application-specific and not part of this
core.
