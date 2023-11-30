use crate::{matrix_orientation_obliquity, util::*, AirlessBody, CfgBody};

use itertools::Itertools;

#[derive(Debug, Clone)]
pub struct PreComputedBody {
    pub mat_orient: Mat4,
    pub normals: Matrix3xX<Float>,
}

impl PreComputedBody {
    pub fn new(asteroid: &AirlessBody, cb: &CfgBody) -> Self {
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
            mat_orient,
            normals,
        }
    }
}
