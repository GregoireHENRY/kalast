KPL/MK

The kernels the four HRSC SRC images of Phobos need, out of rot_shdw.TM's 350:
time, the spacecraft's clock, its frames and the camera's, Phobos's rotation,
where Mars Express, Phobos and the Sun are, and where the spacecraft pointed --
as commanded (ATNM_T6_P...), and as measured on board (ATNM_MEASURED_...),
which rot_shdw.TM does not load: loaded last, it is the one used, the
commanded only where it has no data.

From ESA's Mars Express SPICE set, https://www.cosmos.esa.int/web/spice/mars-express,
but Phobos about Mars and the planets: JPL's MAR099 of 2025, the small one,
mar099s.bsp, from NAIF's generic kernels,
https://naif.jpl.nasa.gov/pub/naif/generic_kernels/spk/satellites/ -- in
place of rot_shdw.TM's MAR097 and DE405, it puts Phobos 1.2 km further back
along its orbit in June 2013. Set PATH_VALUES to where yours are.

   \begindata

     PATH_VALUES       = ( '/Users/gregoireh/data/spice/mex/kernels'
                           '/Users/gregoireh/data/spice/mars_moons' )
     PATH_SYMBOLS      = ( 'KERNELS' 'MOONS' )

     KERNELS_TO_LOAD   = (
                           '$KERNELS/lsk/NAIF0012.TLS'
                           '$KERNELS/sclk/MEX_250326_STEP.TSC'
                           '$KERNELS/fk/MEX_V16.TF'
                           '$KERNELS/ik/MEX_HRSC_V09.TI'
                           '$KERNELS/pck/PCK00010.TPC'
                           '$MOONS/mar099/mar099s.bsp'
                           '$KERNELS/spk/ORMM_T19_101201000000_00755.BSP'
                           '$KERNELS/spk/ORMM_T19_110101000000_00758.BSP'
                           '$KERNELS/spk/ORMM_T19_130601000000_00985.BSP'
                           '$KERNELS/ck/ATNM_T6_P060401000000_01346.BC'
                           '$KERNELS/ck/ATNM_MEASURED_100101_110101_V03.BC'
                           '$KERNELS/ck/ATNM_MEASURED_2011_V04.BC'
                           '$KERNELS/ck/ATNM_MEASURED_2013_V04.BC'
                         )

   \begintext
