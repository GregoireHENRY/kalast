use crate::prelude::*;

pub trait Body: fmt::Debug + Clone {
    fn new(asteroid: Asteroid, cb: &CfgBody) -> Self;
    fn id(&self) -> &str;
    fn asteroid(&self) -> &Asteroid;
    fn asteroid_mut(&mut self) -> &mut Asteroid;
    fn mat_orient(&self) -> &Mat4;
    fn normals(&self) -> &Matrix3xX<Float>;
}

#[derive(Debug, Clone)]
pub struct BodyDefault {
    pub id: String,
    pub asteroid: Asteroid,
    pub mat_orient: Mat4,
    pub normals: Matrix3xX<Float>,
}

impl Body for BodyDefault {
    fn new(asteroid: Asteroid, cb: &CfgBody) -> Self {
        let mat_orient = ast::matrix_orientation_obliquity(0.0, cb.spin.obliquity * RPD);

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

    fn id(&self) -> &str {
        &self.id
    }

    fn asteroid(&self) -> &Asteroid {
        &self.asteroid
    }

    fn asteroid_mut(&mut self) -> &mut Asteroid {
        &mut self.asteroid
    }

    fn mat_orient(&self) -> &Mat4 {
        &self.mat_orient
    }

    fn normals(&self) -> &Matrix3xX<Float> {
        &self.normals
    }
}
