KPL/MK

The kernels lightcurve.py and scene.py need: time, Earth's size and turning (for the
observatory), where Earth and the Sun are (DE430), where the Didymos system
is (its barycentre, solution s205) and where its two bodies are about it
(solution s542, DART's impact in it: Dimorphos's orbit 11.921 h before,
11.37 h after), and how they turn (Didymos's pole, DIMORPHOS_FIXED kept
facing Didymos through the impact).

From NAIF's DART SPICE archive,
https://naif.jpl.nasa.gov/pub/naif/pds/pds4/dart/dart_spice/spice_kernels/,
in the folders of its server. Set PATH_VALUES to where yours are.

   \begindata

     PATH_VALUES       = ( '/Users/gregoireh/data/spice/dart' )
     PATH_SYMBOLS      = ( 'KERNELS' )

     KERNELS_TO_LOAD   = (
                           '$KERNELS/lsk/naif0012.tls'
                           '$KERNELS/pck/pck00010.tpc'
                           '$KERNELS/pck/didymos_system_15.tpc'
                           '$KERNELS/fk/didymos_system_008.tf'
                           '$KERNELS/spk/de430.bsp'
                           '$KERNELS/spk/didymos_barycenter_s205_v01.bsp'
                           '$KERNELS/spk/didymos_system_s542_v01.bsp'
                         )

   \begintext
