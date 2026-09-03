#
# Cosmographia: HERA AFC at the start of the kalast / shapeviewer window.
#
# Puts the camera just behind the spacecraft on the HERA_AFC-1 boresight and
# looks straight down it, so the instrument's frustum is seen end-on with Mars
# inside it. Nothing here changes a shape or an ephemeris: Cosmographia's own
# solarsys.json already drives Mars, Phobos and Deimos from SPICE (Mars as a
# globe at 3396.19 / 3396.19 / 3376.20 km), and the scenario below furnishes
# the same HERA meta-kernel kalast uses, hera_ops_local.tm.
#
# AFC field of view: 5.47 x 5.47 deg (1024x1024)
#
# Run:  File > Run Script (SHIFT-CMD-R), or  Cosmographia -p view_afc.py
#
# Paths are absolute because they are machine-specific, like the rest of this
# project's data paths; see local_paths.toml at the repo root. Move this folder
# and FOLDER below is the one line to update.
#
import os

import cosmoscripting

cosmo = cosmoscripting.Cosmo()


FOLDER      = "/Users/gregoireh/projects/kalast/examples/hera_mars_swingby/cosmographia"
SENSOR_NAME = "sensor_HERA_AFC-1-MARS.json"
SCENARIO    = "/Users/gregoireh/data/spice/hera/misc/cosmo/scenarios/load_hera_ops_001.json"


def sensor_catalog():
    """Absolute path to the sensor catalog, whatever the working directory is.

    Cosmographia runs a script with its own data directory as the working
    directory and hands it a bare relative `__file__`, so `os.path.abspath`
    resolves it against <app>/Contents/Resources/data -- which is exactly where
    the first version of this went looking, and failed. Candidates are tried in
    order and the first one that actually exists on disk wins.
    """
    candidates = [FOLDER]
    try:
        candidates.insert(0, cosmo.scriptDir())          # Cosmographia 4.1+
    except AttributeError:
        pass
    try:
        if os.path.isabs(__file__):
            candidates.insert(0, os.path.dirname(__file__))
    except NameError:
        pass

    for directory in candidates:
        path = os.path.join(directory, "sensors", SENSOR_NAME)
        if os.path.isfile(path):
            return path

    cosmo.displayNote("sensor catalog not found, looked in: "
                      + ", ".join(candidates), 10)
    return os.path.join(FOLDER, "sensors", SENSOR_NAME)


SENSOR = sensor_catalog()

# The first epoch shapeviewer and kalast have in common. The dashboard CSV runs
# 10:00:00 to 14:00:00 UTC on this date, and our renders start at its first row.
EPOCH = "2025-03-12 10:00:00 UTC"

# How far behind the instrument to sit, in km. Big enough to see the spacecraft
# and the near end of the frustum; small enough that Mars still subtends its
# 4.26 deg. Raise it to take in more of the frustum, drop it towards 0 to sit at
# the instrument itself and see what it sees.
STANDOFF = 0.05

cosmo.displayNote( "HERA AFC-1  -  " + EPOCH, 3 )

# --- scene -----------------------------------------------------------------
cosmo.loadCatalogFile( SCENARIO )
cosmo.loadCatalogFile( SENSOR )

# --- time ------------------------------------------------------------------
cosmo.setTime( EPOCH )
cosmo.setTimeRate( 1 )
cosmo.pause()

# --- what to look at -------------------------------------------------------
cosmo.setCentralObject( "HERA" )
cosmo.selectObject( "HERA" )
cosmo.setCameraToBodyFixedFrame()

cosmo.showObject( "HERA" )
cosmo.showObject( "HERA_AFC-1-SENSOR" )

# The instrument frame's axes, so the boresight is visible as well as implied.
cosmo.showSpiceFrame( "HERA", "HERA_AFC-1" )
cosmo.showDirectionVector( "HERA", "Mars" )
cosmo.showDirectionVector( "HERA", "Deimos" )

cosmo.showLabels()
cosmo.showInfoText()

# --- camera: behind the instrument, looking along +Z of HERA_AFC-1 ------
# vec1 = position in the frame, vec2 = view direction, vec3 = up.
try:
    cosmo.moveToPovSpiceFrame( "HERA", "HERA_AFC-1",
                               [ 0.0, 0.0, -STANDOFF ],
                               [ 0.0, 0.0,  1.0 ],
                               [ 0.0, 1.0,  0.0 ], 0 )
except AttributeError:
    # Cosmographia 4.0 has no moveToPovSpiceFrame. HERA_TIRI is 0.0000 deg off
    # HERA_SPACECRAFT in the FK and HERA_AFC-1 is 0.146 deg off, so the
    # spacecraft body-fixed frame is a close stand-in for either.
    cosmo.displayNote( "moveToPovSpiceFrame unavailable, using HERA body-fixed frame", 4 )
    cosmo.moveToPov( "HERA",
                     [ 0.0, 0.0, -STANDOFF ],
                     [ 0.0, 0.0,  1.0 ],
                     [ 0.0, 1.0,  0.0 ], 0 )

cosmo.setFov( 30, 0 )

cosmo.displayNote( "AFC boresight, frustum end-on, Mars 91,364 km ahead", 6 )
