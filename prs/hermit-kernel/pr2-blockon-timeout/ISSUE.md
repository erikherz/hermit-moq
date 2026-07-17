# `block_on` timed waits sleep far longer than the requested timeout (absolute value passed to a RELATIVE futex wait)

**Repo:** hermit-os/kernel · **Files:** `src/executor/mod.rs`, `src/synch/futex.rs` · **Base:** `main`

## Summary

`block_on(future, Some(timeout))` computes its park deadline as an **absolute** timestamp but
passes it through a **relative** futex wait. The value therefore gets added to the current clock a
second time, so a timed wait parks for roughly `uptime + timeout` instead of `timeout` — the error
grows without bound as the machine runs.

## The chain

1. `src/executor/mod.rs`, `block_on`:
   ```rust
   let wakeup_time =
       timeout.map(|duration| start + u64::try_from(duration.as_micros()).unwrap());
   task_notify.wait(wakeup_time);
   ```
   `start = crate::arch::kernel::systemtime::now_micros()` — an **absolute** time. So `wakeup_time`
   is an absolute timestamp.

2. `TaskNotify::wait`:
   ```rust
   let _ = futex_wait_and_set(&self.futex, 0, timeout, Flags::RELATIVE, 0);
   ```
   forwards it with **`Flags::RELATIVE`**.

3. `src/synch/futex.rs`, `futex_wait_and_set`:
   ```rust
   let wakeup_time = if flags.contains(Flags::RELATIVE) {
       timeout.and_then(|t| get_timer_ticks().checked_add(t))   // deadline = now + t
   } else { timeout };
   ```
   With `RELATIVE`, the argument is an offset added to `get_timer_ticks()`. So the effective
   deadline is `get_timer_ticks() + (start + duration)` — inflated by `start`.

## Effect

- `block_on(_, None)` (the common indefinite path) is **unaffected** — `None` waits until woken.
- `block_on(_, Some(d))` parks for `≈ start + d`. In a busy runtime, frequent wakes mask it: the
  loop's own `Duration::from_micros(now - start) >= duration` check then returns `Errno::Time`, so
  the timeout "mostly works" by luck. In a quiet wait (nothing else wakes the futex) it hangs for
  ~uptime. In all cases the deadline handed to the futex is wrong.

## Fix

Pass the **remaining** duration relative to now, so the `RELATIVE` futex adds a correct offset:

```rust
let wakeup_time = timeout.map(|duration| {
    let elapsed = now - start;
    u64::try_from(duration.as_micros()).unwrap().saturating_sub(elapsed)
});
```

(`now` is already recomputed each loop iteration.) `TaskNotify` is private to `executor/mod.rs`
and `wait()` has exactly one caller — this `block_on` — so the fix is complete and local.

Alternative: keep an absolute deadline and switch `TaskNotify::wait` to a non-`RELATIVE` wait, but
that means computing the deadline in the `get_timer_ticks()` clock (not `now_micros()`); the
relative fix above is smaller and preserves `wait()`'s current contract.

## Reproducer / context

Surfaced under a QUIC workload inside a HermitOS/uhyve unikernel, where timed waits misbehaved:
<https://github.com/erikherz/hermit-moq>. Patch attached; happy to open the PR.
