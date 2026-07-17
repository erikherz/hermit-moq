# fix(fd/udp): honor `O_NONBLOCK` on `recvfrom`/`read` (return `EAGAIN`, don't park)

**Repo:** hermit-os/kernel · **File:** `src/fd/socket/udp.rs` · **Patch:** `0001-udp-recv-honor-o-nonblock.patch`

## Problem

`UdpSocket::recvfrom` and `read` ignore the socket's non-blocking flag: on `!socket.can_recv()`
they always `register_recv_waker` + return `Poll::Pending`, even when the socket is non-blocking.
Driven by `block_on(future, None)` at the syscall boundary, a caller doing a non-blocking
`try_io()` recv (e.g. tokio/mio) therefore **blocks in-kernel** instead of getting
`EWOULDBLOCK`. On a single hart the network executor spins and the userspace async runtime stops
making progress (its timers stop firing).

## Change

Add an `else if self.nonblocking { Poll::Ready(Err(Errno::Again)) }` arm to the `!can_recv()`
path in both `recvfrom` and `read`, so a non-blocking receive with no datagram queued returns
`EAGAIN` immediately. `self.nonblocking` is already maintained from `set_status_flags`
(`O_NONBLOCK`); no new state.

```rust
} else if self.nonblocking {
    // A non-blocking socket with no datagram queued must report `EAGAIN` so a
    // caller doing a non-blocking `try_io()` recv (e.g. tokio/mio) can arm
    // readiness and park. Returning `Pending` here instead blocks the caller
    // in-kernel via the syscall boundary's `block_on(None)`, which spins
    // `network_run` and wedges the userspace runtime (its timers stop firing).
    Poll::Ready(Err(Errno::Again))
} else {
    socket.register_recv_waker(cx.waker());
    Poll::Pending
}
```

## Why

POSIX conformance: a non-blocking recv with no data available must return `EAGAIN`/`EWOULDBLOCK`.
The change is not iroh/QUIC-specific — it unblocks any async runtime doing non-blocking UDP recv
on Hermit.

## Testing

- Affects only the non-blocking `!can_recv()` path, which previously could not make forward
  progress anyway (the blocking path is unchanged).
- Two-guest QUIC handshake + 200 KB unidirectional stream now completes **4/4** (previously
  wedged mid-handshake). Reproducer: <https://github.com/erikherz/hermit-moq> (`harness/`).

## Scope / notes

Standalone and self-contained. Two related single-hart runtime-fairness observations
(`network_run` self-wake under `idle-poll`; `block_on` timeout unit handling) are being validated
separately and are **not** part of this PR.
