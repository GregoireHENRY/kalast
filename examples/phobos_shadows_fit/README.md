# Phobos from Mars Express's HRSC SRC

Four images of Phobos from the Super Resolution Channel of Mars Express's
HRSC, drawn by kalast and set beside the real ones.

- [simple.py](simple.py): one of them, picked in its `IMAGES`, in kalast's
  window. The shortest way in.
- [main.py](main.py): the four, saved to `out/phobos_hrsc/`; its docstring
  lists the options (shadow method, scattering law, shape model, ...).
- [compare.py](compare.py): the images beside kalast's -- shadows, pointing,
  brightness -- figures and a table in `out/phobos_hrsc/figures/`.

## Input data

The images are in this folder: `h*_sr2.img`, HRSC level-2 products of the
`MEX-M-HRSC-3-RDR-V2.0` data set, from ESA's
[Planetary Science Archive](https://archives.esac.esa.int/psa/), with their
thumbnails. The rest is downloaded once; the paths in the scripts are where
they are kept here, so set them to yours.

1. **SPICE kernels**: the 13 that [phobos_hrsc.tm](phobos_hrsc.tm) lists. Set
   its `PATH_VALUES`.
    - 12 from ESA's
      [Mars Express set](https://spiftp.esac.esa.int/data/SPICE/MARS-EXPRESS/kernels/),
      in their folders there (`lsk/`, `sclk/`, `fk/`, `ik/`, `pck/`, `spk/`,
      `ck/`), under the first path. The three measured attitudes,
      `ck/ATNM_MEASURED_*`, are 230-250 MB each.
    - `mar099s.bsp`, JPL's ephemeris of Mars's moons, from NAIF's
      [generic kernels](https://naif.jpl.nasa.gov/pub/naif/generic_kernels/spk/satellites/),
      under the second path, in `mar099/`.
    - `pck00011.tpc`, from NAIF's
      [generic kernels](https://naif.jpl.nasa.gov/pub/naif/generic_kernels/pck/):
      Phobos's rotation for Ernst et al.'s model. Its path is in `main.py` and
      `simple.py`.
2. **Phobos's shape**, the paths in `main.py`'s `MESHES` and in `simple.py`:
    - Ernst et al. (2023), the default: 12.6M facets of 18 m,
      [phobos_g_018m_spc_obj_0000n00000_v004.obj.zip](https://sbmt.jhuapl.edu/shared-files/files/phobos_g_018m_spc_obj_0000n00000_v004.obj.zip)
      (190 MB, 925 MB unzipped), from the Small Body Mapping Tool's
      [shared files](https://sbmt.jhuapl.edu/shared-files/).
    - `gaskell` and `dlr`: `PHOBOS_M157_GAS_V01.OBJ` and
      `PHOBOS_K275_DLR_V02.OBJ`, from the Mars Express set's `dsk/` folder.
3. **Mars**, in the reference's scene for its light on Phobos: `res/ico5.obj`,
   shipped with kalast.

## Running

```sh
python simple.py                    # one image in the window
python main.py rays hapke ernst     # the four, saved
python main.py reference            # the same, traced, with Mars's light
python compare.py                   # figures and table
```

On an M1 Pro, an image takes about 7 s with `rays`, and about 3.5 minutes with
`reference`, Mars and a bounce at 1024 samples a pixel (1.75 without the
bounce); the camera's response in `compare.py`, under a second.
