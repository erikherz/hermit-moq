# DHT node discovery inside HermitOS — pure UDP, no DNS, no TCP

Extends the iroh-in-Hermit capstone (see **[CAPSTONE.md](CAPSTONE.md)**) with
**decentralized peer discovery**: BitTorrent's Mainline DHT (BEP-44 / pkarr)
running *inside* a HermitOS/uhyve guest over **pure UDP — no DNS, no TCP, no
WebSocket.**

## Result (reproducible)

A HermitOS unikernel guest:

1. **Bootstraps the public Mainline DHT over UDP** — a real `find_node` query to
   public bootstrap nodes, building a **98-node routing table**
   (`DHT_BOOTSTRAPPED_OK`). This also proves guest→public-internet UDP round-trips
   both ways (the earlier iroh work only exercised guest↔guest UDP on the bridge).
2. **Resolves a value published by a normal host:**
   - host `dht-pub`: `put_immutable("viroh-hermit-dht-proof-…")` → `PUT_ID=998a1710…`
   - guest: `get_immutable(998a1710…)` → **`DHT_GET_OK value=viroh-hermit-dht-proof-…`**

The tiny relay can self-discover origins by node-id over the DHT — the pure-UDP
alternative to iroh's default DNS-based discovery.

## Why this matters

For NAT'd origins there is no public IP to hand out, so the relay must discover
peers itself. iroh's `N0` preset resolves peers via **DNS** (`*.dns.iroh.link`),
which does not work inside Hermit. `n0-mainline` rides **`noq-udp`** — the exact
async UDP socket already ported to Hermit (`vendor/noq-udp-hermit`) and proven
carrying magicsock traffic — so the DHT datapath was already Hermit-ready and the
whole stack cross-compiled with **zero new patches.**

## Stack (all pure UDP / tokio — no TCP/HTTP)

    iroh-mainline-address-lookup 0.4  ->  n0-mainline 0.5  ->  noq-udp (= noq-udp-hermit)

## Layout

- `dht-boot-probe/` — bootable Hermit unikernel: DHT bootstrap + `get_immutable`
- `dht-pub/`, `dht-get/` — native (host) publisher / getter for the round-trip proof
- `dht_boot.sh` — boot harness (masquerade to the public internet; bootstrap node
  hostnames resolved to IPs on the host and passed by env, since the guest has no DNS)
- `moq-hermit/` — the `moq-relay --features quinn,iroh` fork that runs as a HermitOS
  unikernel (iroh transport + `iroh://` cluster-connect + live media pulled inside the
  guest); `moq-hermit/rs/dht-probe/` is the DHT compile spike.

## Reproduce

    bash build_dht_boot.sh
    ./dht-pub/target/release/dht-pub            # prints PUT_ID=<hex>
    DHT_GET=<PUT_ID> bash dht_boot.sh           # guest log: DHT_GET_OK value=...

## Hardening — reliable resolve (10/10)

Two issues surfaced under repeated boots; both are now fixed. Result: **10/10
consecutive boots resolve on the first `get_immutable` attempt, zero panics**
(before: 3/5 resolved, 2/5 panicked).

**1. Cold routing table → `DHT_GET_NONE`.** `bootstrapped()` returns as soon as
the first bootstrap `find_node` resolves, which can leave the routing table with
as few as 2 nodes — too sparse to walk toward the key. Table growth then depends
on the actor's `socket.recv_from()` continuing to deliver query responses.
*Fix (app-level, `dht-boot-probe/src/main.rs`):* after `bootstrapped()`, poll
`info().routing_table_size()` until the table is warm (≥32 nodes, capped at 10 s)
before issuing the get. A `find_node(target)` warm-up was tried and **removed** —
it is a heavy iterative lookup, redundant once the table is warm, and it widened
the window for issue #2.

**2. Scheduler `unreachable!()` panic** (`kernel .../scheduler/task/mod.rs`,
`custom_wakeup`). Not a background-thread race — `n0-mainline`'s actor is a plain
`tokio::spawn` task on the same `current_thread` executor. The real bug is a
**double wake**: the DHT's iterative query has many concurrent sub-queries sharing
timeout timers and socket-readiness wakeups, so two sources can `custom_wakeup`
the *same* task before it is scheduled. The first removes it from the blocked
list; the second finds it gone and hits `unreachable!()`. *Fix (kernel):*
`BlockedTaskQueue::custom_wakeup` now returns `Option` and yields `None` for a
task that is no longer blocked (a redundant wake is a no-op); the three
`scheduler/mod.rs` call sites push to the ready queue only on `Some`. This also
removes the relay's post-resolve panic — the same wake race, same fix.

See `hermit-rs/kernel/src/scheduler/task/mod.rs` + `scheduler/mod.rs` and
`dht-boot-probe/src/main.rs` for the diffs.
