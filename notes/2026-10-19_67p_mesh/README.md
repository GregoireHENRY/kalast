# Comet 67P's mesh: SHAP7 with the tiles in it

19 October 2026. The tiles of `2026-10-16_67p_second_pass/` and
`2026-10-19_67p_example_two_passes/` into one shape model: SHAP7 where no
good tile is, each tile at its own spacing elsewhere.

## Progress

- [x] The results in the example's numbering, in
  `/Users/gregoireh/data/mesh/67p/tiles_v2/`: the first pass's 6,332 tiles,
  the second pass's 7,364, 10,870 held-out scores (Friday's, renumbered;
  hard links, no copy).
- [x] Every image's lines fetched whole for its tiles (the relief box):
  5,334 images, the store 27 to 29 GB.
- [ ] The 1,065 tiles whose image windows had been cut, solved again (second
  pass) and scored again. Their earlier results: `second_cut/`,
  `scores_cut.csv`.
- [ ] The final set: every tile but those that fail the home PC's test and
  those below SHAP7 on their held-out images.
- [ ] The mesh, region by region, and previews beside SHAP7.

## What a tile adds

Three tiles' relief shaded as under a low Sun from the west, SHAP7's and the
solved one's, and the solved albedo:

![SHAP7 and the solved tiles, three tiles](relief_tiles.png)

A tile keeps SHAP7's relief at every scale it resolves and adds what the
images show below it. Band by band (`relief.py`'s tiles):

| tile | below 2.5 m: SHAP7, tile | 2.5-12.5 m | 12.5-40 m | above 40 m |
|---|---|---|---|---|
| 4291, 0.5 m | 0.18 m, 0.35 m (corr 0.47) | 1.53, 1.54 (1.00) | 3.64, 3.64 (1.00) | 0.64, 0.64 (1.00) |
| 4875, 0.5 m | 0.16, 0.25 (0.63) | 1.32, 1.32 (1.00) | 2.18, 2.18 (1.00) | 1.04, 1.04 (1.00) |
| 6956, 1 m | 0.58, 0.75 (0.71) | 2.24, 2.26 (0.97) | 5.00, 5.01 (1.00) | 1.41, 1.42 (1.00) |

The relief's RMS in each band and the two's correlation. The solved panels
look flatter at 20-40 m only because their fine relief takes the shading's
range; the slopes at 10 m are SHAP7's (12.7 deg both, 9.2 and 9.3, 17.2
and 17.1).

## The mesh's size

On 100 tiles at 0.5 m and SHAP7 around them, `compose` took 2.6 s for
12.5 million facets. The whole comet at the tiles' own spacing is about
290 million facets -- two to each 0.25 m2 cell over 33 km2 at 0.5 m, fewer at
1 and 2 m -- 5.5 GB as binary PLY (float positions, int indices); as OBJ
text, 20 GB. `compose` over all of it at once would hold some 20 GB of
tiles: it runs region by region, each region's facets kept, the regions
meeting cell for cell since each sees every tile over its border.
