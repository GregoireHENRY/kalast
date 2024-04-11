use crate::{body::interior::Interior, util::*, InteriorGrid, Surface};

use itertools::Itertools;
use polars::prelude::{CsvReader, SerReader};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct AirlessBody {
    pub surface: Surface,
    pub lowres: Option<Surface>,
    pub interior: Option<Interior>,
    pub matrix_model: Mat4,
}

impl AirlessBody {
    pub fn new(surface: Surface) -> Self {
        Self {
            surface,
            lowres: None,
            interior: None,
            matrix_model: Mat4::identity(),
        }
    }

    pub fn with_matrix_model(self, matrix: Mat4) -> Self {
        Self {
            surface: self.surface,
            lowres: self.lowres,
            interior: self.interior,
            matrix_model: matrix,
        }
    }

    pub fn with_interior_grid(self, interior: InteriorGrid) -> Self {
        Self {
            surface: self.surface,
            lowres: self.lowres,
            interior: Some(Interior::Grid(interior)),
            matrix_model: self.matrix_model,
        }
    }

    pub fn with_interior_grid_depth(self, depth: Vec<Float>) -> Self {
        Self {
            surface: self.surface,
            lowres: self.lowres,
            interior: Some(Interior::Grid(InteriorGrid { depth })),
            matrix_model: self.matrix_model,
        }
    }

    pub fn with_interior_grid_fn<F>(self, depth: F, size: usize) -> Self
    where
        F: Fn(usize) -> Float,
    {
        Self {
            surface: self.surface,
            lowres: self.lowres,
            interior: Some(Interior::Grid(InteriorGrid::from_fn(depth, size))),
            matrix_model: self.matrix_model,
        }
    }

    pub fn with_interior_grid_fn_linear(self, size: usize, a: Float) -> Self {
        Self::with_interior_grid_fn(self, |ii| a * ii as Float, size)
    }

    pub fn with_interior_grid_fn_pow(self, size: usize, a: Float, n: Float) -> Self {
        Self::with_interior_grid_fn(self, |ii| a * (ii as Float).powf(n), size)
    }

    pub fn with_interior_grid_fn_exp(self, size: usize, a: Float) -> Self {
        Self::with_interior_grid_fn(self, |ii| a * (ii as Float).exp() - a, size)
    }

    pub fn with_interior_grid_from_file<P: AsRef<Path>>(self, path: P) -> Self {
        let df = CsvReader::from_path(path.as_ref())
            .unwrap()
            .has_header(false)
            .finish()
            .unwrap();
        let col = df
            .column(&"f0")
            .unwrap()
            .f64()
            .unwrap()
            .into_iter()
            .map(|v| v.unwrap())
            .collect_vec();
        Self::with_interior_grid_depth(self, col)
    }
}

pub fn matrix_spin(spin: Float, axis: Vec3) -> Mat4 {
    if spin == 0.0 {
        Mat4::identity()
    } else {
        Mat4::new_rotation(spin * axis)
    }
}

pub fn matrix_orientation_obliquity(longitude: Float, obliquity: Float) -> Mat4 {
    // let longitude = (node + peri - cb.spin.longitude) * RPD;
    // let obliquity = (cb.spin.obliquity - i) * RPD;

    let tilt_longitude = Mat4::new_rotation(longitude * Vec3::z());
    let tilt_obliquity = Mat4::new_rotation(obliquity * Vec3::y());
    tilt_longitude * tilt_obliquity
}

pub fn matrix_spin_oriented(spin: &Mat4, orientation: &Mat4) -> Mat4 {
    orientation * spin
}

pub fn matrix_spin_oriented_and_rotation(
    spin: &Mat4,
    orientation: &Mat4,
    translation: &Mat4,
    orientation_reference: &Mat4,
) -> Mat4 {
    orientation_reference * translation * orientation * spin
}
