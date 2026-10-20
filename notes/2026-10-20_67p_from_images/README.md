# Comet 67P from its images alone

20 October 2026. After the review (`2026-10-20_67p_mesh_review/`), the user,
to "Build 67P from the OSIRIS images with kalast, using SHAP7 only to
measure against, not as a starting point. Start coarse and whole: about 10
m over the whole comet, as one closed mesh. Measure it against SHAP7 at
that resolution before going finer": "yes".

A test of what kalast can do with Hera's AFC images of Didymos and
Dimorphos: a shape model from a spacecraft's images and its kernels, no
model to start from.

## Progress

- [x] The plan (below).
- [ ] Silhouette images: 340 full NAC frames picked, 60-600 km away, phase
  31-70 deg, from 178 of 180 directions around the nucleus (scratch
  `select_hull.py`); 336 of them whole frames. Downloading from NASA's
  Small Bodies Node: ESA's archive went back into maintenance at 16:40,
  every product answered with its maintenance page.
- [x] The visual hull in kalast (Rust, `src/shape/hull.rs`):
  `kalast.shape.Hull` carves the views' silhouettes into a closed mesh by
  marching tetrahedra; `Surface.draw` and `align` register a view on it.
  An ellipsoid from 60 synthetic views: volume within 0.1 %, vertices an
  eighth of a pixel off at the median, genus 0, every edge in two facets.
  A view that does not hold the whole body carves within an earlier hull
  (`within`), where the bounding sphere would leave its night side and the
  sky past its frame alike.
- [ ] 67P's hull, each image's pointing refined on it; against SHAP7.
- [ ] Tiles over it, coarse to fine: each image registered on each tile;
  each image's pointing and each tile's place -- its height too, the stereo
  -- fitted at once; the relief solved from the shading.
- [ ] The tiles merged into the mesh (Rust): its vertices moved, one closed
  mesh still.
- [ ] Again at 20 m and at 10 m; each version against SHAP7.
- [ ] Against SHAP7 at 10 m: heights, volume, the 57 images of
  `2026-10-13_67p_osiris_shadows/` drawn with both.
- [ ] Made usable: an example, `docs/API.md`, the changelog, the bundle.

## The plan

What goes in: OSIRIS NAC images and ESA's SPICE kernels -- the spacecraft's
path, its attitude, the comet's -- as they are, as Hera's will be. ESA made
those kernels with the shape models of their day, as every mission's
navigation does; no shape model enters here. SHAP7 only measures the
result.

Stereophotoclinometry as Gaskell et al. (2008) run it from scratch: a
shape from the limbs first, then maplets over all of it, each image's
pointing and each maplet's place fitted together from where the images
put it, the relief from the shading, the maplets merged back into the
shape; coarse to fine.

1. **The hull.** In images that show the whole nucleus, the sky is what is
   neither lit nor behind lit ground from the Sun (the night side); a
   point of space that falls on the sky in any image is not the comet.
   Over a grid of 10 m cubes, each one's distance to the nearest sky in
   every image, the least of them (the second least, against a wrong
   one) its distance to the hull; the surface where it crosses zero, by
   marching cubes: closed. Concavities -- the neck, pits -- stay filled
   where no limb crosses them: what the maplets are for.
2. **Pointing.** SPICE's pointing is a few pixels off at these distances
   (`2026-10-13_67p_osiris_shadows/`): each image registered on the hull
   drawn in its geometry, and carved again.
3. **Maplets** over the hull, by kalast.shape as it is: laid by
   `Surface.tiles`, their images by `choose`, solved by `Maplet.solve`.
   Its pointing fit takes a maplet's offset across its plane only, which
   was enough with SHAP7 under it; from a hull, the height is what is
   wrong, and the images' parallax tells it -- the fit takes it too.
4. **Merging** (new): each vertex of the mesh moved along its normal to
   the maplets over it, weighted toward their middles; the triangles
   relaxed. The mesh stays closed, the hull's topology.
5. Steps 3-4 at 40, 20, then 10 m; each version against SHAP7.

## Not used

SHAP7, SHAP8, ESA's DSKs, the tiles of `2026-10-15_67p_tiles/` to
`2026-10-19_67p_mesh/`: nothing of them, except SHAP7 to measure.
