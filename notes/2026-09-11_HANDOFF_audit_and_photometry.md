# Handoff — the audit, the test backlog, and both halves of the photometry

Windows machine, 10–11 September. Everything below is committed and pushed;
nothing is running. 21 commits, 40 files, +5,226 lines.

The previous handoff (`2026-09-09_HANDOFF_axes_grid_gizmo.md`) is closed: its
`maturin develop` blocker is fixed properly, and its two open items about the
grid and the gizmo are done.

Three things happened, in this order: a few loose ends from the axes work, a
codebase audit that found the physics under-defended, and then — after a
question about Brož's polygonal shadowing — the photometry got both of its
missing halves.

## Where the suite stands

**77 Rust tests** (was 61) and **11 Python test files** (was 4). Five of the
Python files are physics that nothing tested before: conduction, view factors,
self-heating, Planck, scattering, plus the shadow path.

## Decisions you made, now recorded

- **`Float = f32` stays**, for geometry throughput — 3.1M-facet arrays halved
  and uploaded with no conversion, since WGSL has no f64 anyway. Costs written
  down in `2026-09-10_f32_decision_and_one_planck.md`. The one thing still
  unbounded is drift over a seasonal run's ~1e6 conduction steps.
- **Planck merged** into one Rust formula, with `radiance.py` broadcasting for
  it.

## The audit, and its own correction

`2026-09-10_code_quality_audit.md`. Verdict: **the engineering practices are
good and the physics was under-defended.** 61 tests of which 51 were app
plumbing and 2 were physics; nothing at all on `mesh.rs`, `radiance.rs`,
`roughness.rs` or `facet_shadow.rs`.

**Its headline finding was wrong and is corrected in place, not dropped.** It
called the compute/render shadow divergence a physics bug. Reading
`src/app/facet_shadow.rs` before touching anything showed two of the three
claimed divergences do not exist — the per-layer bias and matrix are already
passed correctly — and the third is deliberate and right, because the Sun is a
point source and PCF is antialiasing. The real defect was two docstrings
claiming the paths "cannot disagree", plus no test. Read that section before
trusting the rest of the note.

## Four traps found the hard way

These are the reusable part.

**A test that cannot fail is a comment.** Three of the new tests passed against
deliberately broken code on their first version:

- the shadow test passed with the bias multiplied by **100** — one favourable
  Sun angle moved false-lit 18 → 24 of 1540. Fifteen angles and an assertion on
  the *worst* one separates healthy from broken by 10x where the mean manages
  2.3x;
- the conduction reference was **not converged** at 91 samples per facet, so
  the first numbers were ~10 % low — `q4 rms` keeps climbing until the
  reference stops moving;
- the self-heating test would have passed with the emissivity dropped from
  `heating.emitted()`, because that balances perfectly at `eps = 1` and fails
  only at `eps = 0.9`. **A conservation test at the convenient parameter value
  proves less than it looks.**

**Different checks catch different classes of error.** A 3 % error in the
view-factor kernel fails three of seven checks but *not* reciprocity, because
a uniform scale preserves an identity. The closed forms and the identity are
both needed.

**An aggregate can hide a per-facet bug.** The polygon clipper's first depth
test was wrong by up to 0.435 on grazing facets, while the area-weighted lit
fraction agreed to 2e-4 — a disc-integrated check would have passed it.

**Testing an exact method against a sampled one needs convergence, not a
tolerance.** A fixed tolerance measures the sampler. Refine the reference and
watch the gap fall.

## The photometry, which did not exist

A visible-band light curve was not computable at all before this. Both halves
now exist, in Rust with Python bindings.

**`kalast.scattering`** — Lambert, Lommel-Seeliger, the `c·LS + (1−c)·L` mix,
and Hapke IMSA. Two bugs caught by the tests: the **`mu0` convention was
inconsistent** (Hapke's own `r` folds `cos i` in and the others do not, so
swapping laws would have changed the answer by `cos i` — a wrong pole solution
rather than an error), and the **Henyey-Greenstein lobes were backwards**,
which normalisation cannot see because swapping them leaves the sphere average
at exactly 1.

**`src/shadowing.rs`** — exact partial shadowing by polygon clipping. No
Clipper2: our facets are triangles, so `A \ B` decomposes into at most three
convex pieces by half-plane clipping, which needs no dependency and no C++
toolchain in the `maturin develop` path. Worth 5–10x on a light curve, 0.68 →
0.07 mmag at 5120 facets. 7 / 34 / 156 ms at 320 / 1280 / 5120 facets —
**timings neither Brož's paper nor his 96-page deck gives at all**.

It does **not** replace the GPU shadow map. That stays for the thermophysical
model and for 3.1M facets, where the quantisation averages out over a rotation
and this would be hopeless.

## Open, in the order I would take them

1. **`theta_bar` — Hapke's macroscopic roughness — is refused, not
   implemented.** So the pair is exact area × *smooth-surface* Hapke.
   `theta_bar = 0` is exact rather than approximate, so what is there is a
   complete model of the smooth case. The 1984 correction is a page of case
   analysis with no closed form to test against, which is why it was left;
   `Hapke1984` is already in the bibliography. **This is the last known gap in
   the photometry** and wants a deliberate decision, not drift.
2. **The audit's remaining test backlog**: radiance band integration
   (`radiance.rs`, `BandRadiance`), roughness (`roughness.rs`, 350 lines —
   `examples/analytical/roughness.py` already holds four checks including
   Kuehrt's published `F5 > F1 > F6`), and transient conduction
   (`slab_relaxation.py`). All three have their method sitting in
   `examples/analytical/` already; read those before writing new physics.
3. **Smaller audit items, untouched**: 12 dead shaders (~730 lines, including
   `mesh_old.wgsl`), 134 `.unwrap()`s of which 28 are at the Python boundary —
   a second `App()` panics with `RecreationAttempt` instead of raising — and
   `SOLAR_CONSTANT = 1369.0` with no provenance comment (modern TSI is 1361).
4. **`test_stubs`'s case list is hand-maintained**, which is the weakest thing
   in that file: a new `#[pyclass]` is checked only if somebody remembers to
   add it. `Hapke` shipped with a broken stub past every green check until it
   was added.

## The lightcurve project, for Eli

New, and the reason the Brož question came up. `2026-09-10_polygonal_shadowing_assessment.md`
has the method assessment; `2026-09-11_mutual_events_measured.md` has the
mutual-event numbers. Nothing is wired into a runnable light curve pipeline
yet — the pieces exist, the driver does not.

## Practical notes for this machine

- **Build with `python tools/develop.py`**, not `maturin develop`. VS Code's
  language server keeps `kalast/_rs.pyd` mapped for the whole editor session,
  so the plain command throws the finished build away at the copy step with
  `os error 32`. The wrapper renames the old module aside first; renaming a
  mapped image is allowed where writing it is not. macOS is unaffected.
- **Brož's slides** are on the ROB cloud, not only on the Mac:
  `https://cloud-as.oma.be/index.php/s/dG9oeb422NZcGtD` serves a 532 MB zip of
  the whole folder; `unzip -j houches.zip '2024.02-houches/presentations/11_Broz.pdf'`
  gets the 36 MB deck.
- **`Broz2023` was added to `C:\projects\bibs\bibliography.bib`** (209 → 210)
  in the JabRef style, under the existing `Lightcurve Photometry` group. **It
  is uncommitted in that repo** — review and commit it there.
- The 30 modified example scripts are the usual `local_paths.toml`
  substitutions and are not to be committed.
