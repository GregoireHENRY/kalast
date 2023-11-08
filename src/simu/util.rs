use crate::prelude::*;

pub fn read_surface(cb: &CfgBody, kind: CfgMeshKind) -> Result<Surface> {
    let mesh = match kind {
        CfgMeshKind::Main => &cb.mesh,
        CfgMeshKind::Low => &cb.mesh_low.as_ref().unwrap(),
    };

    let builder = match &mesh.shape {
        CfgMeshSource::Path(p) => Surface::read_file(p)?,
        CfgMeshSource::Shape(m) => Surface::use_integrated(*m)?,
    };
    
    let surface = builder
        .flat()
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

pub fn find_reference_matrix_orientation<B: Body>(cb: &CfgBody, other_bodies: &[&B]) -> Mat4 {
    match &cb.state {
        None => Mat4::identity(),
        Some(state) => match state {
            CfgState::Path(_p) => Mat4::identity(),
            CfgState::Orbit(orb) => match &orb.frame {
                CfgFrameCenter::Sun => Mat4::identity(),
                CfgFrameCenter::Body(id) => {
                    let mut mat = Mat4::identity();
                    for other_body in other_bodies {
                        if other_body.id() == *id {
                            mat = *other_body.mat_orient();
                            break;
                        }
                    }
                    mat
                }
            },
        },
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
    cmap: &CfgColormap,
    scalars: &[Float],
    asteroid: &mut Asteroid,
    ii_body: usize,
) {
    for (element, scalar) in izip!(asteroid.surface.elements_mut(), scalars) {
        let mut data = (scalar - cmap.vmin) / (cmap.vmax - cmap.vmin);
        if cmap.reverse {
            data = 1.0 - data;
        }
        element.data = data;
    }

    win.update_vao(ii_body, &mut asteroid.surface);
}

pub fn compute_cosine_incidence_angle<B: Body>(
    body: &B,
    normals: &Matrix3xX<Float>,
    scene: &Scene,
) -> DRVector<Float> {
    let matrix_normal: Mat3 =
        glm::mat4_to_mat3(&glm::inverse_transpose(body.asteroid().matrix_model));
    let normals_oriented = matrix_normal * normals;
    flux::cosine_angle(&scene.sun_dir(), &normals_oriented)
}

pub fn compute_cosine_emission_angle<B: Body>(
    body: &B,
    normals: &Matrix3xX<Float>,
    scene: &Scene,
) -> DRVector<Float> {
    let matrix_normal: Mat3 =
        glm::mat4_to_mat3(&glm::inverse_transpose(body.asteroid().matrix_model));
    let normals_oriented = matrix_normal * normals;
    flux::cosine_angle(&scene.cam_dir(), &normals_oriented)
}

pub fn compute_cosine_phase_angle<B: Body>(body: &B, scene: &Scene) -> DRVector<Float> {
    DRVector::from_row_slice(
        &body
            .asteroid()
            .surface
            .faces
            .iter()
            .map(|f| {
                let v4 = glm::vec3_to_vec4(&f.vertex.position);
                let v3_oriented = glm::vec4_to_vec3(&(body.asteroid().matrix_model * v4));
                (scene.sun_pos - v3_oriented)
                    .normalize()
                    .dot(&(scene.cam_pos - v3_oriented).normalize())
            })
            .collect_vec(),
    )
}
