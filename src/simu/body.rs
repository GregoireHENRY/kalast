use crate::{matrix_orientation_obliquity, util::*, Asteroid, CfgBody};

use itertools::Itertools;

#[derive(Debug, Clone)]
pub struct Body {
    pub id: String,
    pub asteroid: Asteroid,
    pub mat_orient: Mat4,
    pub normals: Matrix3xX<Float>,
}

impl Body {
    pub fn new(asteroid: Asteroid, cb: &CfgBody) -> Self {
        let mat_orient = matrix_orientation_obliquity(0.0, cb.spin.obliquity * RPD);

        let normals = Matrix3xX::from_columns(
            &asteroid
                .surface
                .faces
                .iter()
                .map(|f| f.vertex.normal)
                .collect_vec(),
        );

        Self {
            id: cb.id.clone(),
            asteroid,
            mat_orient,
            normals,
        }
    }
}
