# Didymos and Dimorphos from the ground

The light curve of the binary asteroid (65803) Didymos one night as
TRAPPIST-North (Oukaimeden, Morocco) saw it, 2022-11-18, 00:30 to 06:30 UTC
at a phase angle of 60 deg: Didymos turning every 2.26 h, and the mutual
events of its moon Dimorphos -- about 00:40 to 01:45, Dimorphos's shadow on
Didymos; 04:35 to 06:00, Dimorphos hidden behind Didymos; from 06:20,
Didymos's shadow on Dimorphos. kalast places the two
bodies from DART's SPICE kernels, at the time their light left for the
site, and `kalast.lightcurve.flux` sums every facet's light under Buratti et
al. (2024)'s Hapke law, what each body shadows or hides of the other exact.

- [main.py](main.py): the night, every 2 minutes, in a table -- the
  system's magnitude about the night's mean, and the drop the events make
  (the system against its two bodies drawn apart); `--plot` draws them.
  Two shape models, both by default, or the one named:
    - `meshes`: DART's shape models of the two;
    - `ellipsoids`: Scheirich et al. (2024)'s, the bodies the system's events
      are predicted with -- oblate spheroids of DART's volume and axis
      ratios, [../../res/ico5.obj](../../res/ico5.obj) stretched -- under
      the same law, so that the shapes alone differ. Scheirich et al.
      predict under Lommel-Seeliger's: with it, the shadow takes about
      0.10 mag at its deepest rather than 0.109.
- [didymos.tm](didymos.tm): the SPICE kernels it loads.

![The night with DART's shapes and with spheroids](../../notes/2026-10-21_lightcurve_example/didymos_2022-11-18.png)

DART's shapes add Didymos's rotation, a tenth of a magnitude either way at
this phase; a spheroid turning about its axis has none, and what is left
is the events and the phase angle's slow change. The events differ too,
in `main.py`'s 2-minute steps: Dimorphos's shadow takes 0.095 mag from
DART's Didymos, 0.109 from the spheroid; hidden whole, DART's elongated
Dimorphos takes 0.040 to 0.046 as it turns, the sphere 0.049 throughout.

## Input data

The paths in the scripts are where these are kept here: set them to yours.

1. **SPICE kernels**: the seven [didymos.tm](didymos.tm) lists, 171 MB --
   DE430 120 MB of it, DART's system solution 51 MB -- from NAIF's
   [DART SPICE archive](https://naif.jpl.nasa.gov/pub/naif/pds/pds4/dart/dart_spice/spice_kernels/),
   in their folders there. Set its `PATH_VALUES`.
2. **DART's shape models**, for `meshes`: Didymos v003 and Dimorphos v004,
   49,152 facets each, 3.6 MB each, from the PDS Small Bodies Node's
   [DART shape models](https://pdssbn.astro.umd.edu/holdings/pds4-dart_shapemodel-v1.0/):
   [didymos_g_9309mm_spc_obj_0000n00000_v003.obj](https://pdssbn.astro.umd.edu/holdings/pds4-dart_shapemodel-v1.0/data_derived_didymos_model_v003/didymos_g_9309mm_spc_obj_0000n00000_v003.obj)
   and
   [dimorphos_g_1940mm_spc_obj_0000n00000_v004.obj](https://pdssbn.astro.umd.edu/holdings/pds4-dart_shapemodel-v1.0/data_derived_dimorphos_model_v004/dimorphos_g_1940mm_spc_obj_0000n00000_v004.obj),
   `MESHES` in `main.py`.

## Running

```sh
python main.py              # both shape models, 30 s on an M1 Pro
python main.py ellipsoids   # the spheroids alone, no shape model to download
python main.py --plot
```

Another night, another site: `START`, `END` and `SITE` in `main.py`. The
kernels hold the system from 2001 to the end of 2024, the impact on
2022-09-26 in them. DART's kernel turns Didymos at 2.2600 h; `main.py`
turns it at its true period, 2.2603891 h before the impact and 2.260440 h
after (Durech et al. 2025), its phase the kernel's at the impact. Even so,
the 2022 campaign's photometry puts DART's Didymos 8 to 50 deg off in its
turning, by date: read the rotation's phase loosely, the events' times
closely.
