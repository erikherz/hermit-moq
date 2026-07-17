# HermitOS kernel — upstream-submission artifacts

Submission-ready material for the three HermitOS-kernel fixes, **regenerated against upstream
`main`** (not the internal `patches/hermit-kernel/*.patch`, which are debug→clean diffs from this
fork's monorepo layout and do **not** apply to upstream).

> **Base note.** This fork descends from a commit near current `main`, *not* from any release tag.
> The `v0.13.2` tag predates upstream's `recv_slice`/`&mut [u8]` UDP refactor, so diffs against the
> tag look huge; diffs against `main` are minimal. Always regenerate against `main` before opening.

## Status

| # | Fix | File | Against `main` | Ready? |
|---|---|---|---|---|
| 1 | UDP recv honor `O_NONBLOCK` ⭐ | `src/fd/socket/udp.rs` | clean 2-hunk add (`else if self.nonblocking`) — verified `git apply --check` OK | ✅ **PR-ready** (pending harness re-verify) — see `pr1-udp-nonblock/` |
| 2 | `network_run` self-wake under `idle-poll` | `src/executor/network.rs` | one-line: drop `\|\| cfg!(feature = "idle-poll")` from the poll condition (main already has the timer else-branch) | ⚠️ expressible cleanly, but it **changes deliberate `idle-poll` behavior** → open as a discussion issue, not a drop-in bugfix |
| 3 | `block_on` timeout units | `src/executor/mod.rs` | main still computes `wakeup_time = start + duration` and passes it to `task_notify.wait()` | ⚠️ **likely a live bug** if `wait()` treats its arg as a *relative* offset — must verify `TaskNotify::wait` / `futex_wait_and_set` semantics before asserting |

## Recommended order

1. **#1 only, first** — clean, self-evident, POSIX-conformance bugfix with a reproducer. Best
   first contribution; earns trust before the subtler runtime-fairness changes.
2. **#3** — verify the `wait()` relative-vs-absolute semantics; if confirmed, it's a strong
   standalone bugfix (affects every `block_on` timeout caller).
3. **#2** — raise as a design discussion (it alters an intentional feature's behavior).

## Before opening any of these

- [ ] Strip nothing — these are already clean against `main` (the debug instrumentation lived only
      in the old `patches/` diffs).
- [ ] Re-run the two-guest harness 4/4 on a build host (not doable off-box).
- [ ] Confirm each still applies to `main` at submission time (`git apply --check`).
