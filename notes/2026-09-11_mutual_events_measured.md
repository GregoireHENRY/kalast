# Mutual events: the claim I could not check, now checked

`2026-09-10_polygonal_shadowing_assessment.md` said partial visibility at the
limb was a gap comparable to the shadow quantisation. The single-body
measurement then said otherwise, and that note was corrected — with the
caveat that the claim had really been about **mutual events**, one body
crossing another, and that that case was untested.

`examples/analytical/mutual_event.py` tests it. Two spheres seen edge-on, the
secondary at 0.3 primary radii orbiting at 3, observer along `+x` and the Sun
20 degrees off it so that the transit and the eclipse fall at different
orbital phases. 48 phases, one geometry, one scattering law, so only the
occlusion treatment differs.

## Result

Event depth 299 mmag; 18 of 48 phases inside an event.

| | rms in event | peak |
|---|---|---|
| shadowing quantised to quarters | 3.01 mmag | 8.32 |
| visibility binarised per facet | 2.10 mmag | 6.06 |
| **both — what kalast does** | **2.40 mmag** | 5.01 |
| neither — polygon clipping | 0.59 mmag | 1.68 |

Three things come out of it, and two were not what I expected.

**The original claim holds, but only in this regime.** Binarised visibility
costs 2.10 mmag rms during a mutual event, against 3.01 for the shadow
quantisation — comparable, where on a single body it was negligible beside it.
So the assessment's instinct was right about mutual events and wrong about the
single-body case it was actually measured against, which is exactly the
distinction the correction drew. Both halves of that are now evidence rather
than argument.

**The two approximations partially cancel.** Together they cost 2.40 mmag,
*less* than the shadowing term alone at 3.01. Measuring either in isolation
therefore overstates what removing it buys, and a fix that addressed only one
of them could make the total worse before it made it better. I would not have
predicted this, and it is the sort of thing that makes "improve the obvious
term first" a bad plan.

**The error lives entirely inside the events.** The baseline is 0.00 mmag rms
for every method, because two smooth convex spheres self-shadow nothing —
there is no partial facet anywhere until one body crosses the other. That is
worth holding next to the single-body result, where the error was spread over
the whole rotation: the same code has completely different error structure
depending on what it is pointed at.

Polygon clipping takes it to 0.59 mmag, and as elsewhere much of that residual
is the ray reference's own sampling error rather than the clipper's.

## What this does not cover

- **Circular orbit, spheres, edge-on.** Real mutual events are eccentric,
  inclined and on irregular shapes. The numbers here bound the *method*
  difference, not what a fit to real data would see.
- **Timing rather than depth.** Observers often care about ingress and egress
  *times* more than depth, and a systematic photometric error translates into
  a timing error through the light curve's slope. Not measured.
- **Interpenetrating geometry.** The clipper decides which facet is in front
  at the overlap centroid, which is exact for bodies that do not touch.

## Cost

49 ms a phase for both directions at 1600 facets, so a 48-phase event curve is
about 2.4 s. Comfortably inside a fitting loop.

The ray reference needed the bucketed tracer to be usable at two-body facet
counts — brute force is `O(rays × triangles)` and the first run of this script
did not finish in ten minutes. Same lesson as everywhere else here: the rays
are parallel, so the whole question is two-dimensional.
