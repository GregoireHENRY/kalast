use crate::{
    cosine_angle, matrix_spin, position_in_inertial_frame, simu::Scene, util::*, AirlessBody, Cfg,
    CfgBody, CfgFrameCenter, CfgMeshKind, CfgMeshSource, CfgOrbitKepler, CfgOrbitSpeedControl,
    CfgState, CfgStateCartesian, Material, PreComputedBody, Result, Surface, Time, Window,
};

use itertools::{izip, Itertools};

pub fn read_surface(cb: &CfgBody, kind: CfgMeshKind) -> Result<Surface> {
    let mesh = match kind {
        CfgMeshKind::Main => &cb.mesh,
        CfgMeshKind::Low => &cb.mesh_low.as_ref().unwrap(),
    };

    let mut builder = match &mesh.shape {
        CfgMeshSource::Path(p) => Surface::read_file(p)?,
        CfgMeshSource::Shape(m) => Surface::use_integrated(*m)?,
    };

    if !mesh.smooth {
        builder = builder.flat();
    }

    let surface = builder
        .update_all(|v| {
            v.position.x *= mesh.factor.x;
            v.position.y *= mesh.factor.y;
            v.position.z *= mesh.factor.z;
            v.material = Material {
                albedo: cb.material.albedo,
                emissivity: cb.material.emissivity,
                thermal_inertia: cb.material.thermal_inertia,
                density: cb.material.density,
                heat_capacity: cb.material.heat_capacity,
            };
            v.color_mode = cb.color;
        })
        .build();

    Ok(surface)
}

pub fn read_surface_main(cb: &CfgBody) -> Result<Surface> {
    read_surface(cb, CfgMeshKind::Main)
}

pub fn read_surface_low(cb: &CfgBody) -> Result<Surface> {
    read_surface(cb, CfgMeshKind::Low)
}

pub fn find_reference_matrix_orientation(
    cfg: &Cfg,
    body: usize,
    pre_computed_bodies: &mut [PreComputedBody],
) -> Mat4 {
    match &cfg.bodies[body].state {
        CfgState::Cartesian(CfgStateCartesian { orientation, .. }) => {
            glm::mat3_to_mat4(&orientation)
        }
        CfgState::Equatorial(_) => Mat4::identity(),
        CfgState::File(_p) => Mat4::identity(),
        CfgState::Orbit(orb) => match &orb.frame {
            CfgFrameCenter::Sun => Mat4::identity(),
            CfgFrameCenter::Body(id) => {
                let mut mat = Mat4::identity();
                for (pre, cb) in izip!(pre_computed_bodies, &cfg.bodies) {
                    if cb.id == *id {
                        mat = pre.mat_orient;
                        break;
                    }
                }
                mat
            }
        },
    }
}

pub fn find_matrix_translation(cfg: &Cfg, body: usize, time: &Time) -> Mat4 {
    let elapsed_from_start = time.elapsed_seconds_from_start();
    let other_bodies = cfg
        .bodies
        .iter()
        .enumerate()
        .filter(|(ii, _)| *ii != body)
        .map(|(_, cb)| cb)
        .collect_vec();

    match &cfg.bodies[body].state {
        CfgState::Cartesian(CfgStateCartesian {
            position,
            orientation: _orientation,
        }) => Mat4::new_translation(position),
        CfgState::Equatorial(_) | CfgState::File(_) => Mat4::identity(),
        CfgState::Orbit(orb) => {
            let (mu_ref, factor) = find_ref_orbit(&orb, &other_bodies);
            if mu_ref == MU_SUN {
                Mat4::identity()
            } else {
                let pos = position_in_inertial_frame(
                    orb.a * factor,
                    orb.e,
                    orb.i * RPD,
                    orb.node * RPD,
                    orb.peri * RPD,
                    elapsed_from_start as Float,
                    orb.tp,
                    mu_ref,
                );
                Mat4::new_translation(&(pos * 1e-3))
            }
        }
    }
}

pub fn find_matrix_spin(cfg: &Cfg, body: usize, time: &Time) -> Mat4 {
    let elapsed = time.elapsed_seconds();
    if cfg.bodies[body].spin.period == 0.0 {
        Mat4::identity()
    } else {
        let np_elapsed = elapsed as Float / cfg.bodies[body].spin.period;
        let spin = (TAU * np_elapsed + cfg.bodies[body].spin.spin0 * RPD) % TAU;
        matrix_spin(spin)
    }
}

/// return MU and factor for distances.
pub fn find_ref_orbit(orbit: &CfgOrbitKepler, cfgs: &[&CfgBody]) -> (Float, Float) {
    match &orbit.frame {
        CfgFrameCenter::Sun => (MU_SUN, AU),
        CfgFrameCenter::Body(id) => (
            match orbit.control {
                CfgOrbitSpeedControl::Mass(None) | CfgOrbitSpeedControl::Period(_) => {
                    let mut mu = 0.0;
                    for cfg in cfgs {
                        if cfg.id == *id {
                            mu = GRAVITATIONAL_CONSTANT
                                * cfg
                                    .mass
                                    .expect(&format!("The mass of {} is not defined.", id));
                            break;
                        }
                    }
                    mu
                }
                CfgOrbitSpeedControl::Mass(Some(mass)) => GRAVITATIONAL_CONSTANT * mass,
            },
            1e3,
        ),
    }
}

pub fn update_colormap_scalar(
    win: &Window,
    cfg: &Cfg,
    scalars: &[Float],
    asteroid: &mut AirlessBody,
    ii_body: usize,
) {
    let cmap = &cfg.window.colormap;

    for (element, scalar) in izip!(asteroid.surface.elements_mut(), scalars) {
        let mut data = (scalar - cmap.vmin) / (cmap.vmax - cmap.vmin);
        if cmap.reverse {
            data = 1.0 - data;
        }
        element.data = data;
    }

    win.update_vao(ii_body, &mut asteroid.surface);
}

pub fn compute_cosine_incidence_angle(
    body: &AirlessBody,
    normals: &Matrix3xX<Float>,
    scene: &Scene,
) -> DRVector<Float> {
    let matrix_normal: Mat3 = glm::mat4_to_mat3(&glm::inverse_transpose(body.matrix_model));
    let normals_oriented = matrix_normal * normals;
    cosine_angle(&scene.sun_dir(), &normals_oriented)
}

pub fn compute_cosine_emission_angle(
    body: &AirlessBody,
    normals: &Matrix3xX<Float>,
    scene: &Scene,
) -> DRVector<Float> {
    let matrix_normal: Mat3 = glm::mat4_to_mat3(&glm::inverse_transpose(body.matrix_model));
    let normals_oriented = matrix_normal * normals;
    cosine_angle(&scene.cam_dir(), &normals_oriented)
}

pub fn compute_cosine_phase_angle(body: &AirlessBody, scene: &Scene) -> DRVector<Float> {
    DRVector::from_row_slice(
        &body
            .surface
            .faces
            .iter()
            .map(|f| {
                let v4 = glm::vec3_to_vec4(&f.vertex.position);
                let v3_oriented = glm::vec4_to_vec3(&(body.matrix_model * v4));
                (scene.sun_pos - v3_oriented)
                    .normalize()
                    .dot(&(scene.cam_pos - v3_oriented).normalize())
            })
            .collect_vec(),
    )
}
