# fix(executor): pass a relative offset to `TaskNotify::wait` in `block_on` (was an absolute timestamp)

**Repo:** hermit-os/kernel · **File:** `src/executor/mod.rs` · **Patch:** `0001-blockon-relative-timeout.patch`

## Problem

`block_on(future, Some(timeout))` computes `wakeup_time = start + duration` (with
`start = now_micros()`, an absolute time) and passes it to `TaskNotify::wait`, which forwards it to
`futex_wait_and_set(…, Flags::RELATIVE, …)`. Under `RELATIVE`, `futex_wait_and_set` treats the
argument as an offset added to `get_timer_ticks()`, so the deadline becomes
`get_timer_ticks() + start + duration` — inflated by `start`. Timed waits therefore park for
roughly `uptime + timeout` instead of `timeout` (error grows with uptime). The `None` (indefinite)
path is unaffected.

## Change

Pass the remaining duration relative to now instead of an absolute timestamp:

```rust
let wakeup_time = timeout.map(|duration| {
    let elapsed = now - start;
    u64::try_from(duration.as_micros()).unwrap().saturating_sub(elapsed)
});
```

`now` is already recomputed each iteration. No other change; `TaskNotify::wait` keeps its
`RELATIVE` contract, and `block_on` is its only caller (`TaskNotify` is private to this module).

## Why

The value handed to a `RELATIVE` futex wait must be a duration-from-now, not an absolute clock
value. This restores `Some(timeout)` semantics so timed operations honor their timeout.

## Testing

- Structural: only the `wakeup_time` computation changes; the surrounding loop (including the
  `Duration::from_micros(now - start) >= duration => Errno::Time` check) is unchanged.
- Observed under a QUIC workload in a HermitOS/uhyve guest where timed waits misbehaved before and
  behaved correctly after. Reproducer: <https://github.com/erikherz/hermit-moq>.

## Alternative considered

Keep an absolute deadline and make `TaskNotify::wait` issue a non-`RELATIVE` wait — but that
requires computing the deadline in the `get_timer_ticks()` clock rather than `now_micros()`. The
relative fix is smaller and localized.
