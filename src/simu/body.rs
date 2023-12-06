use crate::{matrix_orientation_obliquity, util::*, AirlessBody, CfgBody};

use itertools::Itertools;

#[derive(Debug, Clone)]
pub struct BodyData {
    pub normals: Matrix3xX<Float>,
    pub translation: Mat4,
    pub orientation: Mat4,
}

impl BodyData {
    pub fn new(asteroid: &AirlessBody, cb: &CfgBody) -> Self {
        let normals = Matrix3xX::from_columns(
            &asteroid
                .surface
                .faces
                .iter()
                .map(|f| f.vertex.normal)
                .collect_vec(),
        );

        let translation = Mat4::identity();
        let orientation = matrix_orientation_obliquity(0.0, cb.spin.obliquity * RPD);

        Self {
            normals,
            translation,
            orientation,
        }
    }
}
