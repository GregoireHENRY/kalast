use crate::{
    cosine_angle, util::*, AirlessBody, Cfg, CfgBody, CfgFrameCenter, CfgMeshKind, CfgMeshSource,
    CfgOrbitSpeedControl, CfgStateOrbit, Material, Result, Surface, Window,
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

/// return MU and factor for distances.
pub fn find_ref_orbit(orbit: &CfgStateOrbit, cfgs: &[&CfgBody]) -> (Float, Float) {
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
    sun_direction: &Vec3,
) -> DRVector<Float> {
    let matrix_normal: Mat3 = glm::mat4_to_mat3(&glm::inverse_transpose(body.matrix_model));
    let normals_oriented = matrix_normal * normals;
    cosine_angle(sun_direction, &normals_oriented)
}

pub fn compute_cosine_emission_angle(
    body: &AirlessBody,
    normals: &Matrix3xX<Float>,
    camera_direction: &Vec3,
) -> DRVector<Float> {
    let matrix_normal: Mat3 = glm::mat4_to_mat3(&glm::inverse_transpose(body.matrix_model));
    let normals_oriented = matrix_normal * normals;
    cosine_angle(camera_direction, &normals_oriented)
}

pub fn compute_cosine_phase_angle(
    body: &AirlessBody,
    camera_direction: &Vec3,
    sun_direction: &Vec3,
) -> DRVector<Float> {
    DRVector::from_row_slice(
        &body
            .surface
            .faces
            .iter()
            .map(|f| {
                let v4 = glm::vec3_to_vec4(&f.vertex.position);
                let v3_oriented = glm::vec4_to_vec3(&(body.matrix_model * v4));
                (sun_direction - v3_oriented)
                    .normalize()
                    .dot(&(camera_direction - v3_oriented).normalize())
            })
            .collect_vec(),
    )
}
