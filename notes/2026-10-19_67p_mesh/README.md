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
  `scores_cut.csv`. 703 solved at 16:48: heights 1.05 m RMS from their
  first solve at the median (2.8 m at the 90th percentile), the same images
  used, the same fit to them.
- [x] The mesh builder (scratch `build_mesh.py`): 32 regions about k-means
  centres of SHAP7's facets, each `compose`d with every tile reaching it;
  each region a binary PLY, then the whole. Two regions welded: no facet in
  both, no edge in three.
- [x] Walls left to SHAP7: a tile's points over 5 m off SHAP7 (below).
- [ ] The final set: every tile but those that fail the home PC's test and
  those below SHAP7 on their held-out images.
- [ ] The mesh, and previews beside SHAP7.

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

## A first drawing, and smooth shading

A held-out OSIRIS image (N20141001T025452, 18.2 km, 0.34 m a pixel, phase
95 deg) drawn by `comet67p_shadows_fit`'s `tiles` mesh -- SHAP7-4M with the
180 tiles it sees -- beside the image (`preview.py`):

![The image, the tiles drawn flat, the tiles drawn smooth](first_render_flat_vs_smooth.png)

Drawn flat (the middle), each 0.5 m cell's two triangles take their own
normals, 1.5 pixels across here: stripes along the diagonals. The OBJ that
`shap7` loads is smoothed; the tiles' mesh, made by `Mesh.from_arrays`,
was not -- `smooth=True` (the right) draws them alike. The large forms are
the image's; its long shadows are not all there, which is SHAP7's relief
(the tiles keep it above 2.5 m) or the drawing -- SHAP7 in full, drawn the
same way, will tell.

## Walls: a tile's points far off SHAP7 left to it

Region 24 drawn from above under a low Sun (`region_view.py`, kalast's
rays) has combs on a cliff, tall thin triangles side by side; turning the
Sun 25 deg does not turn them, so they are the mesh's. The tiles there, of
121 points at 1 and 2 m where the surface turns, run 30 to 90 m off SHAP7
along the walls: a height field cannot follow a wall near square to its
plane, and SHAP7's facets farther than `compose`'s depth from a tile (8 to
26 m) are not its, so they stay too -- two surfaces.

Over the second pass's 6,970 tiles, each tile's farthest point off SHAP7:
3.7 m at the median, 17.5 m at the 90th percentile, 50 m at the 99th. The
held-out gain falls with it, +0.23 under 10 m, +0.19 at 10-20 m, +0.16 at
20-40 m, +0.11 over 40 m. The tiles keep SHAP7's relief above 2.5 m
(above), so a point over 5 m off it is not the finer relief they add: those
points, and two more all round, are SHAP7's -- their reference set unknown,
which `compose` reads as no tile there. 1.3 % of the tiles' ground, in
2,736 tiles; the combs go:

![A cliff in region 24: SHAP7, the tiles as solved, the tiles with their points over 5 m off SHAP7 left to it](cliff_masked.png)

Drawn with shadow maps instead, much of the region under that 12 deg Sun
was lit where the rays shadow it. The previews below are drawn with the
reference; the first drawing above, with maps, may have lost its long
shadows the same way.
