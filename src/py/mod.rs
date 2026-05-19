pub mod app;
pub mod entity;
pub mod mesh;
pub mod routines;
pub mod spice;
pub mod tpm;
pub mod util;

use pyo3::prelude::*;

use crate::{pyadd_c, pyadd_f};

#[pymodule]
#[pyo3(name = "_rs")]
fn python_module(py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
    let util = PyModule::new(m.py(), "util")?;
    pyadd_c!(util, crate::util::EPSILON);
    pyadd_c!(util, crate::util::HOUR);
    pyadd_c!(util, crate::util::DAY);
    pyadd_c!(util, crate::util::PI);
    pyadd_c!(util, crate::util::DPR);
    pyadd_c!(util, crate::util::RPD);
    pyadd_c!(util, crate::util::AU);
    pyadd_c!(util, crate::util::SOLAR_CONSTANT);
    pyadd_c!(util, crate::util::STEFAN_BOLTZMANN);
    pyadd_c!(util, crate::util::PLANK_CONSTANT);
    pyadd_c!(util, crate::util::SPEED_LIGHT);
    pyadd_c!(util, crate::util::BOLTZMANN_CONSTANT);
    pyadd_c!(util, crate::util::TWO_C);
    pyadd_c!(util, crate::util::HC);
    pyadd_c!(util, crate::util::HC2);
    pyadd_c!(util, crate::util::HC_PER_K);
    pyadd_c!(util, crate::util::TWO_HC2);
    pyadd_c!(util, crate::util::TEMP_SUN);
    pyadd_c!(util, crate::util::RADIUS_SUN);
    pyadd_c!(util, crate::util::JANSKY);
    pyadd_c!(util, crate::util::BAND_V0);
    pyadd_c!(util, crate::util::GRAVITATIONAL_CONSTANT);
    pyadd_c!(util, crate::util::MASS_SUN);
    pyadd_c!(util, crate::util::NEWTON_METHOD_MAX_ITERATION);
    pyadd_c!(util, crate::util::NEWTON_METHOD_THRESHOLD);
    pyadd_c!(util, crate::util::SPICE_PICTUR_1);
    pyadd_c!(util, crate::util::SPICE_PICTUR_2);
    pyadd_c!(util, crate::util::SPICE_PICTUR_3);
    pyadd_c!(util, crate::util::SFLUX_545);
    m.add_submodule(&util)?;
    py.import("sys")?
        .getattr("modules")?
        .set_item("kalast._rs.util", util)?;

    let math = PyModule::new(m.py(), "math")?;
    pyadd_f!(math, crate::math::py::cosine_angle_vectors);
    pyadd_f!(math, crate::math::py::cosine_incidence);
    pyadd_f!(math, crate::math::py::flattening_radius);
    pyadd_f!(math, crate::math::py::trapez);
    pyadd_f!(math, crate::math::py::simpson_1_3);
    pyadd_f!(math, crate::math::py::simpson_3_8);
    pyadd_f!(math, crate::math::py::boole);
    m.add_submodule(&math)?;
    py.import("sys")?
        .getattr("modules")?
        .set_item("kalast._rs.math", math)?;

    let spice = PyModule::new(m.py(), "spice")?;
    m.add_submodule(&spice)?;
    py.import("sys")?
        .getattr("modules")?
        .set_item("kalast._rs.spice", spice)?;

    let entity = PyModule::new(m.py(), "entity")?;

    let r = |x| entity::Body::from_raw(x);
    entity.add("EARTH", r(crate::entity::EARTH.clone()))?;
    entity.add("MOON", r(crate::entity::MOON.clone()))?;
    entity.add("MARS", r(crate::entity::MARS.clone()))?;
    entity.add("PHOBOS", r(crate::entity::PHOBOS.clone()))?;
    entity.add("DEIMOS", r(crate::entity::DEIMOS.clone()))?;
    entity.add("DIDYMOS", r(crate::entity::DIDYMOS.clone()))?;
    entity.add("DIMORPHOS", r(crate::entity::DIMORPHOS.clone()))?;
    entity.add("DIMORPHOS_PRE", r(crate::entity::DIMORPHOS_PRE.clone()))?;

    let r = |x| entity::Camera::from_raw(x);
    entity.add("TIRI", r(crate::entity::TIRI.clone()))?;
    entity.add("AFC", r(crate::entity::AFC.clone()))?;

    let r = |x| entity::Spacecraft::from_raw(entity.py(), x);
    entity.add("HERA", r(crate::entity::HERA.clone()))?;
    entity.add("HALCA", r(crate::entity::HALCA.clone()))?;
    entity.add("MEX", r(crate::entity::MEX.clone()))?;
    entity.add("TGO", r(crate::entity::TGO.clone()))?;

    entity.add_class::<entity::Body>()?;
    entity.add_class::<entity::Camera>()?;
    entity.add_class::<entity::Spacecraft>()?;
    m.add_submodule(&entity)?;
    py.import("sys")?
        .getattr("modules")?
        .set_item("kalast._rs.entity", entity)?;

    let mesh = PyModule::new(m.py(), "mesh")?;

    mesh.add_class::<mesh::Vertex>()?;
    mesh.add_class::<mesh::Facet>()?;
    mesh.add_class::<mesh::Material>()?;
    mesh.add_class::<mesh::Mesh>()?;
    // mesh.add_class::<crate::mesh::Model>()?;

    pyadd_f!(mesh, mesh::load_image);
    pyadd_f!(mesh, mesh::normal_facet);
    pyadd_f!(mesh, mesh::area_facet);
    pyadd_f!(mesh, mesh::is_point_in_or_on);
    pyadd_f!(mesh, mesh::is_point_in_or_on_triangle);
    pyadd_f!(mesh, mesh::is_facing_plane);
    pyadd_f!(mesh, mesh::is_not_parallel_to_plane);
    pyadd_f!(mesh, mesh::intersect_plane);
    pyadd_f!(mesh, mesh::intersect_triangle);
    pyadd_f!(mesh, mesh::intersect_triangle_moller_trumblore);
    pyadd_f!(mesh, mesh::intersect_mesh);
    pyadd_f!(mesh, crate::mesh::view_factor_scalar_with_area);
    pyadd_f!(mesh, crate::mesh::view_factor_scalar);
    pyadd_f!(mesh, mesh::view_factor_facets);
    pyadd_f!(mesh, crate::mesh::largest_slope_angle_sphere);
    pyadd_f!(mesh, crate::mesh::curvature_radius);
    pyadd_f!(mesh, crate::mesh::curvature_diameter_from_radius);
    pyadd_f!(mesh, crate::mesh::curvature_diameter_sphere);
    pyadd_f!(mesh, crate::mesh::z_in_crater);
    pyadd_f!(mesh, crate::mesh::rms_slope);
    pyadd_f!(mesh, crate::mesh::rms_slope_hemisphere);
    pyadd_f!(mesh, mesh::rms_slope_terrain);
    pyadd_f!(mesh, crate::mesh::distribution_slope_angles);

    m.add_submodule(&mesh)?;
    py.import("sys")?
        .getattr("modules")?
        .set_item("kalast._rs.mesh", mesh)?;

    let astro = PyModule::new(m.py(), "astro")?;
    // nothing yet
    m.add_submodule(&astro)?;
    py.import("sys")?
        .getattr("modules")?
        .set_item("kalast._rs.astro", astro)?;

    let tpm = PyModule::new(m.py(), "tpm")?;
    m.add_submodule(&tpm)?;
    py.import("sys")?
        .getattr("modules")?
        .set_item("kalast._rs.tpm", &tpm)?;

    let core = PyModule::new(tpm.py(), "core")?;
    pyadd_f!(core, crate::tpm::core::stability);
    pyadd_f!(core, crate::tpm::core::stability_maxdt);
    pyadd_f!(core, crate::tpm::core::conduction);
    pyadd_f!(core, crate::tpm::core::effective_temperature);
    pyadd_f!(core, crate::tpm::core::radiation_sun);
    pyadd_f!(core, crate::tpm::core::radiation_sun_reflected);
    pyadd_f!(core, crate::tpm::core::radiation_sun_reflected_reuse);
    pyadd_f!(core, crate::tpm::core::radiation_emitted);
    pyadd_f!(core, crate::tpm::core::newton_method_fn);
    pyadd_f!(core, crate::tpm::core::newton_method_dfn);
    pyadd_f!(core, crate::tpm::core::py::newton_method);
    pyadd_f!(core, crate::tpm::core::py::conduction_1d);
    tpm.add_submodule(&core)?;
    py.import("sys")?
        .getattr("modules")?
        .set_item("kalast._rs.tpm.core", core)?;

    let properties = PyModule::new(tpm.py(), "properties")?;

    let r = |x| tpm::properties::Properties::from_raw(x);
    properties.add("DIDYMOS", r(crate::tpm::properties::DIDYMOS))?;
    properties.add("DIMORPHOS", r(crate::tpm::properties::DIMORPHOS))?;
    properties.add("MOON", r(crate::tpm::properties::MOON))?;
    properties.add("PHOBOS", r(crate::tpm::properties::PHOBOS))?;
    properties.add("DEIMOS", r(crate::tpm::properties::DEIMOS))?;
    pyadd_f!(properties, crate::tpm::properties::conductivity);
    pyadd_f!(properties, crate::tpm::properties::diffusivity);
    pyadd_f!(properties, crate::tpm::properties::thermal_inertia);
    pyadd_f!(properties, crate::tpm::properties::skin_depth_1);
    pyadd_f!(properties, crate::tpm::properties::skin_depth_2pi);
    properties.add_class::<tpm::properties::Properties>()?;
    tpm.add_submodule(&properties)?;
    py.import("sys")?
        .getattr("modules")?
        .set_item("kalast._rs.tpm.properties", properties)?;

    let emit = PyModule::new(tpm.py(), "emit")?;
    pyadd_f!(emit, crate::tpm::emit::planck);
    pyadd_f!(emit, crate::tpm::emit::planck_photon_count);
    pyadd_f!(emit, crate::tpm::emit::spectral_radiance);
    pyadd_f!(emit, crate::tpm::emit::steradian);
    pyadd_f!(emit, crate::tpm::emit::irradiance);
    pyadd_f!(emit, crate::tpm::emit::reflectance);
    pyadd_f!(emit, crate::tpm::emit::py::radiance);
    tpm.add_submodule(&emit)?;
    py.import("sys")?
        .getattr("modules")?
        .set_item("kalast._rs.tpm.emit", emit)?;

    let routine = PyModule::new(tpm.py(), "routine")?;
    pyadd_f!(routine, crate::tpm::routine::py::update_thermal_state);
    tpm.add_submodule(&routine)?;
    py.import("sys")?
        .getattr("modules")?
        .set_item("kalast._rs.tpm.routine", routine)?;

    let app = PyModule::new(m.py(), "app")?;
    m.add_submodule(&app)?;
    py.import("sys")?
        .getattr("modules")?
        .set_item("kalast._rs.app", &app)?;

    let core = PyModule::new(app.py(), "_core")?;
    core.add_class::<app::App>()?;
    app.add_submodule(&core)?;
    py.import("sys")?
        .getattr("modules")?
        .set_item("kalast._rs.app._core", &core)?;

    let body = PyModule::new(app.py(), "body")?;
    body.add_class::<app::body::Body>()?;
    app.add_submodule(&body)?;
    py.import("sys")?
        .getattr("modules")?
        .set_item("kalast._rs.app.body", &body)?;

    let frame = PyModule::new(app.py(), "frame")?;
    frame.add_class::<app::frame::Eye>()?;
    frame.add_class::<app::frame::Projection>()?;
    app.add_submodule(&frame)?;
    py.import("sys")?
        .getattr("modules")?
        .set_item("kalast._rs.app.frame", &frame)?;

    let config = PyModule::new(app.py(), "config")?;
    config.add_class::<app::config::Config>()?;
    app.add_submodule(&config)?;
    py.import("sys")?
        .getattr("modules")?
        .set_item("kalast._rs.app.config", &config)?;

    let gpu = PyModule::new(app.py(), "gpu")?;
    gpu.add_class::<app::gpu::InstanceInput>()?;
    app.add_submodule(&gpu)?;
    py.import("sys")?
        .getattr("modules")?
        .set_item("kalast._rs.app.gpu", &gpu)?;

    let simulation = PyModule::new(app.py(), "simulation")?;
    simulation.add_class::<app::simulation::Simulation>()?;
    simulation.add_class::<app::simulation::State>()?;
    app.add_submodule(&simulation)?;
    py.import("sys")?
        .getattr("modules")?
        .set_item("kalast._rs.app.simulation", &simulation)?;

    // let routines = PyModule::new(m.py(), "routines")?;
    // m.add_submodule(&routines)?;
    // py.import("sys")?
    //     .getattr("modules")?
    //     .set_item("kalast._rs.routines", &routines)?;

    // let setup = PyModule::new(routines.py(), "setup")?;
    // setup.add_class::<routines::setup::ProgressDebug>()?;
    // setup.add_class::<routines::setup::Time>()?;
    // setup.add_class::<crate::routines::setup::SkinDepthParams>()?;
    // setup.add_class::<routines::setup::BodyDataMap>()?;
    // setup.add_class::<routines::setup::Body>()?;
    // setup.add_class::<routines::setup::Setup>()?;
    // routines.add_submodule(&routines)?;
    // py.import("sys")?
    //     .getattr("modules")?
    //     .set_item("kalast._rs.routines.setup", setup)?;

    Ok(())
}
