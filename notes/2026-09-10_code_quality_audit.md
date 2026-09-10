# Code quality audit — 17 days, 280 commits, +46k lines

Asked directly: did we go too fast, is anything conflicting, is anything
implemented twice, and what is the test coverage on the physics.

Measured rather than judged. Everything below has a command or an experiment
behind it, and the two most important findings were confirmed by running the
code, not by reading it.

## Scale, for context

| | |
|---|---|
| commits since 24 August | 280 |
| net change | 287 files, +45,992 / -1,350 |
| Rust | 27,865 lines |
| WGSL | 2,785 lines |
| Python | 4,037 lines |
| Rust tests | 61 |
| Python tests | 4 files |

## What is genuinely good, and should not be lost

Worth stating before the problems, because these are the practices that make
the rest fixable rather than fatal.

- **`notes/` is better than most projects manage.** Dated write-ups, negative
  results recorded (`lto = "fat"` gave nothing; one-encoder-per-layer gave
  nothing), and handoffs between machines. Decisions do not have to be
  re-derived.
- **Generated code with two-way guards.** The `.pyi` stubs, the config panel
  and the Python bindings are generated and then checked *both* against the
  generator *and* against a real object. That combination caught four real
  bugs: `debug_light_cube_fit`, `facet_labels`, `selection_color`,
  `colorbar_border`. This is real engineering, not ceremony.
- **Comments carry the why, including rejected approaches.** The four grid
  failures, the shadow-bias calibration, the reversed-Z rationale.
- **Only 2 TODO markers in 27k lines.** Nothing is being deferred silently.

The problems below are concentrated almost entirely in **the physics**, which
is the part with the fewest tests and the most duplication — the exact inverse
of where the care has gone.

---

## 1. The compute shadow path and the render shadow path disagree, and only the docstrings are wrong about it

**Corrected after investigating.** The first version of this audit called this
a physics bug and ranked it most serious. That was wrong, and the way it was
wrong is worth keeping, because it is the trap the code sets for a reader.

### What is actually shared, and what is not

`shaders/mesh_shadow.wgsl` shadows the render. `shaders/facet_shadow.wgsl`
shadows the TPM. Two things I listed as divergences are not: the Rust caller
in `src/app/facet_shadow.rs` passes the queried body's **own** layer bias
(`layer_bias.x/y/z`) and that layer's **own** `light_view_proj`, with a
comment explaining that using the scene-wide values instead reported Deimos
at 0.55 % shadowed where it should be ~46 %. That was already found and
already fixed.

What genuinely differs, at `shadow_pcf > 0` only:

| | `mesh_shadow.wgsl` (render) | `facet_shadow.wgsl` (physics) |
|---|---|---|
| normal offset | `lb.x * (1.0 + shadow_pcf) * k` | `lb.x * k` |
| filtering | PCF kernel, `(2N+1)^2` taps | single `textureLoad` |

At `shadow_pcf = 0` these are the same expression and the same single tap.

### And the compute path is right to differ

The Sun is modelled as a point source, so occlusion is binary. PCF is
image-space antialiasing; it has no physical referent, and blurring the
terminator would put softness into the boundary condition that nothing in the
model asks for. `mesh_shadow.wgsl` says as much in passing -- `lb.x` is "the
right separation for a single tap" -- and the `(1 + shadow_pcf)` factor exists
only to compensate for a kernel that reaches N texels away, which the compute
path does not have.

More decisive: `notes/2026-09-08_shadow_bias.md` tuned the compute path's
slope, floor and offset against **ray-traced ground truth** -- a ray from each
facet centroid to the Sun over 15 Sun angles -- and settled on
`slope = 1, floor = 1, offset = sqrt2` at single-tap. Adding PCF would
invalidate that tuning. The same note already records, in its ruled-out
table, that "`shadow_pcf = 4` changes nothing -- the compute path does not
filter".

### So what is the defect?

Not the numbers. The **claims**:

- `facet_shadow.wgsl` header: "reading the same shadow map the render pass
  samples, with the same projection and the same depth bias, **so that what
  this reports and what you see rendered cannot disagree**."
- `occluded()` doc: "mirroring the fragment shader's shadow lookup **exactly**
  (normal offset, y flip, bias)".

Both are false at `shadow_pcf > 0`, and they are exactly the sentences that
would stop someone re-deriving the difference when a rendered figure and a TPM
number fail to line up. `crater_self_shadow` has `shadow_pcf = 8` commented
out one line above its config, so the discrepancy is one keystroke away.

Second defect: **there was no test on any of it.** The ray-traced sweep that
validated the bias parameters lived in a note as a one-off. Nothing would
catch a regression, and nothing pinned the `shadow_pcf` invariance that the
design depends on.

**Both are fixed**: the docstrings now say what is and is not shared and why,
and `tests/test_facet_shadow.py` asserts the invariance and re-runs the
ray-traced sweep as a regression bound. See
`notes/2026-09-10_pinning_the_shadow_path.md` for how the budgets were
calibrated -- the first version of that test passed against a deliberately
broken shader, which is worth knowing before trusting it.

### Confirmed by experiment

`examples/crater_self_shadow`'s mesh, one Sun position, `facet_shadow(0)`
after the pipeline settles, three PCF settings, each in its own process:

    pcf=0 / 4 / 8:  mean_shadowed = 0.56054688, identical, 0/2048 differing

Invariant, as designed -- but as an accident of nobody having written it down
as a property, rather than as something asserted.

## 2. The entire physics core runs in f32, and nobody recorded that as a choice

`src/lib.rs` has a `use_f64` feature. It is **not** in `default`, and
`pyproject.toml`'s `[tool.maturin] features = ["python"]` does not enable it
either. So every build anyone actually runs — `maturin develop`,
`cargo build`, the wheel — has `Float = f32` for temperatures, radiance,
geometry, conduction, everything.

Demonstrated:

    >>> kalast.tpm.emit.planck(300.0, 11e-6)
    9573183.0            # a whole number: the f32 mantissa is exhausted

Three consequences, in increasing order of concern:

1. **The Rust and numpy Planck agree only to ~1e-6 relative**, which is f32
   epsilon, not agreement to double precision. The numpy side is explicitly
   `dtype=numpy.float64`.
2. **`exp()` overflows f32 at x > 88**, where f64 goes to 709. So Rust
   `planck` returns *exactly* `0.0` where numpy returns a finite value —
   measured at T=300/0.5 um, T=200/2 um, and T=20/8 um. The operating range
   for TIRI (8–14 um, 100–400 K) is clear of this, so it is latent rather than
   active, but nothing marks the boundary and nothing warns at it.
3. **The CPU TPM was validated against the GPU TPM to 1.5e-05 K.** That figure
   is at the edge of f32 resolution for a ~300 K quantity, so it may be
   measuring the precision floor rather than the agreement.

This wants a decision recorded either way. f32 is defensible — the GPU path is
f32 regardless, and the memory saving at 3.1M facets is real — but it should
be a written choice with the conduction accumulation error bounded, not a
default nobody revisited.

## 3. Planck's law is implemented twice, with its own constants

- `src/tpm/emit.rs:8` `planck(t, w)` — Rust, from `crate::util::TWO_HC2` and
  `HC_PER_K`.
- `kalast/tpm/radiance.py:80` `planck(temperature, wavelength)` — numpy, from
  `_H`, `_C`, `_KB` **redefined locally at lines 69-71**.

`kalast/util.py` already re-exports the Rust constants (`PLANK_CONSTANT`,
`SPEED_LIGHT`, `BOLTZMANN_CONSTANT`), so the Python module is bypassing the
single source of truth it already has. The values happen to match today.
Nothing enforces that.

The numpy version exists for a real reason — it broadcasts over arrays for
band integration, which the scalar Rust function cannot. That is an argument
for **exposing an array-shaped Rust function**, not for a second formula.

Related naming wart: the constant is spelled `PLANK_CONSTANT`, and it is
public and exposed to Python.

## 4. 1,787 lines of Python numerics that never call Rust

Unchanged since `2026-09-09_rust_core_audit.md` — this is open decision 1,
restated here with the measurement:

| file | lines | calls into `_rs` |
|---|---|---|
| `heating.py` | 471 | 0 |
| `explicit.py` | 362 | 0 |
| `implicit.py` | 345 | 0 |
| `radiance.py` | 226 | 0 |
| `routine.py` | 201 | 0 |
| `nonuniform.py` | 131 | 0 |

By contrast `core.py`, `emit.py`, `properties.py` and `column.py` are 3–15
line wrappers that do nothing but re-export Rust. The split is clean, which
makes the size of the untranslated half easy to see.

## 5. Test coverage: the plumbing is tested, the physics is not

61 Rust tests, distributed:

| area | tests |
|---|---|
| `src/app/**` — frame, window, gizmo, cargo, gui, hemicube | 51 |
| `src/app/gui` | 8 |
| **`src/tpm/**` — all physics** | **2** |

Both TPM tests are in `core.rs` and are guards, not formula checks:
`insolation_is_never_negative` and `insolation_unchanged_when_lit`.

**Zero tests** in, among others:

| file | lines | what is untested |
|---|---|---|
| `src/mesh.rs` | 1,425 | facet areas, normals, centroids, flatten/smoothen, bounds |
| `src/tpm/radiance.rs` | 341 | band integration |
| `src/tpm/roughness.rs` | 350 | crater roughness correction |
| `src/app/facet_shadow.rs` | 247 | the shadowing above |
| `src/app/occlusion.rs` | 366 | occlusion queries |
| `src/app/facet_id.rs` | 471 | picking |

On the Python side there are four test files and **all of them test generated
code or startup** — `test_stubs.py`, `test_config_panel.py`,
`test_config_bindings.py`, `test_editor_startup.py`. There is no test of any
physics in Python at all, including the 1,787-line numpy TPM that is the
reference the GPU path was validated against.

**The one piece of physics with a real test is the right kind**:
`hemicube::delta_form_factors_close_to_unity` — view factors over a hemicube
must sum to unity. That is a conservation law checked against a closed form,
and it is exactly the model for what the rest needs.

### The tests worth writing first, in order

Each is a closed form or a conservation law, so none of them needs a reference
dataset:

1. **Facet geometry** (`mesh.rs`): area of a known triangle; the areas of a
   tetrahedron summing to the analytic total; normals unit-length and
   outward-facing after `flatten`; `flip_facets` inverting exactly the facets
   named.
2. **Energy conservation**: absorbed + reflected + emitted balancing for a
   facet in equilibrium; `equilibrium_temperature` inverted through the
   emitted flux returning the input. **Partly done** —
   `tests/test_self_heating.py` covers the conservation law on a sealed
   isothermal cavity, which is the strongest form of it and also the only
   coverage `kalast.tpm.heating` has. `equilibrium_temperature` itself is
   still unpinned.
3. **Planck** (`emit.rs`): Wien's displacement law — the peak of
   `planck(T, ·)` sits at `2.898e-3 / T`; Stefan–Boltzmann — integrating
   `planck` over all wavelengths and multiplying by pi gives `sigma T^4`. Both
   are exact and catch a constants error instantly. Add the f32 overflow
   boundary as an explicit case.
4. **Shadowing**: the cross-check in finding 1. **Done** —
   `tests/test_facet_shadow.py`.
5. **Radiance band integration**: a flat unit response over a narrow band must
   return the band-centre spectral radiance times the width. Still open, and
   now the last of the cheap ones — `src/tpm/radiance.rs` and
   `kalast/tpm/radiance.py` are 567 lines between them with nothing on either.

   Also still open, and worth doing at the same time since they share a
   module: the view-factor *kernel* is pinned by `tests/test_view_factors.py`
   but `src/tpm/roughness.rs` (350 lines) is not, and
   `examples/analytical/roughness.py` already has the four checks for it —
   exact limits, convergence, Kuehrt's published `F5 > F1 > F6`, and the
   grazing-emission divergence. `slab_relaxation.py` likewise covers the
   *transient* conduction response that `test_conduction.py` does not.
6. **Conduction** (`explicit.py` / `implicit.py`): a semi-infinite solid under
   a sinusoidal surface flux has an analytic thermal-wave solution — skin
   depth and phase lag. **Done** — `tests/test_conduction.py`.

   Worth recording that the validation already existed.
   `examples/analytical/sinusoidal.py` had been checking exactly this from the
   start, with eight error figures and an order-of-accuracy table. It *prints*
   them, so a regression was only ever caught if somebody ran the example and
   read the output — which is the whole difference between a demonstration
   and a test. Turning that into assertions needed no new physics and was the
   cheapest coverage on this list.

   So before writing items 2, 3 and 5 from scratch, look in
   `examples/analytical/` first: `cavity_heating.py`, `view_factors.py`,
   `roughness.py`, `slab_relaxation.py` and `tpm_gpu_vs_cpu.py` are all
   sitting there in the same shape.

## 6. Smaller things

- **12 dead shaders, ~730 lines**, never referenced from any Rust source:
  `compute`, `cubes_shadow`, `equirectangular`, `hdr`, `light_save`,
  `mesh_flat`, `mesh_old`, `pentagon`, `sky`, `texture`, `uniform_camera`,
  `with_depth`. `mesh_old.wgsl` in particular invites being read as current.
- **134 `.unwrap()` in `src/`, 28 of them in `src/py/`**, where a panic crosses
  into Python as `PanicException` rather than a typed exception. Hit while
  writing this audit: constructing a second `App()` in one process panics with
  `RecreationAttempt`. That is a legitimate restriction, but it should be a
  `RuntimeError` carrying a sentence that explains it.
- **`skin_depth_1` and `skin_depth_2pi` documented their period argument as
  "density".** The formulas were always the period's and are correct; only the
  comments were wrong, which is the kind of mismatch that returns a plausible
  number from a wrong argument. Fixed, and both are now pinned by
  `tests/test_conduction.py`.
- **`SOLAR_CONSTANT = 1369.0`**, commented "integrated solar flux at 1 AU".
  The modern accepted TSI is 1361; 1369 appears in older TPM literature.
  Probably deliberate, but the provenance is not written down, and it
  propagates into every insolation.
- **Two config structs**, `Config` and `AppConfig` in one file, already noted
  as a standing source of mistakes — the `background` collision.

## Verdict

**The engineering practices are good and the physics is under-defended.** The
pace has not produced sloppy code; it has produced code whose *care is
unevenly distributed*. The generated-code guards, the notes and the comments
are all well above average. But the same 17 days added a second shadow
implementation that has already silently diverged from the first, kept a
second Planck's law with its own constants, and left the entire thermophysical
core at two tests.

Nothing here is a rewrite. Finding 1 is a real bug and should be fixed and
pinned with a test. Finding 2 is a decision to record. The rest is a test
backlog with an obvious ordering, and the hemicube test already shows the
shape it should take.
