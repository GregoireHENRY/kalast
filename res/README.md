# Data the examples need

`res/` itself ships with kalast and is enough for `examples/cube`,
`examples/two_spheres` and `examples/crater_self_shadow`. Everything about
Hera, Didymos, Dimorphos, Mars and Deimos needs data that is not in this
repository, in three sets. Each is found through one environment variable;
the defaults are one person's layout, so set them.

| variable | default | what lives there |
|---|---|---|
| `KALAST_HERA` | `~/data/spice/hera` | HERA.zip, unpacked: SPICE kernels, meta-kernels, and the shape models |
| `KALAST_MESH` | `~/data/mesh` | meshes that are not in HERA.zip: decimated Didymos and Dimorphos, `sphere4.obj`, Mars, Phobos |
| `KALAST_TIRI` | `~/data/hera/tiri` | the TIRI response function and image lists (not public here) |

## HERA.zip — the kernels and the full-resolution shape models

The Hera SPICE kernel dataset, maintained by ESA:

    https://spiftp.esac.esa.int/data/SPICE/HERA/misc/skd/HERA.zip

About 1.1 GB. Unpack it anywhere; the directory should then contain
`kernels/` and `misc/`:

    mkdir -p ~/data/spice && cd ~/data/spice
    curl -O https://spiftp.esac.esa.int/data/SPICE/HERA/misc/skd/HERA.zip
    unzip HERA.zip -d hera

Inside it, what the examples use:

- `kernels/mk/*.tm` — the meta-kernels. `hera_plan.tm` for the Didymos
  proximity phase, `hera_ops.tm` for the Mars swing-by.
- `kernels/dsk/*.obj` — the shape models as OBJ, beside their DSKs:
  `g_01165mm_spc_obj_didy_0000n00000_v003.obj` (Didymos, 3.1 M facets),
  `g_00243mm_spc_obj_dimo_0000n00000_v004.obj` (Dimorphos),
  `deimos_k005_tho_v02.obj`.
- `misc/cosmo/scenarios/` — the Cosmographia scenarios.

### The meta-kernels have to be told where they are

Every `.tm` starts with

    \begindata
      PATH_VALUES       = ( '..' )
      PATH_SYMBOLS      = ( 'KERNELS' )

and SPICE resolves that `'..'` against the **working directory of your
process**, not against the file. Loaded from anywhere but `kernels/mk/`
itself, the first kernel in the list is not found and `furnsh` fails with an
error that does not name the cause. The examples therefore load a
`*_local.tm` twin of each meta-kernel, with the absolute path of `kernels/`
in place of `'..'`. Make one for each meta-kernel you use -- `hera_plan.tm`
for the Didymos scripts, `hera_ops.tm` for the Mars swing-by -- either by
copying the file and editing that one line in a text editor (forward slashes
are fine on Windows too), or in a POSIX shell:

    cd ~/data/spice/hera/kernels/mk
    sed "s|PATH_VALUES *= *( *'\.\.' *)|PATH_VALUES = ( '$HOME/data/spice/hera/kernels' )|" hera_plan.tm > hera_plan_local.tm
    sed "s|PATH_VALUES *= *( *'\.\.' *)|PATH_VALUES = ( '$HOME/data/spice/hera/kernels' )|" hera_ops.tm  > hera_ops_local.tm

Leave the pristine `.tm` alone; the dataset is versioned and a later zip
replaces it.

Then:

    export KALAST_HERA=~/data/spice/hera      # or wherever you unpacked it

`examples/hera_didymos/afc.py` and `afc_eclip_didy.py` need nothing else.

## Meshes that are not in HERA.zip — `KALAST_MESH`

Most scripts do not want the 3.1 M-facet Didymos: `examples/didymos/main.py`
loads `_100k` versions, the TPM scripts `_10k`. Those are decimated locally
with `examples/mesh/decimate.py` (MeshLab through pymeshlab; see the note in
`pyproject.toml` about face flipping), and kept as

    $KALAST_MESH/didymos/g_01165mm_spc_didy_v003_100k.obj
    $KALAST_MESH/didymos/g_01165mm_spc_didy_v003_10k.obj
    $KALAST_MESH/dimorphos/g_00243mm_spc_dimo_v004_100k.obj
    $KALAST_MESH/dimorphos/g_00243mm_spc_dimo_v004_10k.obj

The Mars swing-by scripts also want `sphere4.obj` (a unit sphere, 20,480
facets), `mars/mars_dtm_10x.obj`, `phobos/phobos_m003_gas_v01_10k.obj` and
`deimos/deimos_10k.obj` under the same root.

## TIRI — `KALAST_TIRI`

`response.csv` (the instrument's spectral response), the image lists and the
radiance FITS files the `tiri_*` scripts compare against. These come from
the TIRI team and are not redistributed here.

## Checking

A path that exists is not enough: the kernels have to cover the epoch a
script uses, and a gap surfaces as `SPKINSUFFDATA` in the middle of a run.
After setting the variables, run `examples/hera_didymos/afc.py` once end to
end before anything longer.
