KPL/MK

The kernels the four HRSC SRC images of Phobos need, out of rot_shdw.TM's 350:
time, the spacecraft's clock, its frames and the camera's, Phobos's rotation,
where Mars Express, Phobos and the Sun are, and where the spacecraft pointed.

From ESA's Mars Express SPICE set, https://www.cosmos.esa.int/web/spice/mars-express
-- set PATH_VALUES to where yours is.

   \begindata

     PATH_VALUES       = ( '/Users/gregoireh/data/spice/mex/kernels' )
     PATH_SYMBOLS      = ( 'KERNELS' )

     KERNELS_TO_LOAD   = (
                           '$KERNELS/lsk/NAIF0012.TLS'
                           '$KERNELS/sclk/MEX_250326_STEP.TSC'
                           '$KERNELS/fk/MEX_V16.TF'
                           '$KERNELS/ik/MEX_HRSC_V09.TI'
                           '$KERNELS/pck/PCK00010.TPC'
                           '$KERNELS/spk/DE405.BSP'
                           '$KERNELS/spk/MAR097_030101_300101_V0001.BSP'
                           '$KERNELS/spk/ORMM_T19_101201000000_00755.BSP'
                           '$KERNELS/spk/ORMM_T19_110101000000_00758.BSP'
                           '$KERNELS/spk/ORMM_T19_130601000000_00985.BSP'
                           '$KERNELS/ck/ATNM_T6_P060401000000_01346.BC'
                         )

   \begintext
