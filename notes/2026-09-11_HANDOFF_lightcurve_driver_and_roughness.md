# Handoff — the light curve driver, and Hapke's roughness

macOS machine, 11 September, continuing the same day's Windows handoff
(`2026-09-11_HANDOFF_audit_and_photometry.md`), which is now closed: its
headline open item was `theta_bar`, and that is done. 4 commits, 29 files,
+2,658 lines. Everything committed and pushed; nothing is running.

**The photometry's forward model is complete.** A visible-band light curve of
a shape with a spin state and a scattering law can be computed, end to end,
from Rust or from Python.

## What happened

The incoming handoff ended "the lightcurve project for Eli has its pieces now
and no driver". So: the driver, then the last gap it exposed.

- **`src/lightcurve.rs`** — the disc integral. `flux()` for one epoch of
  placed geometry (which is how a binary goes through it) and `lightcurve()`
  for a rotating body, carrying the Sun and observer into the body frame
  rather than rotating the mesh. Spin state in convex inversion's convention,
  so a published pole solution needs no translation.
  `notes/2026-09-11_lightcurve_driver.md`.
- **Hapke's `theta_bar`** — implemented, Hapke (1984).
  `notes/2026-09-11_hapke_roughness.md`.
- **`scattering::LommelSeeligerLambert`** — the mix as a parameter set, so
  "a law" is one argument. It is Lambert at `c = 0` and pure LS at `c = 1`
  exactly, which is why `Law` has two variants rather than four.
- **`examples/lightcurve/main.py`** — runs on `res/` alone, no data paths.
- `cargo test --release` is green end to end for the first time in a while:
  two doc comments had used four-space indented blocks, which rustdoc compiles
  as Rust. Fenced, not reworded.

Suite: **87 Rust tests** (was 77) and **12 Python files** (was 11).

## The decision you made, now recorded

**`theta_bar` is implemented rather than refused**, asked and answered on 11
September. The refusal now covers only an out-of-range value, `[0, pi/2)`.

**And the estimate I gave you before implementing it was wrong by an order of
magnitude** — this is the part worth carrying forward. I placed it by analogy
against the other Hapke parameters, which could be measured: tripling `w`
moves a normalised rotation curve 3-5 mmag, deleting the opposition surge 1.4.
Measured, `theta_bar = 30 deg` is **23 mmag rms at 20 deg phase and a 15 %
change in the curve's amplitude** — the quantity an axis ratio is fitted to.
`w` and `b0` move the curve's *level*; roughness acts near the limb and
terminator, whose share of the disc changes as an elongated body turns.

Every other term in this photometry was settled by measuring it. That one had
been settled by argument, and the argument was wrong.

## Five traps, which are the reusable part

**A test that "something has an effect" does not say which something.** The
first concave-shape test asserted only that occlusion changed the flux. It
did, by 6 mmag — and all of it came from the observer pass, because at that
Sun direction the shape self-shadowed nothing, to six figures. A completely
dead Sun-side clipping would have passed. Each pass is tested alone now.

**Reciprocity cannot see an inverted branch condition.** Hapke's `i <= e` and
`i > e` branches exist to make the model reciprocal, so reciprocity is the
obvious test and a good one — it catches a wrong term inside a branch at once.
But swapping the branches *wholesale* leaves it completely green, because they
are each other's mirror image: exchanging them preserves the very symmetry
they were built to provide. `S = 1` at zero azimuth catches it, holding on one
branch only.

**And that test had a numerical blind spot.** Its first geometries were all
near-normal incidence, where `E2` underflows, the correction drops out of
*both* branches and `S` is 1 either way. It discriminates only where both
angles are far from normal. A new way for a test to be unable to fail: not a
loose tolerance, not a conservation law at a convenient parameter value, but a
test sitting where the term it probes has underflowed.

**An exact identity and a discretisation error can sit in the same
measurement.** A 2:1 ellipsoid's light curve amplitude is `2.5 log10(a/b)` at
80 facets as well as at 5120 — an icosphere's projected area is isotropic to
~1e-7, so there is nothing to converge and the 5120 row is the *worst* of the
four, being f32 rounding. The curve *shape* is exact at no resolution and
converges as `N^-0.94`. A fixed tolerance on the second would have measured
the mesh.

**Of five deliberate breaks on the driver, two were caught by exactly one
check each.** A mirrored rotation is invisible to every curve test, since the
ellipsoid curve is symmetric in time — only the Rust `rotation_is_prograde`
test sees it, and without it a real target's curve would come out
time-reversed with everything green.

## Two pre-existing defects, found by writing the example

- **Every generated stub's numpy annotations were `numpy.object`**, an
  attribute numpy removed in 1.24, because `resolve()` rewrote dotted names
  identifier by identifier. **53 of them across 9 committed stub files**, each
  an error of exactly the kind that function exists to prevent. Fixed by
  resolving a dotted name by its root.
- **`test_stubs`' hand-maintained case list is now loud rather than silent** —
  open item 4 of the incoming handoff.
  `test_every_generated_stub_class_is_covered` fails when a stub class is in
  neither the case list nor an explicit `UNCOVERED` set. It immediately found
  two classes a manual count had missed, and documents **16 that nothing
  checks** — a backlog that can now shrink but not grow. It does not construct
  objects automatically, which is what would remove the list entirely; it
  removes the silence, which is the part that bit.

## Open, in the order I would take them

1. **Nothing reads real photometry.** The forward model is complete; a fit
   needs observed curves, a chi-squared and a minimiser. This is what stands
   between the pieces and a result for Eli, and it is now the largest gap.
2. **A binary still needs its geometry placed per epoch** by the caller and
   `flux()` called. That works — it is what `mutual_event.py` does by hand —
   but the convenience that takes two bodies and an orbit does not exist.
3. **The LS+Lambert mix has no phase function**, so a fit spanning a range of
   `alpha` wants Hapke, or an empirical phase function applied outside.
4. **The audit's remaining test backlog**, untouched here: radiance band
   integration, roughness (`tpm::roughness`, the Kuehrt one), and transient
   conduction. All three have their method sitting in
   `examples/analytical/` already.
5. **Smaller audit items**, untouched: 12 dead shaders, 134 `.unwrap()`s of
   which 28 are at the Python boundary, `SOLAR_CONSTANT = 1369.0` with no
   provenance.
6. `bond_albedo()` is the smooth-surface closed form; roughness changes the
   Bond albedo and that is not reflected there.

The older thermal line is untouched and still queued: the **GPU TPM/radiance
port**, then the **GIS3D TIRI re-run** with heating on, plus the phase-1
spin-up re-run at 100k Didymos / 10k Dimorphos.

## Practical notes for this machine

- **`target/` had grown to 73 GB** and `cargo clean --profile dev` reclaimed
  **62 GB** of it — 509,335 files, 110 GiB *apparent*, the gap being cargo's
  hard links, which is why a size-summing tool reported ~131 GB. Artifacts
  went back to **18 July 2025** while the current stable toolchain was
  installed 24 August 2026, and a toolchain change invalidates every artifact
  built before it without deleting any. `target/release` was left intact and
  is still incremental (2.2 s). This will recur: cargo never garbage-collects.
  `cargo install cargo-sweep` then `cargo sweep --installed` is the fix and is
  **not** set up yet.
- `target/spirv-builder` (174 MB, 18 July 2025) is referenced by nothing in
  the source tree — a leftover from an abandoned rust-gpu experiment. Not
  deleted.
- The examples' data paths need no substitution here: `/Users/gregoireh/data`
  *is* this machine's layout, and `local_paths.toml` matches.
