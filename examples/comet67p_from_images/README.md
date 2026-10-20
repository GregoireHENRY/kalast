# Comet 67P from its images alone

Comet 67P/Churyumov-Gerasimenko's shape made by `kalast.shape` from 354 of
Rosetta's OSIRIS narrow-angle images and ESA's SPICE kernels, as a
spacecraft arriving at a body knows it -- no shape model to start from.
The images' silhouettes carved into a hull, then maplets over it, coarse to
fine, the mesh made again from them each round: 40 m, 20 m, then 10 m.
Preusker et al.'s SHAP7 only measures the result (`compare`).

- [main.py](main.py), into `out/67p_from_images/`:
    - `fetch`: the frames of `frames.csv`, whole;
    - `hull`: the frames' silhouettes carved (`kalast.shape.Hull`) -- the
      sky in a frame is what is neither lit nor behind lit ground from the
      Sun -- each frame that shows a limb registered on the hull three
      times, then meshed in 20 m cubes, smoothed by 30 m: `hull.ply`, one
      closed piece, and each frame's pointing, `hull.npz`;
    - `tiles`: rounds of maplets of 31 x 31 points, laid once a stage --
      ten at 40 m, four at 20 m, three at 10 m. Each frame registered on
      each maplet; every frame's pointing and every maplet's place and
      height fitted to all the registrations at once
      (`kalast.shape.pointing`), the frames registered by their limbs
      holding theirs; each maplet solved again with the pointing held; all
      merged into the mesh made again (`kalast.shape.merge`):
      `40m_1.ply` to `10m_3.ply`, each a closed mesh, with its frames'
      pointing;
    - `compare`: the last mesh against SHAP7 -- as the kernels place both,
      then SHAP7 laid on it at its best.
- [frames.csv](frames.csv): the 354 frames, their data sets, distance (km)
  and phase (deg); `hull` 1 for the 336 the hull is carved from -- 60 to
  600 km, phase 31 to 70 deg, from 178 of 180 directions about the
  nucleus -- 0 for 18 more that see what those see least.

## Input data

The paths in the scripts are where these are kept here: set them to yours.

1. **The images**: downloaded by `main.py fetch` from ESA's
   [Planetary Science Archive](https://archives.esac.esa.int/psa/ftp/INTERNATIONAL-ROSETTA-MISSION/OSINAC/),
   or NASA's [PDS Small Bodies Node](https://pdssbn.astro.umd.edu/holdings/)
   when ESA's is down: 38 MB a frame, 13 GB, into `osiris.DATA` (set in
   [../comet67p_shadows_fit/osiris.py](../comet67p_shadows_fit/osiris.py)).
2. **SPICE kernels**: those of
   [../comet67p_shadows_fit/rosetta.tm](../comet67p_shadows_fit/rosetta.tm),
   from ESA's [Rosetta set](https://spiftp.esac.esa.int/data/SPICE/ROSETTA/kernels/),
   550 MB; its `PATH_VALUES` set to where they are.
3. **SHAP7**, for `compare` only, as binary PLY:
   [cg-dlr_spg-shap7-v1.0.ply](http://comsim.esac.esa.int/rossim/SHAPE_MODEL_DRAFTS/SHAP7_8/SPG/cg-dlr_spg-shap7-v1.0_fullres-and-reductions_PLY/cg-dlr_spg-shap7-v1.0_fullres_PLY/cg-dlr_spg-shap7-v1.0.ply)
   (835 MB), `SHAP7` in `main.py`.

## Running

```sh
python main.py fetch      # 13 GB, ten minutes at 20 MB/s
python main.py hull       # 15 minutes on an M1 Pro
python main.py tiles      # 40 m: 10 rounds of 12 minutes; 20 m: 4 of 17; 10 m: 3 of 40
python main.py compare
```

Each step keeps what it made; `tiles` starts again at the first round not
made. See `notes/2026-10-20_67p_from_images/` for how each part was found
and how near SHAP7 each round comes.
