use crate::{config::Body, util::*, AirlessBody, ColorMode};

use itertools::Itertools;

#[derive(Debug, Clone)]
pub struct SelectedFace {
    pub index: usize,
    pub mode: ColorMode,
    pub color: Vec3,
}

impl SelectedFace {
    pub fn set(body: &AirlessBody, index: usize) -> Self {
        let face = &body.surface.faces[index].vertex;
        Self {
            index,
            mode: face.color_mode,
            color: face.color,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BodyData {
    pub normals: Matrix3xX<Float>,
    pub translation: Mat4,
    pub orientation: Mat4,
    pub selected: Vec<SelectedFace>,
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
        let selected = vec![];

        Self {
            normals,
            translation,
            orientation,
            selected,
        }
    }
}
