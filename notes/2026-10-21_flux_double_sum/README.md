# `flux` summed in double precision, and a binary as two lists

21 October 2026. The user, of the light-curve example's note that `flux` is
off on DART's finest shape models: "why??". Then, of the index arithmetic
a binary needed: "flux could instead accept a list of bodies ... yes, this
would make more sense for user".

## The fault

Seen earlier the same day (`2026-10-21_lightcurve_example/`, *The shape
models' resolution*): on DART's 3,145,728-facet models, at 03:00 on
2022-11-18 -- no event, the bodies 1.17 km apart -- the system came out
6.8 mmag fainter than its two bodies apart, nothing overflowed; on the
49,152-facet models, 0.0.

**Not the shadows.** With both passes off, nothing occludes and the order of
the facets cannot matter -- yet it did (the system against its two bodies
apart):

| facets | Didymos's facets first | Dimorphos's first |
|---|---|---|
| 49,152 | 0.00 mmag | 0.00 |
| 3,145,728 | 7.12 mmag faint | 0.01 |

**The sum.** The Python build is `f32` (`Float`, no `use_f64`), and
`flux_at` summed its facets in it. Past Didymos's facets the running sum is
1.4e-3, where `f32`'s step is 1.2e-10; each of Dimorphos's 0.24 m facets
adds some 1e-11, about half a step, and was rounded to nothing or to a
whole step.
Dimorphos lost 15 % of its light: the 8.9e-6 the system missed against its
5.9e-5. Summed first, from zero, its facets keep it. At 49,152 facets a
facet's share is 64 times larger, and the rounding stays at 3 parts in a
million.

The shadowing's grid was looked at first: a scene of both bodies makes its
cells 2.7 m wide against 0.3 m for Dimorphos alone, so a facet meets
hundreds of candidates rather than about 8. It changes the work, not the
answer: the candidates are kept by their bounding boxes and depths, which
are exact, whatever cells they share.

## The fix

`flux`, `illuminated` and `lit_area` are `f64` in `flux_at`, whatever
`Float` is; each facet's term is still `Float`, good to its own 1e-7. The
light curve's `curve` goes through `flux_at` and is fixed with it.

- `lightcurve::tests::a_million_small_facets_after_a_large_one_still_count`:
  a triangle, then a million each a hundred-millionth of it. The module
  built before the fix comes out 0.99 % faint, the million gone; now within
  1e-6.
- The order test above: 0.00 mmag both ways at both resolutions.
- The events on the finest models now (49,152 facets; before the fix):

  | 2022-11-18 | drop |
  |---|---|
  | 00:52, Dimorphos's shadow | 71.2 mmag (72.7; 75.4) |
  | 03:00, no event | -0.0 (0.0; 6.8) |
  | 05:10, Dimorphos hidden | 46.1 (46.4; 42.5) |

## Two lists for a binary

`flux` took one mesh, so a binary script stacked the bodies' vertices and
offset the second's indices by the first's vertex count:

```python
flux(numpy.vstack([v1, v2]), numpy.concatenate([i1, i2 + len(v1)]), sun, observer, law)
```

Now, each body's indices its own:

```python
flux([v1, v2], [i1, i2], sun, observer, law)
```

In the binding (`src/lightcurve.rs`, `bodies`): a list or tuple whose first
item is 2-D is a list of bodies, their triangles appended into the one list
`flux_at` takes; a list of 3-number rows is still one body's vertices. A
count of indices other than the bodies' raises. `tests/test_lightcurve.py`:
the two lists give the stacked mesh's flux to the bit, a moon before its
primary takes 75 mmag that the two apart do not, rows are one body, a
missing list raises. The stubs are generated from the binding's signature,
`numpy.ndarray | Sequence[numpy.ndarray]`.

## A plot window beside kalast's

Making the light curve as the night is drawn (`scene_lightcurve.py`,
`2026-10-21_lightcurve_example/`), a matplotlib window at the night's end
took the process down on macOS: "tried to get a delegate that was not the
one Winit has registered", a panic in a function that cannot unwind. Also
after `app.close()`: winit's application delegate stays the process's. Qt
and Tk would take the application over the same way. So the example plots
draw to a file with `Agg` and open it in the system's viewer, as
`examples/sphere/tpm_plot.py` does; `docs/API.md` already says the UI app
gives scripts `agg`.
