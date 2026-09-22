# Hera meshes and kernels

The Hera, Didymos and Dimorphos examples need ESA's Hera SPICE kernel
dataset, which is not in this repository. It also contains the shape models.

## Download

    https://spiftp.esac.esa.int/data/SPICE/HERA/misc/skd/HERA.zip

About 1.1 GB. Unpack it where you keep your data; the directory then
contains `kernels/` and `misc/`. For example:

    mkdir -p ~/data/spice && cd ~/data/spice
    curl -O https://spiftp.esac.esa.int/data/SPICE/HERA/misc/skd/HERA.zip
    unzip HERA.zip -d hera

What the examples use from it:

- `kernels/mk/*.tm` — the meta-kernels: `hera_ops.tm` for the operational telemetry and measured attitude, `hera_plan.tm` for the predicted trajectory and default attitude. 
- `kernels/dsk/*.obj` — the shape models as OBJ, beside their DSKs:
  `g_01165mm_spc_obj_didy_0000n00000_v003.obj` (Didymos, 3.1 M facets),
  `g_00243mm_spc_obj_dimo_0000n00000_v004.obj` (Dimorphos),
  `deimos_k005_tho_v02.obj`.

## The meta-kernels: set `PATH_VALUES`

Every `.tm` begins with

    \begindata
      PATH_VALUES       = ( '..' )
      PATH_SYMBOLS      = ( 'KERNELS' )

SPICE resolves that `'..'` against the **working directory of your process**,
not against the file, so loaded from anywhere but `kernels/mk/` itself the
first kernel in the list is not found and `furnsh` fails with an error that
does not name the cause.

Copy the meta-kernel you use to a `*_local.tm` twin and put the absolute path
of `kernels/` in that one line. In a text editor (forward slashes are fine on
Windows), or in a POSIX shell:

    cd ~/data/spice/hera/kernels/mk
    sed "s|PATH_VALUES *= *( *'\\.\\.' *)|PATH_VALUES = ( '$HOME/data/spice/hera/kernels' )|" hera_plan.tm > hera_plan_local.tm
    sed "s|PATH_VALUES *= *( *'\\.\\.' *)|PATH_VALUES = ( '$HOME/data/spice/hera/kernels' )|" hera_ops.tm  > hera_ops_local.tm

Leave the pristine `.tm` alone; the dataset is versioned and a later zip
replaces it. The examples load the `_local` twins.

## Pointing the examples at it + Decimated mesh

Each example names its kernel and mesh paths near the top of the script.
Edit them to where you unpacked the zip. Several scripts also load decimated
`_10k` / `_100k` versions of the two shape models, which are not in the zip;
`examples/mesh/decimate.py` makes them from the full-resolution OBJs.

A path that exists is not enough: the kernels have to cover the epoch a
script uses, and a gap surfaces as `SPKINSUFFDATA` in the middle of a run.
Run `examples/hera_didymos/afc.py` once end to end before anything longer.
