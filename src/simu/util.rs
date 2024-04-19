use crate::config as cg;
use crate::{
    config::Config, cosine_angle, util::*, AirlessBody, Material, Result, Surface, Window,
};

use cg::{CMAP_VMAX, CMAP_VMIN};
use itertools::{izip, Itertools};

pub fn read_surface(cb: &cg::Body, kind: cg::MeshKind) -> Result<Surface> {
    let mesh = match kind {
        cg::MeshKind::Main => &cb.mesh,
        cg::MeshKind::Low => &cb.mesh_low.as_ref().unwrap(),
    };

    let mut builder = match &mesh.shape {
        cg::MeshSource::Path(p) => Surface::read_file(p)?,
        cg::MeshSource::Shape(m) => Surface::use_integrated(*m)?,
    };

    if !mesh.smooth {
        builder = builder.flat();
    }

    let surface = builder
        .update_all(|v| {
            // v.position.x = mesh.orientation * mesh.factor.x * v.position.x + mesh.position.x;
            // v.position.y *= mesh.factor.y;
            // v.position.z *= mesh.factor.z;

            v.position = mesh.orientation * v.position.component_mul(&mesh.factor) + mesh.position;

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

pub fn read_surface_main(cb: &cg::Body) -> Result<Surface> {
    read_surface(cb, cg::MeshKind::Main)
}

pub fn read_surface_low(cb: &cg::Body) -> Result<Surface> {
    read_surface(cb, cg::MeshKind::Low)
}

/// return MU and factor for distances.
pub fn find_ref_orbit(orbit: &cg::StateOrbit, cfgs: &[&cg::Body]) -> (Float, Float) {
    match &orbit.frame {
        None | Some(cg::FrameCenter::Sun) => (MU_SUN, AU),
        Some(cg::FrameCenter::Body(id)) => (
            match orbit.control {
                None | Some(cg::OrbitSpeedControl::Period(_)) => {
                    let mut mu = 0.0;
                    for cfg in cfgs {
                        if cfg.name == *id {
                            mu = GRAVITATIONAL_CONSTANT
                                * cfg
                                    .mass
                                    .expect(&format!("The mass of {} is not defined.", id));
                            break;
                        }
                    }
                    mu
                }
                Some(cg::OrbitSpeedControl::Mass(mass)) => GRAVITATIONAL_CONSTANT * mass,
            },
            1e3,
        ),
    }
}

pub fn update_colormap_scalar(
    _win: &Window,
    config: &Config,
    scalars: &[Float],
    asteroid: &mut AirlessBody,
    _ii_body: usize,
) {
    if let Some(cmap) = config.window.colormap.as_ref() {
        for (element, scalar) in izip!(asteroid.surface.elements_mut(), scalars) {
            let vmin = cmap.vmin.unwrap_or(CMAP_VMIN);
            let vmax = cmap.vmax.unwrap_or(CMAP_VMAX);
            let mut data = (scalar - vmin) / (vmax - vmin);
            if let Some(true) = cmap.reverse {
                data = 1.0 - data;
            }
            element.data = data;
        }
    }

    // win.update_vao(ii_body, &mut asteroid.surface);
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
