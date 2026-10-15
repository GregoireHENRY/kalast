# Handoff to the home PC: comet 67P tiles 2000 to 6488

From the Mac session of 15 October 2026. Comet 67P is being solved whole,
in the 6,489 tiles of `examples/comet67p_tiles/tiles.csv` (see `README.md`
beside this note). The Mac (M1 Pro) solves tiles **0 to 1999**; this
machine is asked for **2000 to 6488**, about 70% -- it is about twice as
fast. Same code, same `tiles.csv`, same images: the two halves meet without
a seam.

## Steps

1. **Pull `main` and build**, from the repository's root:

   ```sh
   python tools/develop.py --release
   ```

   It brings `kalast.shape.Surface`, `choose`, `compose`, the solver's
   `triangles`, and `kalast.mesh.Mesh.from_arrays`. Check:

   ```sh
   python -c "from kalast.shape import Surface, choose, compose; print('ok')"
   ```

   The example needs `spiceypy` and `scipy` besides kalast:
   `pip install spiceypy scipy` if they are missing.

2. **Data**, where it suits this machine. Set the paths in the scripts to
   it, and **do not commit those edits** -- the examples keep the Mac's,
   and each machine sets its own (no per-machine file, no variable):

   - SHAP7 in full and its 4M-facet reduction, binary PLY, links in
     `examples/comet67p_tiles/README.md` (835 MB and about 75 MB):
     `SHAP7` and `SHAP7_4M` at the top of `examples/comet67p_tiles/main.py`.
   - The Rosetta SPICE kernels of `examples/comet67p_shadows_fit/rosetta.tm`
     (550 MB, from https://spiftp.esac.esa.int/data/SPICE/ROSETTA/kernels/,
     the folders it lists): its `PATH_VALUES` to where they are. Forward
     slashes; a path longer than 80 characters is split with `+`.
   - Where the images go: `DATA` in
     `examples/comet67p_shadows_fit/osiris.py` -- about 25 GB for this
     share, under `DATA/tiles67p/` and `DATA/labels/`.

3. **Run**, from the repository's root (the outputs go to `out/67p_tiles/`):

   ```sh
   python examples/comet67p_tiles/main.py fetch 2000 6489
   python examples/comet67p_tiles/main.py solve 2000 6489
   ```

   `fetch` reads each image's label, then fetches only the lines its
   tiles need from ESA's archive: about 25 GB, half an hour to an hour.
   `solve` prints a line a tile and the hours left. Both start again where
   they stopped if run again. `WORKERS` at the top of `main.py` is the
   tiles solved at once, each taking one to two cores: **10** suits a
   9800X3D (8 cores, 16 threads); lower it if the machine is wanted for
   something else while it runs.

4. **What to expect**: a line a tile like

   ```text
   t02000 0.5 m: 70 images, correlation 0.45 -> 0.62 at the last level | 1/4489, 6.3 h left
   ```

   -- the median correlation of the finest level's images with the tile,
   SHAP7's and then the solution's. A tile that cannot be done prints
   `failed` and its reason; a few are expected (an image the archive does
   not answer for); write them down.

5. **Bring back** `out/67p_tiles/tiles/` -- `t02000.npz` to `t06488.npz`,
   about 3 GB -- to the Mac, by any copy (a cloud drive, a disk). Not into
   git. The Mac then scores every tile on the held-out images and puts the
   tiles into the image simulator.

## When it is done

Write in this folder what happened -- `run_home_pc.md`: how long, how many
tiles failed and why, anything odd -- commit that note alone, and push.
Do not commit the path edits or the outputs.
