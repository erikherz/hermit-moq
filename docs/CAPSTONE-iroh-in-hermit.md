# iroh-in-Hermit capstone — LANDED (2026-07-13)

Full iroh QUIC transport running **inside HermitOS unikernel guests** (uhyve/KVM microVMs),
two-guest live loop, reproducible **4/4** clean:

- SERVER_ACCEPTED  + SERVER_SENT_BYTES=200000
- CLIENT_CONNECTED + CLIENT_RECEIVED_BYTES=200000 + PROBE_DONE

i.e. bidirectional QUIC handshake completes between two Hermit guests over a Linux bridge,
and a 200 KB QUIC uni-stream is transferred and received intact.

## Environment
- Host: THE-HOST ((host access redacted)). Fleet autoscaler pid <N> left untouched.
- Spike root: /root/iroh-spike  (throwaway, local only).
- Target: x86_64-unknown-hermit, nightly -Zbuild-std=std,panic_abort,
  RUSTFLAGS="-C target-feature=+crt-static -C relocation-model=pic --cfg tokio_unstable".
- Runtime: tokio 1.52 fork (current_thread, enable_all), mio 1.2.1 (poll(2) selector on hermit),
  iroh 1.0.2 with n0 forks vendored as noq / noq-udp / noq-hermit / noq-udp-hermit.
- Harness: /root/iroh-spike/two-guest-macfix.sh  (br0 + tapA/tapB, distinct tap MACs, -c 2).

## The three fixes (all on the single-core two-executor race)

Architecture: two executors share the guest — (1) the kernel async executor runs `network_run`
(polls virtio-net/smoltcp); (2) the tokio runtime runs the app. They alternate via park →
sys_poll → kernel block_on. Every fix below is about not letting one starve the other.

### Fix 1 — kernel `network_run` conditional self-wake
File: hermit-rs/kernel/src/executor/network.rs
Was: `if __pr == SocketStateChanged || cfg!(feature="idle-poll") { wake_by_ref() }`
Now: only self-wake on real progress (`SocketStateChanged`); otherwise register waker +
poll-delay timer and go idle. Under idle-poll the unconditional self-wake made network_run
perpetually ready, monopolizing the single core once the tokio task parked → tokio process()
stopped ~6s in.

### Fix 2 — noq-hermit EndpointDriver::poll: quinn-idiomatic single recv cycle + self-wake
File: vendor/noq-hermit/src/endpoint.rs (EndpointDriver::poll)
One `drive_recv` per poll (its internal loop drains to WouldBlock or the recv work-limiter),
then self-wake ONLY when `keep_going` is true (limiter tripped, no readiness waker armed on
that exit). Bounded: once truly drained → Pending, runtime parks, timers fire. Replaces an
earlier full-drain-to-1024 loop that processed the whole handshake burst under the lock in one
poll and wedged before accept.

### Fix 3 — kernel UDP recv honors O_NONBLOCK  ← THE LINCHPIN
File: hermit-rs/kernel/src/fd/socket/udp.rs (recvfrom + read)
The socket tracks `nonblocking: bool` (set via set_status_flags) but recvfrom/read ignored it:
on `!can_recv()` they always `register_recv_waker; Poll::Pending`. Driven by `block_on(…, None)`
at the syscall boundary, tokio's *non-blocking* `try_io()` recv therefore BLOCKED in-kernel,
spinning network_run and freezing the tokio runtime (heartbeat + connection timers dead) exactly
at the handshake send burst. Fix: when `nonblocking && !can_recv()` return `Errno::Again`
(WouldBlock) immediately, so tokio arms readiness and the runtime parks normally.

## Reproduce
    cd /root/iroh-spike && bash build90.sh        # or any build script (RUSTFLAGS baked in)
    bash two-guest-macfix.sh                       # look for SERVER_SENT_BYTES=200000 / CLIENT_RECEIVED_BYTES=200000

## Notes / follow-ups
- Instrumentation still present (eprintln/log markers: NET_POLL, EPDRV_POLL, APP_RX, POLL_CANRECV,
  NOQ_*, CONN_TIMER_*, DEV_RX). Removal is optional and timing-sensitive (the race is real) — re-run
  the harness after stripping to confirm no regression.
- Plain two-guest.sh regressed to APP_RX=0 after a host reboot: it relies on flaky ARP; the guest
  MACs are distinct but bridge FDB learning is racy. Use two-guest-macfix.sh (distinct tap MACs +
  stp_state 0).
- Backups: network.rs.bak81, endpoint.rs.bak_predrain / .selfwake_bak, udp.rs.bak_nonblock.
