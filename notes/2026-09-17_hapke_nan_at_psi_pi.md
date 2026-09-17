# Hapke's roughness returned NaN in the back-scattering plane

Found by pulling the macOS work onto Windows and running the suite, which is
the only reason it surfaced: **`cargo test` and `test_lightcurve` were both
red here and green there.**

## Symptom

```
scattering::tests::roughness_is_reciprocal_across_the_branch ... FAILED
  not reciprocal at mu0=0.9 mu=0.2 alpha=2: NaN vs NaN

test_roughness_darkens_a_sphere_more_at_larger_phase_angle
  0 deg: 1   20 deg: nan   60 deg: nan   100 deg: nan mmag
```

`roughness_terms` returned finite `mu0e` and `mue` and a **NaN `S`**.

## Cause

`f(psi) = exp(-2 tan(psi/2))`, written literally:

```rust
let f = (-2.0 * half.tan()).exp();
```

As `psi` approaches `pi`, `tan(psi/2)` diverges and `f` must go to **zero**.
In f32 it goes to infinity instead:

| | |
|---|---|
| f32 `pi/2` | 1.5707964 |
| true `pi/2` | 1.5707963267948966 |
| `tan(f32 pi/2)` | **-2.2877332e7** |

The representable `pi/2` sits just *past* the true pole, so the tangent comes
back large and **negative**. Then `-2 * t` is large and positive, `exp`
overflows to `+inf`, and the shadowing denominator evaluates

```text
1 - f + f * chi * ratio   ->   1 - inf + inf   =   NaN
```

## Why it matters, and why it is not a contrived geometry

`psi = pi` is the whole back-scattering plane. `cos psi` is recovered from
`cos alpha = mu0 mu + sin i sin e cos psi` and clamps to `-1` for every facet
at the **limb and terminator** — which is precisely where roughness does its
work, as `2026-09-11_hapke_roughness.md` explains at length. So this was not a
corner: it was NaN across the part of the body the parameter exists to model,
and it propagated straight into the disc integral.

The smooth path is unaffected and returns a finite number at the same
geometry, because it never needs `psi`.

## Fix

A negative tangent here can only mean the argument crossed the pole, since
`psi/2` lies in `[0, pi/2]` by construction. So the sign is the test, and the
limit is zero:

```rust
let tan_half = half.tan();
let f = if tan_half < 0.0 || !tan_half.is_finite() {
    0.0
} else {
    (-2.0 * tan_half).exp()
};
```

## The test, and why the existing one was not enough

Reciprocity caught it — but by luck. It failed on `NaN vs NaN`, because two
NaNs compare unequal. A reciprocity check written as `(a - b).abs() < tol`
would have been **green**, since that comparison is false for NaN and a test
asserting closeness with `<` on NaN... does fail, but the diagnosis would have
read as an asymmetry rather than as a non-number. Either way, nothing in the
battery was *aiming* at finiteness.

`roughness_stays_finite_in_the_back_scattering_plane` now sweeps `alpha`
across its whole range for four `(mu0, mu)` pairs and five `theta_bar`, and
asserts `mu0e`, `mue`, `S` and the reflectance are all finite. Verified to
fail without the guard: `theta_bar=5 mu0=0.9 mu=0.2 alpha=1.85 ... S=NaN`.

**A NaN is not caught by a test that checks a value is close to another
value.** It is caught by a test that checks the value is a number. Those are
different assertions and the battery had only the first.

## Why one machine and not the other

Not established. The arithmetic is f32 on both and the geometry is identical,
so the difference is presumably in how `tan` is evaluated near the pole —
different libm, or the Mac's run being `cargo test --release` against this
one's debug build. Worth knowing that it is possible at all: **a
platform-dependent NaN in a physics path that both machines' suites are
supposed to cover.** Running the suite after a pull is what found it.

## Also corrected while here

`65f5794` removed the `(1 + N)` PCF scaling from the render's normal offset —
it was moving shadow edges rather than blurring them
(`2026-09-17_pcf_erosion.md`). Four places still described the old behaviour
and claimed it as the difference between the render and compute shadow paths:
`shaders/facet_shadow.wgsl`, `src/app/facet_shadow.rs`,
`tests/test_facet_shadow.py` and `2026-09-10_pinning_the_shadow_path.md`. The
offset is now the same `lb.x * k` in both, and **filtering is the only
difference**. Updated.

That is the second time these two shadow paths have drifted apart in their
documentation while agreeing in code, which is an argument for the comparison
living in a test rather than in four prose descriptions of it.
