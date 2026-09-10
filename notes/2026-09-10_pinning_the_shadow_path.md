# Pinning the shadow path the TPM runs on

Follow-on from `2026-09-10_code_quality_audit.md`, finding 1. Two corrections
and one new test, and the most useful part is how nearly the test was useless.

## The correction: it was never a physics bug

The audit's first draft called the compute/render shadow divergence "the most
serious finding" and "a correctness issue in the illumination the TPM runs
on". That was wrong on both counts, and reading `src/app/facet_shadow.rs`
before changing anything is what caught it.

Two of the three divergences listed did not exist. The Rust caller already
passes the queried body's **own** layer bias and that layer's **own**
`light_view_proj` -- there is a comment there recording that the scene-wide
values reported Deimos at 0.55 % shadowed against a true ~46 %. Found and
fixed long before this audit.

The one real difference is at `shadow_pcf > 0`: the fragment shader filters
over a `(2N+1)^2` kernel and widens its normal offset by `(1 + N)` to match
that kernel's reach, and the compute path does neither.

**And the compute path is right.** The Sun is a point source here, so
occlusion is binary; PCF is image-space antialiasing with no physical
referent, and softening the terminator would put blur into a boundary
condition that nothing in the model asks for. `notes/2026-09-08_shadow_bias.md`
fitted that path's slope, floor and offset against ray-traced ground truth at
single-tap, and already recorded in its ruled-out table that "`shadow_pcf = 4`
changes nothing -- the compute path does not filter".

So the defect was never in the numbers. It was in two sentences:

- the shader header, claiming the two "cannot disagree";
- `occluded()`, claiming it mirrors the fragment shader "exactly".

Both false above `shadow_pcf = 0`, and both exactly the sentence that would
stop someone re-deriving the difference when a figure and a TPM number fail to
line up. Corrected in `shaders/facet_shadow.wgsl` and
`src/app/facet_shadow.rs`, in both cases saying *why* the paths differ rather
than just that they do.

## The test, and why the first version was worthless

`tests/test_facet_shadow.py` checks two things: that the compute path is
invariant to `shadow_pcf`, and that it agrees with a ray from each facet
centroid to the Sun.

The first version used **one** Sun angle, high above the crater, with a
percentage budget that looked reasonable. It passed. Then, to check it could
fail, the bias in `facet_shadow.wgsl` was multiplied by 100 and the module
rebuilt:

    healthy    18/1540 false-lit (1.2 %)
    bias x100  24/1540 false-lit (1.6 %)     -- still passing

A hundredfold error in the constant the test exists to protect moved the
metric by six facets. The test was decoration.

### What fixed it: sweep the angle, and assert on the worst one

The bias earns its keep at grazing incidence, where one shadow texel spans a
large depth range -- which is the geometry that produced the original bug
(`2026-09-08_shadow_bias.md`, the night side reading 0.6 % sunlit). A single
high Sun never visits it. Fifteen angles, the same coverage that note used:

| | healthy | bias x100 |
|---|---|---|
| false-lit, aggregate | 0.33 % | 0.76 % |
| **false-lit, worst angle** | **1.39 %** | **14.29 %** |
| false-dark | 0.35 % | 0.35 % |

**The aggregate is still nearly useless -- 2.3x -- and the worst single angle
separates by 10x.** Averaging a grazing-incidence failure over fifteen angles
of good behaviour is precisely what hides it. The assertion is therefore on
`max over angles`, with the aggregate kept only as a loose sanity bound.

Budgets are set from that measured pair rather than from round numbers:
worst-angle false-lit at 5 %, which is 3.6x above the healthy value and 2.9x
below the broken one. Confirmed to fail against the broken build and pass
against the restored one, with the shader diff checked afterwards to be sure
nothing was left behind.

False-dark is unmoved by a bias error, and that makes sense: too much bias
lets facets escape the depth comparison, which can only manufacture false
*lit*. It is bounded near its measured value to catch the opposite regression.

## What is pinned, and what is not

- **Proven to have teeth**: the ray-trace checks. A 100x bias error fails them.
- **Not proven**: the `shadow_pcf` invariance. It is a plain array comparison
  over a non-degenerate answer -- 2048 facets, 42 % shadowed -- and it would
  fail if the compute path ever became `shadow_pcf`-dependent. But
  demonstrating that needs a build where it *is* dependent, which means adding
  the parameter to `Params` and the shader, and that was not done. The check
  is sound by construction rather than by demonstration; worth knowing.

## Traps

- **A GPU test needs one `App` per process.** A second constructor call panics
  with `RecreationAttempt` -- a raw `.unwrap()` reaching Python as
  `PanicException`. `shadow_pcf` is live, so the sweep mutates one app rather
  than needing three processes. That restriction is legitimate; surfacing it
  as a panic is not, and is on the audit's list.
- **Ray tracing from the centroid needs an epsilon along the normal**, or the
  facet occludes itself. `1e-4` against ~0.03 facet edges.
- **Only sunward facets are comparable.** A facet turned away is dark because
  of its own orientation, which is insolation's question, not the shadow
  map's, and the map is not fitted for it.
- **Verifying a test by breaking the code costs one rebuild each way** and is
  the only thing that distinguishes a test from a comment. It changed the
  design of this one completely.
