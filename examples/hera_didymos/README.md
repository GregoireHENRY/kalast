# Hera at Didymos

Didymos and Dimorphos as Hera's cameras see them: the Asteroid Framing Camera
(AFC) in visible light and the Thermal InfraRed Imager (TIRI). The three
scripts below follow AFC's and TIRI's images, at the times they were taken.

- [afc.py](afc.py): what AFC saw. Each AFC-1 image is drawn in the rows and
  columns of the products, each pixel the I/F of the ground it sees over the
  pixel (kalast's reference image, under Buratti et al. (2024)'s Hapke law).
  The output goes to `out/hera_didymos/afc/`:
    - each drawn image as FITS, beside a PNG of the frame;
    - `afc.csv` and `afc.png`: each image against its drawing -- the
      pointing's error, Didymos's size and signal, Dimorphos beside it -- and
      crops of both.

  With no images found, AFC every 15 minutes from `START` to `END`, as PNGs.
- [tiri.py](tiri.py): what TIRI saw. kalast's thermophysical model runs 3
  days before the first image (`SPINUP`), under the Sun as SPICE has it and
  the shadows of the frames. Each image is drawn in its own filter, each
  pixel the radiance (W m⁻² sr⁻¹) of the ground it sees. The output goes to
  `out/hera_didymos/tiri/`:
    - each drawn image as FITS, spread by TIRI's point spread (`PSF`), with
      the rows and columns of TIRI's products;
    - `tiri.csv` and `tiri.png`: each body in each image against its
      drawing -- the pointing's error, Dimorphos beside Didymos (its orbit,
      also as time along it), their signals -- and crops of both.
- [coverage.py](coverage.py): the geometry at the times of the images where
  `afc.py` and `tiri.py` found Didymos (run them first), in
  `out/hera_didymos/coverage/`:
    - `geometry.png`: Hera's distance to Didymos and to Dimorphos, the phase
      angle, the sub-spacecraft points, and AFC's and TIRI's resolution;
    - `coverage.png`: maps of the ground each instrument saw, with the
      sub-spacecraft points on them. Ground counts as seen where it was in
      the field as the image shows it pointed and facing Hera, and lit for
      AFC.

Pointing comes from Hera's measured attitude: `hera_ops.tm` is loaded over
`hera_plan.tm`, which holds Didymos and Dimorphos. Where the measured attitude
ends, SPICE takes the plan's, and the CSVs say which an image had. TIRI's
boresight is the one measured in flight (`ALIGN`,
`kalast/tiri_alignment.py`).

`afc.py` and `tiri.py` draw Didymos where each image has it: SPICE's pointing
is turned onto it, and the turn, with what the fit leaves, is the pointing's
error in the CSVs.

Two corrections, constants in each script, 0 for the kernels as they are:
- `DIDYMOS_SHIFT`: Didymos turned as the dataset's PCK has it that much
  later. The PCK leaves the prime meridian undefined.
- `DIMORPHOS_SHIFT`: Dimorphos placed where the dataset's ephemeris has it
  that much later. `afc.py` and `tiri.py` measure what is left on each run
  and print the value that would match the images they ran on.

The other scripts are the thermal studies of the Dimorphos transit of
2027-01-21: [tpm.py](tpm.py) and [tpm_phase2.py](tpm_phase2.py) for the
temperatures, [tiri_fits.py](tiri_fits.py), [tiri_movie.py](tiri_movie.py)
and [tiri_movie_compose.py](tiri_movie_compose.py) for the images,
[heating_preflight.py](heating_preflight.py) for which body needs mutual
heating. [afc_eclip_didy.py](afc_eclip_didy.py) plays AFC from November 2026
to April 2027.

## Data

The paths are at the top of each script; set them to where your data is.

- **ESA's Hera SPICE dataset**, `HERA.zip`, 1.1 GB, newest as of 26 October
  2026:
  <https://spiftp.esac.esa.int/data/SPICE/HERA/misc/skd/HERA.zip>. It holds
  the kernels (`KERNELS`, its `kernels/mk`) and Didymos's and Dimorphos's
  shape models. [../../res/README.md](../../res/README.md) has how to unpack
  it and set the meta-kernels' `PATH_VALUES`. ESA updates it as the mission
  goes: download it again for newer attitude and trajectories.
- **The shape models at 100,000 facets** (`g_01165mm_spc_didy_v003_100k.obj`,
  `g_00243mm_spc_dimo_v004_100k.obj`), made from the dataset's
  `kernels/dsk/*.obj` with [../mesh/decimate.py](../mesh/decimate.py).
- **AFC's images** (`IMAGES` in `afc.py`): the level-1B FITS of the Hera
  image archive at BME (<https://didymos.mogi.bme.hu/images>, a Hera team
  login), in any folders below `IMAGES`. `afc.py` takes AFC-1's from `SINCE`
  (2026-10-23), when Didymos is resolved, 4.2 MB each. A product's time is
  its `DATE-OBS`. These are the Hera team's data, not public yet.
- **TIRI's images** (`IMAGES` in `tiri.py`), as edds_decoder, the TIRI
  decoder, writes them: one folder a day, `<IMAGES>/<yyyymmdd>/tiri_par_*.fits`.
  A product's time is its `DATE-OBS`, or its `SPI_TIME` when it has none, and
  its filter is `FW_NUM`. Images with the shutter closed are skipped. These
  are the TIRI team's data, not public yet.
- **TIRI's spectral response** (`RESPONSE`, 210 kB), for `tiri.py`: the
  TIRI team's `response.csv`, a filter's response a column, from 3 µm.

## Run

From the repository's root, in this order: `coverage.py` reads the CSVs of
the other two. On an M1 Pro, `afc.py` takes 1.5 min on AFC's images to 26
October, `tiri.py` 4 min on TIRI's to 27 October and `coverage.py` 10 s.

    python examples/hera_didymos/afc.py
    python examples/hera_didymos/tiri.py
    python examples/hera_didymos/coverage.py
