KPL/MK

   https://data.darts.isas.jaxa.jp/pub/hayabusa2/spice_bundle/spice_kernels

   This meta-kernel lists the Hayabusa2 SPICE kernels providing coverage
   from the launch to around the Earth Swing-by on December 3, 2015. All
   of the kernels listed below are archived in the Hayabusa2 SPICE kernel
   archive. This set of files and the order in which they are listed were
   picked to provide the best available data.

   It is recommended that users make a local copy of this file and modify
   the value of the PATH_VALUES keyword to point to the actual location
   of the Hayabusa2 SPICE kernel archives ``spice_kernels'' directory on
   their system. Replacing ``/'' with ``\'' and converting line
   terminators to the format native to the user's system may also be
   required if this meta-kernel is to be used on a non-UNIX workstation.

   Release History
   ---------------

     - hyb2_v01.tm Jan. 16, 2017 Yukio Yamamoto, ISAS/JAXA.
     - hyb2_v02.tm Mar. 27, 2018 Yukio Yamamoto, ISAS/JAXA.
     - hyb2_v03.tm Feb. 20, 2020 Yukio Yamamoto, ISAS/JAXA.


   \begindata
      PATH_VALUES     = ( 'C:\data\SPICE\hayabusa2'      )
      PATH_SYMBOLS    = ( 'KERNELS' )
      KERNELS_TO_LOAD = (
                          '$KERNELS/lsk/naif0012.tls'

                          '$KERNELS/pck/pck00010.tpc'
                          '$KERNELS/pck/hyb2_ryugu_shape_v20190328.tpc'

                          '$KERNELS/fk/hyb2_v14.tf'
                          '$KERNELS/fk/hyb2_hp_v01.tf'
                          '$KERNELS/fk/hyb2_ryugu_v01.tf'

                          '$KERNELS/ik/hyb2_lidar_v02.ti'
                          '$KERNELS/ik/hyb2_nirs3_v02.ti'
                          '$KERNELS/ik/hyb2_onc_v05.ti'
                          '$KERNELS/ik/hyb2_tir_v03.ti'

                          '$KERNELS/sclk/hyb2_20141203-20191231_v01.tsc'

                          '$KERNELS/spk/de430.bsp'
                          '$KERNELS/spk/2162173_Ryugu.bsp'
                          '$KERNELS/spk/2162173_ryugu_20180601-20191230_0060_20181221.bsp'

                          '$KERNELS/spk/hyb2_20141203-20161119_0001h_final_ver1.oem.bsp'
                          '$KERNELS/spk/hyb2_20141203-20141214_0001m_final_ver1.oem.bsp'
                          '$KERNELS/spk/hyb2_20151123-20151213_0001m_final_ver1.oem.bsp'
                          '$KERNELS/spk/hyb2_approach_od_v20180811114238.bsp'
                          '$KERNELS/spk/hyb2_hpk_20180627_20190213_v01.bsp'
                          '$KERNELS/spk/lidar_derived_trj_20191114_20180630053224_20190213030000_v02.bsp'

                          '$KERNELS/ck/hyb2_hkattrpt_2014_v02.bc'
                          '$KERNELS/ck/hyb2_hkattrpt_2015_v02.bc'
                          '$KERNELS/ck/hyb2_hkattrpt_2016_v02.bc'
                          '$KERNELS/ck/hyb2_hkattrpt_2017_v02.bc'
                          '$KERNELS/ck/hyb2_hkattrpt_2018_v02.bc'
                          '$KERNELS/ck/hyb2_hkattrpt_20190101000000_20190213030000_v02.bc'
                          '$KERNELS/ck/hyb2_aocsc_2014_v02.bc'
                          '$KERNELS/ck/hyb2_aocsc_2015_v02.bc'
                          '$KERNELS/ck/hyb2_aocsc_2016_v02.bc'
                          '$KERNELS/ck/hyb2_aocsc_2017_v02.bc'
                          '$KERNELS/ck/hyb2_aocsc_2018_v02.bc'
                          '$KERNELS/ck/hyb2_aocsc_20190101000000_20190213030000_v02.bc'
                        )
   \begintext

End of MK file.
