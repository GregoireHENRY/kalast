#!/usr/bin/env python
"""Decimate a shape model without introducing flipped faces.

The decimated Didymos-system meshes were originally cut with MeshLab's
defaults. `preservenormal` defaults to **False** there, and MeshLab's own
tooltip for it reads "try to avoid face flipping effects" -- so the default
permits exactly the defect that turned up later: facets whose normal ends up
pointing into the body, which a hemicube sees as a self view factor near 1 and
which the thermophysical model leaves permanently dark, since it clamps
`cos(incidence)` at zero.

Measured on Dimorphos, cutting 3,145,728 faces down to 10,000, with the self
view factor from `kalast.tpm.heating.pathological_facets` as ground truth and
surface area against the 3.1M original as the fidelity check:

| recipe | max self VF | facets > 0.5 | area error |
|---|---|---|---|
| as shipped (MeshLab defaults) | 0.9957 | 1 | +0.02 % |
| + planarquadric | 0.5096 | 2 | +2.07 % |
| + preservenormal, preservetopology | 0.6796 | 1 | +0.07 % |
| **RECIPE below** | **0.3189** | **0** | **+0.06 %** |

`planarquadric` also removes the flips but costs 2 % of the surface area, so
it is deliberately not used. `qualitythr` at 0.6 (against a 0.3 default) gets
there by keeping triangles well shaped instead.

**Why 0.6 and not higher.** Same recipe and target, only `qualitythr` varied:

| qualitythr | faces | area err | min triangle quality | quality < 0.3 |
|---|---|---|---|---|
| 0.0 | 2,690,976 | -0.013 % | 0.0512 | 7409 |
| 0.3 (default) | 10,000 | +0.067 % | 0.0609 | 110 |
| **0.6** | **10,000** | **+0.060 %** | **0.1216** | **100** |
| 0.8 | 10,000 | +0.056 % | 0.0838 | 103 |
| 1.0 | 10,000 | +0.056 % | 0.0838 | 103 |

Quality is `2 r_in / r_circ`: 1 equilateral, 0 a sliver. **0.8 and 1.0 produce
a byte-identical mesh** -- the penalty saturates between 0.6 and 0.8, so 1.0 is
not a stronger setting, only a rounder number for the same one. And 0.6 has the
best *worst* triangle of the five, so raising it makes the sliver that matters
slightly worse. `qualitythr = 0` does not decimate at all, stopping at 2.69M
faces, which is why its area error is not comparable.

The fidelity one might expect to lose by favouring shape over the quadric error
does not appear: area error *improves* slightly across the range and every
value is under 0.07 %. Note this sweep measures triangle shape and area, not
the self view factor that motivated the recipe -- 0.6 already reaches zero
pathological facets, so there is no headroom above it to recover.

For reference the genuine concavities on these bodies top out around 0.35, so
a maximum of 0.319 means nothing pathological is left.

Worth knowing: **only the decimated Dimorphos models were ever affected.**
Every Didymos model and the full-resolution 3.1M Dimorphos are clean.

Usage:
    python examples/mesh/decimate.py IN.obj OUT.obj 10000
"""

import os
import sys

# Before `import pymeshlab`, and it has to stay there. MeshLab parses OBJ
# floats through the C locale, so under a comma-decimal locale (fr_BE here)
# every coordinate truncates at the dot: the 3.1M Dimorphos loads as a bbox of
# +/-9 with volume exactly 0, and decimation then writes that out with no
# error at all. pymeshlab initialises Qt, which calls setlocale(LC_ALL, "")
# and so undoes a `locale.setlocale` made beforehand -- setting it after the
# MeshSet exists works, but a new MeshSet undoes it again, so the environment
# is the only place the setting survives.
os.environ["LC_ALL"] = "C"

import pymeshlab


RECIPE = dict(
    preservenormal=True,     # the one that matters: no face flipping
    preservetopology=True,
    qualitythr=0.6,          # default 0.3; keeps triangles well shaped
    optimalplacement=True,
)


def decimate(src, dst, faces, **overrides):
    ms = pymeshlab.MeshSet()
    ms.load_new_mesh(str(src))
    before = ms.current_mesh().face_number()
    ms.apply_filter(
        "meshing_decimation_quadric_edge_collapse",
        targetfacenum=int(faces),
        **{**RECIPE, **overrides},
    )
    ms.save_current_mesh(str(dst), save_vertex_normal=False, save_face_color=False)
    return before, ms.current_mesh().face_number()


if __name__ == "__main__":
    if len(sys.argv) < 4:
        raise SystemExit(__doc__.strip().splitlines()[-1])
    src, dst, faces = sys.argv[1], sys.argv[2], sys.argv[3]
    before, after = decimate(src, dst, faces)
    print(f"{before:,} -> {after:,} faces\n  {dst}")
    print("\nVerify with the hemicube, not with Mesh.inward_facing_facets --")
    print("that heuristic over-reports about 20x on these meshes:")
    print("  heating.pathological_facets(view_factors, body)  # self VF > 0.5")
