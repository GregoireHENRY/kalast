use crate::{config::Body, util::*, AirlessBody};

use itertools::Itertools;

#[derive(Debug, Clone)]
pub struct BodyData {
    pub normals: Matrix3xX<Float>,
    pub translation: Mat4,
    pub orientation: Mat4,
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

        Self {
            normals,
            translation,
            orientation,
        }
    }
}
