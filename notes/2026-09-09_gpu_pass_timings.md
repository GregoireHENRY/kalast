# Per-pass GPU timings, and what the first numbers said

`config.gpu_timing = True` turns on timestamp queries; `sim.gpu_timings()`
and the `{gpu}` HUD placeholders read them back. From the WebGPU samples'
`timestampQuery`, and built for the question in
`2026-09-08_step_one_frame_and_a_bad_benchmark.md`: every timing here was
wall clock around a whole frame, which says how long a frame took and nothing
about what took it.

## What the hardware allows

The M1 Pro reports `TIMESTAMP_QUERY` and **neither** `..._INSIDE_ENCODERS`
nor `..._INSIDE_PASSES`. So a timestamp can be written where a pass begins
and where it ends, and nowhere else: timing half a pass means splitting the
pass. wgpu maps the two writes onto Metal's `startOfVertexSampleIndex` and
`endOfFragmentSampleIndex`, so what comes back is bracketed by that pass's
vertex stage starting and its fragment stage finishing.

The feature is *requested* rather than required. Asking for a feature the
adapter lacks fails `request_device` outright, and a machine that cannot time
its passes should still open a window; `gpu_timings()` returns `{}` there,
which a script can tell from a dict of zeros.

## Two things the numbers are not

**They are not exclusive occupancy, and they do not sum.** This is the trap,
and it is measured rather than assumed:

| bodies | shadow | render | span | wall clock |
|---|---|---|---|---|
| 1 | 1.60 | 1.74 | 1.77 | 3.56 |
| 2 | 2.45 | 1.58 | 1.80 | 3.11 |
| 4 | 4.63 | 1.33 | 3.28 | 3.60 |

Four bodies report **4.6 ms of shadow passes inside a 3.6 ms frame**. Each
body is its own shadow layer and its own submit, and each pass's figure
includes waiting for the GPU to reach it, so the passes overlap and adding
them produces a number larger than the frame it happened in. `Timings::span`
-- first timestamp to last across the frame -- is the figure to put beside a
frame time, and it stays under the wall clock everywhere above.

An earlier version of this exposed `"total"`, the sum. It was wrong in
exactly the way a debugging tool must not be: plausible, precise, and larger
than the thing it was a part of.

**They are a few frames old.** Reading back means mapping a buffer, which is
sound only once the GPU has finished with it. Blocking for that would cost
more than it measures, so a frame resolves its queries and reads whatever an
earlier one left -- about five frames back at 300 it/s. `gpu_timings()`
carries `"frame"`, the iteration the numbers were measured on, so a caption
cannot claim the wrong one.

## Where the resolve goes

At the *start* of the following frame, not the end of the one being timed.
In the editor the egui pass is submitted after `Window::render` returns, so a
resolve recorded inside it would read that pass's slot before the GPU had
written it -- which showed up as `gui` being the only pass ever timed there.

The same shape caused the first bug: `scope()` refused to hand out slots
while a readback was in flight, on the theory that a frame which cannot be
read is not worth timing. But only the *resolve* needs the buffer; the query
set is always writable. Refusing scopes meant a frame recorded only the
passes that happened to run after the map landed, which in the editor was
egui and nothing else. Slots are handed out unconditionally now, and the
frames whose resolve is skipped simply go unread.

Slots come from a pool of sixteen rather than one per scope, because two
scopes repeat within a frame: the shadow map is one pass per body, and the
text overlay is drawn up to three times (into the export, into the viewport,
onto the swapchain). Each slot remembers whose it is, so repeats sum instead
of overwriting each other.

## The first finding

At one body the GPU spans **1.8 ms of a 3.6 ms frame**. Half the frame is not
the GPU, which is the shape of the open question in the benchmark note -- and
the first time it has been measured rather than inferred.

The other half of the table is more actionable: `shadow` triples from 1.6 to
4.6 ms going from one body to four, while the geometry stays at twelve facets
each. That is not shading, it is per-pass cost, and the passes exist
separately because a uniform write is ordered against submits rather than
against command recording, so each layer's matrix needs its own submit. A
per-layer uniform offset would collapse them into one encoder. Untried;
Didymos is a two-body scene and Hera scenes are three, so this is worth
knowing before the shadow pass is touched again.

## The first thing it measured, and the answer was no

The finding above suggested collapsing the per-layer submits: one encoder,
one submit, the layer index handed to the pass as a dynamic offset instead of
being written into a uniform between submits. Done in `bb5e1cf`, and it
bought nothing. Medians of five runs, first discarded:

| bodies | wall before | wall after | span before | span after |
|---|---|---|---|---|
| 1 | 3.434 | 3.278 | 1.789 | 1.741 |
| 2 | 3.207 | 3.256 | 1.789 | 1.936 |
| 4 | 3.690 | 3.680 | 3.247 | 3.042 |

Every difference is inside the run-to-run spread. So the ~1 ms per extra body
is **per render pass, not per submit** -- the same thing the depth-pass probe
above hinted at. Collapsing command buffers cannot touch it; collapsing the
*passes* is what would, and there are two ways to do that on this machine:

- **Multiview.** Supported here (checked). One pass writing every array layer,
  with `@builtin(view_index)` selecting the matrix -- the shadow map is
  already an array texture with a view per layer.
- **A shadow atlas.** All layers side by side in one depth texture, one pass,
  `set_viewport` per body. No feature needed, but the sampling side has to
  learn the UV offsets.

Neither is attempted. The value of the negative result is that it says which
of the two candidate explanations was right, and the dynamic-offset change is
kept because a layer index that is a pass parameter is the prerequisite for
either.

## Not covered

The compute passes -- view factors, per-facet occlusion, the GPU
thermophysical model -- are not instrumented. They are the same mechanism
(`ComputePassDescriptor` takes the same `timestamp_writes`) and the pool has
room; they were left out because they run on request rather than per frame,
and the frame is what the open question is about.
