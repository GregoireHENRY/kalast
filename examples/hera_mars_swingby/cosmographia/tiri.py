import cosmoscripting

cosmo = cosmoscripting.Cosmo()

# cosmo.loadCatalogFile(
#     "/Users/gregoireh/data/spice/hera/misc/cosmo/scenarios/load_hera_ops_001.json"
# )
cosmo.loadCatalogFile(
    "/Users/gregoireh/projects/kalast/examples/hera_mars_swingby/cosmographia/sensors/sensor_HERA_TIRI-MARS.json"
)
cosmo.setTime("2025-03-12 10:00:00 UTC")
cosmo.setTimeRate(1)
cosmo.pause()

# cosmo.setCentralObject("HERA")
# cosmo.selectObject("HERA")
# cosmo.setCameraToBodyFixedFrame()

# cosmo.showSpiceFrame("HERA", "HERA_TIRI")
# cosmo.showDirectionVector("HERA", "Mars")
# cosmo.showDirectionVector("HERA", "Deimos")

# cosmo.showLabels()
# cosmo.showInfoText()

# cosmo.moveToPovSpiceFrame(
#     "HERA", "HERA_TIRI", [0.0, 0.0, -0.05], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0], 0
# )

# cosmo.moveToPov("HERA", [0.0, 0.0, -STANDOFF], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0], 0)

# cosmo.setFov(30, 0)