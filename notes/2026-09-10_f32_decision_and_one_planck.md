# f32 stays, and there is one Planck now

Closes findings 2 and 3 of `2026-09-10_code_quality_audit.md`.

## Decision: `Float = f32`, deliberately

Asked and answered:

> im ok to stay f32 core, this is to allow fast and smooth loading mesh time
> and sending to shader

So this is now a recorded choice rather than a default nobody revisited, which
is all the audit asked for. `use_f64` remains available as a feature and is
not enabled by `default` or by `pyproject.toml`.

The reason is geometry throughput: vertex and facet arrays at 3.1M facets are
halved, and they upload to the GPU without a conversion pass because WGSL has
no f64 anyway. Doubling them would cost memory and a conversion on the hot
path to buy precision the renderer cannot use.

What that costs elsewhere, so it is on the record:

- **Physics constants exposed to Python round to f32.** `skin_depth_1` agrees
  with `sqrt(D P / pi)` to ~1e-8 absolute on a 1 cm skin depth, not to f64.
  `tests/test_conduction.py` sets its tolerances from that and says why.
- **`exp` overflows at x > 88 rather than 709.** Measured to be outside the
  working range: over 6-16 um and 30-500 K, **no sample** is driven to zero.
- **The CPU/GPU TPM agreement of 1.5e-05 K** is at the resolution floor for a
  300 K quantity, so it is a bound on the disagreement rather than a
  measurement of it. Worth remembering before quoting it as an accuracy
  figure.

None of these is a reason to revisit, and the trade is the right way round for
what this engine is for. But a seasonal run accumulating over ~1e6 conduction
steps is the case that would test it, and nobody has bounded that drift.

## One Planck, and what testing it turned up

`kalast/tpm/radiance.py` carried a second implementation in numpy, with `_H`,
`_C` and `_KB` redefined at lines 69-71 -- while `kalast/util.py` was already
re-exporting those very constants from Rust. Two closed forms agreeing only
because nobody had changed either.

`radiance.planck` now broadcasts and calls `emit.planck_array`, a new
`#[pyfunction]` that runs the one formula in `src/tpm/emit.rs` elementwise.
Broadcasting stays in numpy, which does it far better than a hand-rolled Rust
version would; the Rust side is a loop over one expression. The documented
"any shapes that broadcast together" contract is unchanged, and the two
internal callers needed no edit.

### Measured before switching, not after

The numpy version was f64 and the Rust one is f32, so the merge is a precision
change and had to be justified rather than assumed. Over 6-16 um and
30-500 K, against a synthetic TIRI-like band:

| | |
|---|---|
| worst pointwise difference | 1.2e-5 relative, at 30 K |
| samples f32 drove to zero | **0** |
| band-integrated table, worst | 3.8e-6 relative |
| the table's own interpolation error | **5.1e-5** |
| brightness temperature round-trip | identical, 0.00 K |

So the change is 13x under the error `BandRadiance` already accepts by
tabulating. If `use_f64` is ever enabled it simply gets better.

### The test found a real defect in the survivor

`tests/test_planck.py` checks Wien's displacement law and Stefan-Boltzmann --
consequences of the formula involving constants it does not mention, so an
error in `h`, `c`, `k_B` or a wavelength power moves them where a
self-consistency check would not. It also checks both asymptotic limits.

The Rayleigh-Jeans limit failed, and not for the reason it first looked like.

Two wrong diagnoses on the way, both worth keeping:

1. **First attempt asserted "within 5e-3 of Rayleigh-Jeans" at 2 mm.** That
   fails against *correct* code: `B/B_RJ = x/(e^x - 1) = 1 - x/2 + O(x^2)`,
   and at 2 mm and 300 K, `x/2` is already 1.2e-2. The deviation is physics.
   Only its **rate** distinguishes a right formula from a wrong one, so the
   check became "the fractional shortfall approaches `x/2`".
2. **Then it still failed at 50 mm**, by 2 %. That one was real.
   `src/tpm/emit.rs` computed `(...).exp() - 1.0`. At 50 mm and 300 K the
   exponent is 9.6e-4, `exp` of it is just above 1, and subtracting 1 in f32
   destroys nearly every significant digit -- ~6e-5 relative, which is 2 % of
   a 4.8e-4 shortfall.

`exp_m1` computes that small difference directly and fixes it: the ratio now
runs 0.9960, 0.9984, 0.9992, 1.0003 across 2-50 mm, converging as it should.
Applied to `planck_photon_count` for the same reason.

**The numpy implementation that was deleted already used `expm1`.** So the
merge would have silently traded away a correctness property of the copy being
removed, in a regime nothing in the repo currently exercises, if the limit had
not been tested. That is the argument for testing a merge rather than
eyeballing that two formulas "look the same" -- they did look the same, and
one of them was better.

Outside the thermal IR this never mattered: in the 8-14 um band at 30-500 K
the exponent is O(1) and there is no cancellation. It matters for anything
long-wavelength, and `planck` is public.

## Backlog after this

Item 3 done. Remaining: radiance **band integration** (`radiance.rs`, and the
`BandRadiance` table machinery), **roughness** (`roughness.rs`, 350 lines,
with `examples/analytical/roughness.py` already holding four checks), and
**transient conduction** (`slab_relaxation.py`).
