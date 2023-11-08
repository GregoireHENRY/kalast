use kalast::prelude::*;

fn main() -> Result<()> {
    let path = Path::new(file!()).parent().unwrap();
    let sc = Scenario::new(path)?;

    let sc = sc.select_routines(simu::routines_thermal_default());
    let mut sc = sc.select_body_type::<BodyCustom>();

    sc.iterations()?;

    // let user choose when to enable mutual shadows in the default thermal routines.
    // check how to change just the function fn_end_of_iteration of routine

    Ok(())
}

#[derive(Debug, Clone)]
pub struct BodyCustom {
    pub id: String,
    pub asteroid: Asteroid,
    pub mat_orient: Mat4,
    pub normals: Matrix3xX<Float>,
}

impl Body for BodyCustom {
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
