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
