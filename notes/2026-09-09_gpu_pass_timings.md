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

Every difference is inside the run-to-run spread.

Cubes are not proof for a real scene, so the same A/B was run on Didymos and
Dimorphos at 3,145,728 facets each, four runs a side:

| | shadow | render | span | frame |
|---|---|---|---|---|
| submit per layer | 25.93 | 20.28 | 34.31 | 34.49 |
| one submit | 28.64 | 20.24 | 34.21 | 34.34 |

Neutral there too: the span moves 0.3 %, the frame less than the spread
between runs. The one figure that does move is `shadow` itself, *upwards* --
the passes share a command buffer now, so each one's residency window covers
the others. A good reminder of what the per-pass numbers are and are not:
`span` is the honest one.

So the ~1 ms per extra body is **per render pass, not per submit** -- the same thing the depth-pass probe
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

## The same passes on a real scene

The numbers above are twelve-facet cubes, chosen so that per-pass overhead is
the only thing left in them. That answers "how big is the overhead" and says
nothing about what share of a real frame it is, so: Didymos and Dimorphos at
full resolution, 3,145,728 facets each, mutual shadowing on, medians over 300
frames.

| scene | shadow | render | span | frame | rate |
|---|---|---|---|---|---|
| 12-facet cube, 1 body | 1.63 | 1.76 | 1.81 | 3.53 | 283 it/s |
| 12-facet cube, 2 bodies | 2.39 | 1.49 | 1.94 | 3.07 | 325 it/s |
| 3.1M, 1 body, 100k shadow proxy | 2.29 | 9.93 | 9.94 | 10.05 | 100 it/s |
| 3.1M, 1 body, no proxy | 9.87 | 13.65 | 15.82 | 15.84 | 63 it/s |
| 6.3M, 2 bodies, proxies | 6.53 | 15.58 | 17.32 | 17.39 | 58 it/s |
| 6.3M, 2 bodies, no proxy | 28.53 | 20.27 | 34.17 | 34.54 | 29 it/s |

Three things follow, and only the first was expected.

**Per-pass overhead is a *smaller* share of a real frame, not a larger one.**
The fixed cost is ~1.5 ms of residency per pass and does not grow with
geometry, so at 6.3M facets it is at most a few percent of a 34.5 ms frame.
Multiview or a shadow atlas removes one pass out of two here. It is worth
single-digit percent on this scene, against roughly half the frame on the
cube -- which is the opposite of the direction that made it look attractive.
Neither is worth building for the Hera case on these numbers.

**`shadow_path` proxies are worth 2x the whole frame.** 28.53 ms of shadow
passes becomes 6.53 with a 100k stand-in, and the frame goes 34.54 -> 17.39
ms. That is the lever, it already exists, and it dwarfs anything the pass
structure can offer.

**The shadow pass is quadratic in bodies.** Each layer draws every mesh --
that is what keeps mutual shadowing -- so two bodies is four body-draws, and
`shadow` goes 9.87 -> 28.53 accordingly. With two bodies both draws are
genuinely needed. With more, per-layer occluder culling would matter far more
than how the passes are packaged.

View factors are untouched by any of this: the hemicube path is compute, runs
on request rather than per frame, and is not instrumented here.

## Not covered

The compute passes -- view factors, per-facet occlusion, the GPU
thermophysical model -- are not instrumented. They are the same mechanism
(`ComputePassDescriptor` takes the same `timestamp_writes`) and the pool has
room; they were left out because they run on request rather than per frame,
and the frame is what the open question is about.
