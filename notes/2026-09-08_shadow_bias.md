# The night side was 0.6 % sunlit

Reported from the crater example: with the Sun below the plane, `lit`
oscillated between 0 % and about 1 % instead of sitting at 0.

## What it was

The slope term of the shadow depth bias, `shadow_bias_scale`, fitted as **ten
texel-depths** and multiplied by `(1 - N.L)^2` in the shader. That factor is
largest at grazing incidence, which is precisely the night-side geometry: the
Sun below the horizon, the far wall of the crater steeply tilted toward it,
and a shadow texel covering a large depth range. The bias let those facets
escape the depth comparison, so the map called them lit.

Confirmed by ray tracing every facet the map called lit at 220 deg: **12
blocked, 0 genuinely reachable**. The map was wrong, not the geometry.

Ruled out first, by measurement:

| | |
|---|---|
| shadow map resolution | 1024, 8192, 16384 all give 12-14 false-lit |
| PCF | `shadow_pcf = 4` changes nothing -- the compute path does not filter |
| the normal offset | at its optimum already; see below |
| the bias floor | needed; at 0 the sweep grows 24 false-dark facets |

## The sweep

Ground truth is a ray from each facet's centroid to the Sun, over 15 Sun
angles covering day, both terminators and night. `false-lit` is the map
calling a facet lit where the ray is blocked; `false-dark` the reverse.

| slope | floor | offset | false-lit | false-dark |
|---|---|---|---|---|
| **10** | 1 | sqrt2 | **288** | 0 |
| 3 | 1 | sqrt2 | 252 | 0 |
| **1** | 1 | sqrt2 | **250** | 0 |
| 0.3 | 1 | sqrt2 | 250 | 0 |
| 0 | 1 | sqrt2 | 250 | 0 |
| 10 | 0 | sqrt2 | 272 | **24** |
| 0 | 1 | 0.7 | 254 | 0 |
| 0 | 1 | 0.3 | 516 | 2 |
| 0 | 1 | 0 | 756 | **498** |

`slope = 1` takes everything the reduction has to give while keeping a slope
term, which is what stops acne on slanted surfaces in scenes this crater does
not represent. Going to 0 buys nothing further.

Night side, `lit` per Sun angle from 100 deg to 260 deg:

| slope | result |
|---|---|
| 10 | 0.00 - 0.59 %, flickering |
| 1 | **0.00 % at every angle** |

Day side is unchanged: 100.00, 99.32, 88.28, 72.85, 57.13, 45.70 % at 0, 15,
30, 45, 60, 75 deg, symmetric about noon.

## What is left

250 false-lit facet-instances over 15 angles, about 0.8 %, all of them
reporting occlusion of **0.5 or 0.75** -- never 0. These are facets straddling
a shadow edge, where some of the four sample points (3 vertices + centroid)
are lit and the centroid is not. A centroid ray cannot adjudicate those, and
the honest reading is that a facet on the terminator is partly lit. Their
median `cos i` is 0.38, so they are not a grazing-angle artefact.

Casting rays from the vertices instead does not settle it either: a ray
launched from a shared vertex escapes between facets, which reported 4612
false-dark and is a defect of the test, not of the code.

## Also

`sim.facet_illumination` exists now, and `lit` in the crater examples uses it.
The old `(shadow < 0.5).mean()` counted facets nothing was *blocking*, which
with the Sun on the far side read 99.2 % where the true insolation is 0.
