# Hera at Didymos

Didymos and Dimorphos as Hera's cameras see them: the Asteroid Framing Camera
(AFC) in visible light and the Thermal InfraRed Imager (TIRI). The three
scripts below follow TIRI's images, at the times the images were taken.

- [afc.py](afc.py): what AFC saw at each image's time, a frame a time
  written to `out/hera_didymos/afc/`. With no images found, every 15 minutes
  from `START` to `END`.
- [tiri.py](tiri.py): what TIRI saw. kalast's thermophysical model runs 3
  days before the first image (`SPINUP`), under the Sun as SPICE has it and
  the shadows of the frames. Each image is drawn in its own filter, each
  pixel the radiance (W m⁻² sr⁻¹) of the ground it sees. The output goes to
  `out/hera_didymos/tiri/`:
    - each drawn image as FITS, spread by TIRI's point spread (`PSF`), with
      the rows and columns of TIRI's products;
    - `tiri.csv` and `tiri.png`: each body in each image against its
      drawing -- Didymos's place (the pointing), Dimorphos's beside it (its
      orbit, also as time along it), their signals -- and crops of both.
- [coverage.py](coverage.py): the geometry at the images' times, in
  `out/hera_didymos/coverage/`:
    - `geometry.png`: Hera's distance to Didymos and to Dimorphos, the phase
      angle, the sub-spacecraft points, and AFC's and TIRI's resolution;
    - `coverage.png`: maps of the ground each instrument saw, with the
      sub-spacecraft points on them. Ground counts as seen where it was in
      the field as the instrument was pointed and facing Hera, and lit for
      AFC.

Pointing comes from Hera's measured attitude: `hera_ops.tm` is loaded over
`hera_plan.tm`, which holds Didymos and Dimorphos. TIRI's boresight is the one
measured in flight (`ALIGN`, `kalast/tiri_alignment.py`).

Dimorphos is placed where TIRI's images of 2026-10-25 have it: 120° further
along its orbit than the dataset's ephemeris, which has it there 3.80 h later
(`DIMORPHOS_SHIFT`, 13,689 s, in each script; 0 for the ephemeris as it is).
`tiri.py` measures what is left on each run and prints the value that would
match the images it ran on.

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
- **TIRI's images** (`IMAGES`), as edds_decoder, the TIRI decoder, writes
  them: one folder a day, `<IMAGES>/<yyyymmdd>/tiri_par_*.fits`. A
  product's time is its `DATE-OBS`, or its `SPI_TIME` when it has none, and
  its filter is `FW_NUM`. Images with the shutter closed are skipped. These
  are the TIRI team's data, not public yet.
- **TIRI's spectral response** (`RESPONSE`, 210 kB), for `tiri.py`: the
  TIRI team's `response.csv`, a filter's response a column, from 3 µm.

## Run

From the repository's root, in this order (`tiri.py` is the long one, a few
minutes):

    python examples/hera_didymos/afc.py
    python examples/hera_didymos/tiri.py
    python examples/hera_didymos/coverage.py
