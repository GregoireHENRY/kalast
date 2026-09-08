# `step()` drew more than one frame, and the benchmark that found it

## One `step()` is one frame now

`step()` documented "one call is one *frame*, not one pump". It was not true.

The redraw handler calls `window.request_redraw()` on entry, so a single
`pump_app_events` dispatches every redraw it can feed itself. `step()` pumped
until `frame_drawn`, but a pump does not stop mid-way: it delivers what is
queued, and the queue refills as it goes.

Measured on the crater, 3000 iterations:

| | loop turns | iterations |
|---|---|---|
| `step.py` | 3001 | 3000 |
| `step.rs` | **580** | 3000 |

So the Rust example ran about **five frames per `step()`** -- five `update()`s
-- and the work done before the call applied to only the first of them. That
is exactly the mismatch between a moved Sun and the shadow map that the
before/after-`step()` idiom exists to prevent. Python's slower loop happened
to leave one redraw in the queue, so it never showed there, which is why this
survived: the language that would have exposed it did not exist yet.

Fixed by returning early from the redraw handler when a frame has already been
drawn *and* the loop is being driven by `step()` -- `start()` owns its loop and
must keep drawing. The redraw stays requested, so the next `step()` has one
waiting. `tests/test_editor_startup.py::test_one_step_is_one_frame` holds it:
40 steps must be 40 iterations.

## The benchmark itself is not usable yet

The question was how `step.py` and `step.rs` compare in release. The honest
answer is that **only the loop body can be compared today**:

| | per iteration | share of a frame |
|---|---|---|
| `step.py` body | 43-57 us | ~1.2 % |
| `step.rs` body | 1.4-1.9 us | ~0.0 % |

25x, and it does not matter: everything else in a frame is the same Rust code
either way.

End to end the numbers **do not compare**, and it took five experiments to be
sure it was the measurement and not the code:

- the Rust binary sits at **121 it/s exactly**, and does not move for the
  per-frame shadow readback being on or off, an 8192 vs 512 shadow map, or
  MSAA 4 vs 1 -- five workloads differing by a large factor, one rate.
  8.26 ms is the 120 Hz panel.
- the Python process is not pinned and wanders between 142 and 281 it/s
  across runs of identical work.
- ruled out: adapter (same "Apple M1 Pro", same features), surface size and
  scale (800x600 at 2.00 in both), config (msaa, shadow resolution, pcf,
  per-body shadows, image and window size all identical), mesh (2048 facets,
  6144 vertices), occlusion (bringing either window to the front or covering
  it changes nothing), and loop pacing (sleeping 45 us per turn in the Rust
  loop, to mimic Python's body, changes nothing).
- both are granted `PresentMode::Immediate` -- neither falls back to `Fifo`,
  and there is now a printed warning if either ever does.

So a Rust binary is somehow presenting in lockstep with the display while the
Python extension is not, with the same code, the same adapter and the same
present mode. Until that is understood, **any it/s comparison between the two
is measuring the window server**, and quoting "Python is 2x faster" would
repeat the mistake already recorded in `CLAUDE.md`: the "3.1M facets costs 2x"
conclusion that was entirely a 120 Hz panel.

Diagnostics left behind, since they are what made this legible:
`[WINDOW] adapter ...`, `[WINDOW] surface configured WxH physical, scale, render`,
and an unconditional warning when `vsync = false` does not get `Immediate`.

## Open

Why the example binary syncs to the display and the extension module does not.
Worth an answer before any cross-language timing is published.
