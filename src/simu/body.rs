use crate::{config::Body, util::*, AirlessBody, ColorMode};

use itertools::Itertools;

#[derive(Debug, Clone)]
pub struct FacetColorChanged {
    pub index: usize,

    // `Mode` and `color` is used to store previous state of facet before changing it to revert it later.
    pub mode: ColorMode,
    pub color: Vec3,
}

impl FacetColorChanged {
    pub fn set(body: &AirlessBody, index: usize) -> Self {
        match &body.surface.faces[index].vertex.color_mode {
            ColorMode::Color | ColorMode::DiffuseLight => Self::set_color(body, index),
            ColorMode::Data => Self::set_data(body, index),
        }
    }

    pub fn set_color(body: &AirlessBody, index: usize) -> Self {
        let face = &body.surface.faces[index].vertex;
        Self {
            index,
            mode: face.color_mode,
            color: face.color,
        }
    }

    pub fn set_data(body: &AirlessBody, index: usize) -> Self {
        let face = &body.surface.faces[index].vertex;
        Self {
            index,
            mode: face.color_mode,
            color: vec3(face.data, 0.0, 0.0),
        }
    }
}

#[derive(Debug, Clone)]
pub struct BodyData {
    pub normals: Matrix3xX<Float>,
    pub translation: Mat4,
    pub orientation: Mat4,
    pub facets_selected: Vec<FacetColorChanged>,
    pub facets_showing_view_factor: Vec<FacetColorChanged>,
}

impl BodyData {
    pub fn new(asteroid: &AirlessBody, _cb: &Body) -> Self {
        let normals = Matrix3xX::from_columns(
            &asteroid
                .surface
                .faces
                .iter()
                .map(|f| f.vertex.normal)
                .collect_vec(),
        );

        let translation = Mat4::identity();
        let orientation = Mat4::identity();
        let facets_selected = vec![];
        let facets_showing_view_factor = vec![];

        Self {
            normals,
            translation,
            orientation,
            facets_selected,
            facets_showing_view_factor,
        }
    }
}
