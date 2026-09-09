# Occlusion queries — 9 September 2026

Third and last of the items from `2026-09-09_HANDOFF_gpu_timing.md`, after
reversed-Z and primitive picking. The handoff called it the smallest and least
consequential of the three, and that was right.

## The gap

The editor's Visibility panel reports `n of m visible` plus counts clipped
near, far and to the sides. All of it comes from `diagnose`
(`src/app/window.rs`), which transforms each body's bounding box by
`view_proj` and counts the body outside only when *every* corner fails the same
plane.

So "visible" means "its bounding box overlaps the frustum". Dimorphos entirely
behind Didymos still counts. `config.occlusion_queries` adds a row that answers
the other question: did it actually put pixels on screen.

## Where the queries go, and why it matters

A `wgpu::QuerySet` of type `Occlusion` hangs off `RenderPassDescriptor`, and
draws wrapped in `begin_occlusion_query`/`end_occlusion_query` come back with
the number of samples that passed depth.

The obvious thing -- wrap each body's own draw in the main pass -- **does not
give the right answer**. A query counts samples that passed depth *at the
moment that body was drawn*, and bodies are drawn in order, so a body drawn
first and covered later still reports its full silhouette. That is tighter than
the frustum test but still not what the panel claims.

So the queries go on a **bounding box drawn at the end of the main pass**,
after every body has written depth: depth test on, depth writes off, colour
write mask empty. By then the depth buffer is complete, so the answer is
occlusion against the finished scene and is order-independent. It costs 36
vertices per body rather than a second geometry pass.

The overlays -- light cube, axes, colour bar -- are drawn before it and write
no depth, so what the boxes test against is the geometry and nothing else.

Details that are not obvious:

- **`cull_mode: None`.** Culled, a camera inside a box would see only faces it
  is behind.
- **`GreaterEqual`, not `Greater`.** The box touches its body at the extremes,
  and a body exactly filling its box in some view would otherwise fail against
  its own depth. (`Greater` is the reversed-Z sense; see
  `2026-09-09_reversed_z.md`.)
- **A fragment stage that writes to a fully masked target**, rather than
  `fragment: None`. An occlusion query counts samples surviving the
  per-fragment tests, and "there are no fragments to speak of" is a thinner
  guarantee than is worth resting a printed number on.
- **The box is a conservative stand-in.** It can be visible where the body is
  not, so `drawn` can overcount -- the same direction the frustum test already
  errs in, which is the safe one for a diagnostic.
- **A camera inside a box is special-cased on the CPU.** Its front faces are
  behind the near plane and its back faces behind the body's own surface, so
  the query reports a body filling the screen as invisible. Detected in
  `prepare` and treated as drawn without asking.

## Three frames, three copies of the same two numbers

The readback discipline is `gpu_timing.rs`'s, and the same trap appeared in a
new place. `prepare` stages the boxes during the update, which runs *before*
`render` resolves the previous frame's queries. Resolving from the staged count
would therefore read the previous frame's queries with the current frame's
count -- wrong for exactly one frame every time a body is added or removed.

So there are three: `used`/`inside` staged by `prepare`, `recorded` set by
`draw` when the queries actually go into the set, and `pending_*` for whatever
the in-flight readback belongs to. `resolve` and `after_submit` read the middle
one.

## Cost: nothing, and a benchmark that says otherwise

Measured on one Didymos body, `msaa = 1`, `vsync = False`, medians of 60
frames. **GPU span, `gpu_timing` on in both arms so both sync identically:**

| facets | queries off | queries on |
|---|---|---|
| 81,708 | 3.003 / 2.918 ms | 2.901 / 2.916 ms |
| 2,621,156 | 14.195 / 14.551 ms | 14.181 / 14.144 ms |

Indistinguishable. The queries are free; it is the readback that is not, and
that is a buffer map, which is why the flag is off by default.

**Wall-clock frame time says something different and is wrong.** The naive A/B
gave `off 8.84 ms, on 12.54 ms` at 2.6M, and repeats put `off` anywhere between
1.02 and 15.31 ms while `on` stayed at 13.7-14.6. With nothing forcing a sync,
`app.step()` returns as soon as the work is queued; turning the queries on adds
a readback, which forces it. So the A/B mostly measures *whether a sync
happened*. The `on` figures agree with the GPU span, because they are the only
ones that waited for the work.

This is the same artefact the picking benchmark hit the same day, from a
different direction. Stated once more because it keeps coming back: **a
benchmark that mixes blocking and non-blocking frames measures the ordering,
not the work.**

## Two reversed-Z bugs found in the same function

`diagnose` tests clip-space depth directly, and reversed-Z had just moved the
near plane to `z = w` and the far one to `z = 0`. Both tests kept their old
senses:

- `n &= p.z < 0.0` and `f &= p.z > p.w` were **swapped**, so every body clipped
  near was reported as clipped far and the other way round. Invisible in
  testing, because the visible count is right either way -- only the two labels
  exchange.
- `d.light_cube_clipped = p.z > p.w`, commented "only the far plane", became
  the *near* test. The warning fired on the wrong condition, which in practice
  meant never, since the Sun is rarely close enough to trip it.

Both fixed. The guard,
`a_clipped_body_is_reported_against_the_plane_it_actually_left`, needed a
second attempt: the first version had one body behind the eye and one past the
far plane, and **passed with the bug reinstated** -- a swap merely exchanges
which body is which and both counts still read 1. It now puts *two* bodies
behind the eye and one beyond far, so the counts are unequal and a swap shows.
Verified by reinstating the bug and watching it fail.

Every other clip-space test was checked: the two screen-projection sites use
only `ndc.x/y` and a `w <= 0` behind-camera check, so they are depth-sense
independent, and the remaining shader cases are the light path that was
deliberately left forward-Z.

## Verification

Two cubes on the view axis, the far one hidden behind the near one:

- frustum says `visible: 2`, occlusion says `drawn: 1`;
- stepping the far one aside gives `drawn: 2`.

A camera placed inside a body's own bounding box still reports it drawn, which
is the special case above. And a frame with the queries on is **pixel-identical**
to one without, which is what "writes no colour and no depth" has to mean.

The first version of the two-cube test was flaky: it read `visibility()` at a
fixed iteration, and the first readback does not always land by then. It waits
for a valid result now, which is what the API's own documentation implies --
the counts lag, so a caller must not assume an iteration.
