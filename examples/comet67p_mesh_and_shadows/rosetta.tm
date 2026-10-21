KPL/MK

The kernels the OSIRIS NAC images of images.csv need: time, the spacecraft's
clock, its frames and the camera's, where Rosetta, the comet and the Sun are
(DE405 for the Sun), how the comet turned, and where the spacecraft pointed --
as commanded (RATT), as measured on board (ROS_SC_MES), and as reconstructed
from OSIRIS images (ROS_SC_REC, August 2014 to June 2016). Loaded last, the
reconstructed is the one used, the measured where it has no data, the
commanded where neither has.

From ESA's Rosetta SPICE set, https://www.cosmos.esa.int/web/spice/rosetta,
in the folders of its server, https://spiftp.esac.esa.int/data/SPICE/ROSETTA/kernels/,
the superseded RATT and CATT among them, in ck/former_versions/ as its final
meta-kernel, ROS_OPS_V350_20220906_001.TM, loads them: each covers the
mission from its date on, and the latest wins. Set PATH_VALUES to where
yours are.

   \begindata

     PATH_VALUES       = ( '/Users/gregoireh/data/spice/rosetta/kernels' )
     PATH_SYMBOLS      = ( 'KERNELS' )

     KERNELS_TO_LOAD   = (
                           '$KERNELS/lsk/NAIF0011.TLS'
                           '$KERNELS/sclk/ROS_160929_STEP.TSC'
                           '$KERNELS/fk/ROS_V38.TF'
                           '$KERNELS/spk/DE405.BSP'
                           '$KERNELS/spk/CORB_DV_257_03___T19_00345.BSP'
                           '$KERNELS/spk/RORB_DV_257_03___T19_00345.BSP'
                           '$KERNELS/ck/former_versions/CATT_DV_145_02_______00216.BC'
                           '$KERNELS/ck/former_versions/CATT_DV_223_02_______00302.BC'
                           '$KERNELS/ck/CATT_DV_257_03_______00344.BC'
                           '$KERNELS/ck/ROS_OSIRIS_V2.BC'
                           '$KERNELS/ck/former_versions/RATT_DV_145_01_01_T6_00216.BC'
                           '$KERNELS/ck/former_versions/RATT_DV_223_01_01_T6_00302.BC'
                           '$KERNELS/ck/RATT_DV_257_02_01_T6_00344.BC'
                           '$KERNELS/ck/ROS_SC_MES_140120_150101_V03.BC'
                           '$KERNELS/ck/ROS_SC_MES_160101_160930_V03.BC'
                           '$KERNELS/ck/ROS_SC_REC_140801_141231_V01.BC'
                           '$KERNELS/ck/ROS_SC_REC_150101_151231_V01.BC'
                           '$KERNELS/ck/ROS_SC_REC_160101_160601_V01.BC'
                         )

   \begintext
