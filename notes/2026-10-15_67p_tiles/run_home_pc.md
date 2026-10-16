# The home PC's run: tiles 2000 to 6488

From the home PC session of 15-16 October 2026 -- Ryzen 7 9800X3D (8 cores,
16 threads), 61 GB, Windows 11 -- on `handoff_home_pc.md` as it stood at
823ad6f, built `--release`. The Mac took the same range overnight not
knowing this run had started (the README says the home PC never started; it
had, at 17:23 on the 15th), so tiles 2000 to 6488 were solved twice, by the
same code: `src/shape` has not changed since. What this run adds is below.

## How long

| step | | time |
|---|---|---|
| SHAP7, SHAP7-4M, the 18 kernels | 1.5 GB | 3 min |
| the 5,219 images' labels | ten at a time, before `fetch` | 21 min |
| `fetch 2000 6489` | 28 GB kept | 1 h 52 min |
| `solve 2000 6489` | `WORKERS = 10` | 15 h 40 min |

The solve took 19 h 41 min by the clock: suspended for four hours in the
night, when the machine was wanted, and resumed where it stood. All 4,489
tiles solved, none failing; 3.17 GB in `out/67p_tiles/tiles/`. The pace by
spacing: 303 tiles an hour at 0.5 m, 222 at 1 m, 256 at 2 m; 69 CPU-s a
tile, 10.9 GB in memory at most.

PSA took about 10 s to answer each request, then gave 1-2.5 MB/s: ten at
once came to about 10 MB/s, where the Mac saw 20-27 (this link took 36 MB/s
from elsewhere at the same time). The labels one by one would have been
seven hours; `main.py` now fetches them eight at a time (15f9c9e).

`WORKERS`: the run kept 4 to 6 of the 16 threads busy after its first hour
(7 in it), other programs open beside it. Measured after it on 20 tiles at
0.5 m, nobody at the machine: 8.7 tiles a minute with 4 workers,
8.7 with 10, 7.6 with 16 -- the eight cores are full by 4. A tile's
threads in one process go as fast as in processes of their own (four
tiles, 36.3 s against 34.4 s), so it is not Python's lock. Why the run went
at 5 a minute where the same 10 workers did 8.7 after it is not
established.

## The 1 m and 2 m tiles: SHAP7 missing under the grid

The Mac's own share is all 0.5 m: the 711 tiles at 1 m (5667-6377, 240 m
across) and the 111 at 2 m (6378-6488, 480 m) are all in this range. Where
the surface turns -- the neck, the cliffs -- much of such a grid has no
facet facing up beneath it: `Maplet.from_surface` leaves `reference` NaN
there, the solver fills the whole grid anyway, and the heights go where no
image puts them.

| SHAP7 missing under the grid | tiles | moved over 5 m RMS from SHAP7 | no image used at the end | moved, median RMS |
|---|---|---|---|---|
| under 10% | 4,142 | 5 | 0 | 0.36 m |
| 10-50% | 211 | 15 | 0 | 1.3-2.0 m |
| 50-70% | 53 | 37 | 1 | 7.8 m |
| over 70% | 83 | 76 | 37 | 21.5 m |

Past 5 m by spacing: 5 of the 3,667 tiles at 0.5 m (8.2 m the most), 70 of
the 711 at 1 m, 58 of the 111 at 2 m. The 38 with no image used end with a
NaN residual and their heights 11 to 174 m RMS from SHAP7: t06086 (1 m),
SHAP7 missing under 83% of it, came out 34 m down and tilted 65 deg. The
same code from the same plan should give the Mac the same.

Only 40 of these 133 tiles are seen by a held-out image, so `score` cannot
find most of them, and leaving as SHAP7 the tiles below it on their
held-out images lets the other 93 into `compose`. A test that needs no
held-out image, on a tile's file:

```python
d = numpy.load(path)
moved = numpy.sqrt(numpy.nanmean((d["heights"] - d["reference"]) ** 2))  # km
missing = numpy.isnan(d["reference"]).mean()
bad = moved > 0.005 or not d["used"].any()
```

For the second pass and after: tiles smaller where the surface turns, or
the grid's points with no surface beneath them left out of the solve.

No image used (38):

```text
5824 5851 5865 5867 5952 6001 6086 6094 6116 6223 6287 6342 6344 6347 6373
6383 6393 6398 6407 6427 6433 6434 6441 6443 6444 6446 6451 6457 6470 6472
6473 6475 6476 6477 6478 6481 6483 6488
```

Moved over 5 m RMS, some image used (95):

```text
4235 4527 4856 5064 5418 5667 5846 5847 5860 5868 5896 5925 5926 5937 5940
5946 5954 5970 5972 5987 5996 6017 6020 6023 6056 6057 6074 6079 6085 6096
6097 6099 6101 6107 6119 6121 6138 6151 6158 6161 6162 6198 6221 6226 6235
6249 6259 6267 6276 6283 6285 6289 6295 6341 6351 6358 6364 6365 6367 6372
6384 6386 6388 6391 6395 6408 6409 6411 6413 6414 6416 6417 6418 6425 6428
6435 6436 6439 6442 6447 6449 6450 6452 6454 6455 6456 6458 6459 6461 6465
6466 6474 6479 6480 6487
```

## A tile depends on the tiles fetched with it

`fetch` keeps of each image the lines `lines_of` finds for its tiles'
boxes -- each tile's square, a quarter of its half-width above and below --
and `cut` takes its window from the tile's points, which can stand outside
that box, then clips it to the lines kept without a word. Under tile 2000,
SHAP7 runs from -21.7 to +7.1 m, its box +-15 m. Solved after fetching tile
2000 alone, its heights came out up to 0.85 m from the run's, the median
correlation the same (0.41 -> 0.70); solved alone again after the whole
range's fetch, bit-identical to the run's.

Over the range, 509 of 298,761 tile-image pairs were cut short, in 353
tiles (7.9%): one image a tile at the median, 17 lines short (523 at most),
26 of the tiles within 50 of the range's ends. The Mac, keeping the lines of
all 6,489 tiles, cuts no more short, so these tiles can differ between the
two runs for that alone. Sizing `lines_of` from the points `cut` uses would make
a tile's result its own.

The solve itself is deterministic: tile 2000 solved twice alone, and alone
against the ten-worker run, bit-identical.

## A Windows fault in `osiris.fetch_lines`

`kept = numpy.load(path)` keeps the .npz open, and Windows will not let
`os.replace` replace an open file -- `PermissionError(13, 'Access is
denied')`; macOS does. So on Windows an image whose kept lines must be
widened fails, the widened `X.npz.part.npz` left beside it. Here, the 75
images of the trial above: the widened files were moved into place (the
lines both held identical), and `fetch` run again found all 5,219 complete.
The fix: `with numpy.load(path) as kept:`.

## The correlations

The log's, each tile's images at its last level, the median; the 38 with no
image used left out.

| tiles at | tiles | images, median | SHAP7 -> solved | lower after |
|---|---|---|---|---|
| 0.5 m | 3,667 | 70 | 0.51 -> 0.66 | 414 |
| 1 m | 711 | 68 | 0.56 -> 0.72 | 50 |
| 2 m | 111 | 54 | 0.45 -> 0.70 | 8 |

## The tiles

`t02000.npz` to `t06488.npz` stay on the home PC, in `out/67p_tiles/tiles/`,
with the inputs in `C:/data`. They are not needed, the Mac having its own.
Set beside the Mac's they would say whether the two platforms solve a tile
alike: a tile not cut short above should differ by floating point at most.
