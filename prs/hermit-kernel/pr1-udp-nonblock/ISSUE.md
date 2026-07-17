# UDP `recvfrom`/`read` on a non-blocking socket parks in-kernel instead of returning `EAGAIN`

**Repo:** hermit-os/kernel · **File:** `src/fd/socket/udp.rs` · **Base:** `main`

## Summary

`UdpSocket`'s async `recvfrom`/`read` ignore the socket's non-blocking flag. When the socket
has no datagram queued (`!socket.can_recv()`), both methods unconditionally
`register_recv_waker(cx.waker())` and return `Poll::Pending` — even when the socket was set
non-blocking (`O_NONBLOCK` via `set_status_flags`, which is already recorded in
`self.nonblocking`).

Because the syscall boundary drives these futures with `block_on(future, None)`, a caller that
performs a **deliberately non-blocking** receive — e.g. tokio/mio probing readiness with
`try_io()` — does not get `EWOULDBLOCK`. Instead the future parks and the caller **blocks
in-kernel**. On a single hart this keeps the network executor spinning and the userspace async
runtime stops making progress: its timers and heartbeats stop firing a few seconds in.

## Expected (POSIX)

A non-blocking recv with no data available returns `EAGAIN`/`EWOULDBLOCK` immediately.

## Actual

Returns `Poll::Pending` and parks, regardless of the non-blocking flag.

## Where

`src/fd/socket/udp.rs`, the `else` arm of `if socket.can_recv()` in **both** `recvfrom` and
`read`:

```rust
} else {
    socket.register_recv_waker(cx.waker());
    Poll::Pending
}
```

## Impact

This is **not** iroh/QUIC-specific — it affects any `O_NONBLOCK` UDP consumer on Hermit
(mio/tokio, or any hand-rolled async UDP). It surfaced while bringing up a QUIC stack
(quinn + magicsock) inside a HermitOS/uhyve unikernel: a full handshake could not complete
because the runtime wedged mid-handshake. With the fix, a two-guest QUIC handshake followed by a
200 KB unidirectional stream completes reliably (4/4).

## Fix

When `self.nonblocking && !socket.can_recv()`, return `Errno::Again` immediately instead of
registering a waker and parking. Two-line addition in each of `recvfrom`/`read`; `self.nonblocking`
is already maintained from `set_status_flags`. Patch attached; PR to follow.

## Reproducer

Minimal two-guest QUIC reproducer + full root-cause write-up:
<https://github.com/erikherz/hermit-moq> (`harness/` + `docs/CAPSTONE-iroh-in-hermit.md`).

I have the fix ready and am happy to open the PR — filing this first to confirm the diagnosis
reads right to you.
