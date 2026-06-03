use glam::Vec3Swizzles;
use numpy::ndarray::ArrayView1;

use crate::{Float, Vec3};

pub fn cosine_angle_vectors(u: &Vec3, v: &Vec3) -> Float {
    u.dot(*v)
}

pub fn cosine_incidence(sundir: &Vec3, normal: &Vec3) -> Float {
    cosine_angle_vectors(sundir, normal).max(0.0)
}

pub fn flattening_radius(radii: &Vec3) -> Float {
    (radii.x - radii.z) / radii.x
}

pub fn cart2sph(v: &Vec3) -> Vec3 {
    let r = v.length();
    let theta = v.y.atan2(v.x);
    let phi = v.z.atan2(v.xy().length());

    Vec3::new(r, theta, phi)
}

pub fn sph2cart(v: &Vec3) -> Vec3 {
    let x = v.x * v.z.cos() * v.y.cos();
    let y = v.x * v.z.cos() * v.y.sin();
    let z = v.x * v.z.sin();

    Vec3::new(x, y, z)
}

pub fn trapez(y: ArrayView1<Float>, x: ArrayView1<Float>) -> Float {
    let mut r = 0.0;

    for ii in 0..(x.len() - 1) {
        let h = x[ii + 1] - x[ii];
        r += (y[ii] + y[ii + 1]) * h / 2.0
    }

    r
}

pub fn simpson_1_3(y: ArrayView1<Float>, x: ArrayView1<Float>) -> Float {
    let mut r = 0.0;

    for ii in (0..x.len()).step_by(2) {
        let h = x[ii + 2] - x[ii];
        r += (y[ii] + 4.0 * y[ii + 1] + y[ii + 2]) * h / 6.0
    }

    r
}

pub fn simpson_3_8(y: ArrayView1<Float>, x: ArrayView1<Float>) -> Float {
    let mut r = 0.0;

    for ii in (0..x.len()).step_by(3) {
        let h = x[ii + 3] - x[ii];
        r += (y[ii] + 3.0 * y[ii + 1] + 3.0 * y[ii + 2] + y[ii + 3]) * h / 8.0
    }

    r
}

pub fn boole(y: ArrayView1<Float>, x: ArrayView1<Float>) -> Float {
    let mut r = 0.0;

    for ii in (0..x.len()).step_by(4) {
        let h = x[ii + 4] - x[ii];
        r +=
            (7.0 * y[ii] + 32.0 * y[ii + 1] + 12.0 * y[ii + 2] + 32.0 * y[ii + 3] + 7.0 * y[ii + 4])
                * h
                / 90.0
    }

    r
}

pub(crate) mod py {
    use numpy::{PyReadonlyArray1, ToPyArray};
    use pyo3::prelude::*;

    use crate::{Float, Vec3};

    type Array<'py> = numpy::PyReadonlyArray1<'py, Float>;

    #[pyfunction]
    pub fn cosine_angle_vectors(u: Array<'_>, v: Array<'_>) -> PyResult<Float> {
        Ok(super::cosine_angle_vectors(
            &Vec3::from_slice(u.as_slice().unwrap()),
            &Vec3::from_slice(v.as_slice().unwrap()),
        ))
    }

    #[pyfunction]
    pub fn cosine_incidence(sundir: Array<'_>, normal: Array<'_>) -> PyResult<Float> {
        Ok(super::cosine_incidence(
            &Vec3::from_slice(sundir.as_slice().unwrap()),
            &Vec3::from_slice(normal.as_slice().unwrap()),
        ))
    }

    #[pyfunction]
    pub fn flattening_radius(radii: Array<'_>) -> PyResult<Float> {
        Ok(super::flattening_radius(&Vec3::from_slice(
            radii.as_slice().unwrap(),
        )))
    }

    #[pyfunction]
    pub fn cart2sph<'py>(
        v: [Float; 3],
        py: Python<'py>,
    ) -> pyo3::Bound<'py, numpy::PyArray1<Float>> {
        super::cart2sph(&v.into()).to_array().to_pyarray(py)
    }

    #[pyfunction]
    pub fn sph2cart<'py>(
        v: [Float; 3],
        py: Python<'py>,
    ) -> pyo3::Bound<'py, numpy::PyArray1<Float>> {
        super::sph2cart(&v.into()).to_array().to_pyarray(py)
    }

    #[pyfunction]
    pub fn trapez(
        y: PyReadonlyArray1<'_, Float>,
        x: PyReadonlyArray1<'_, Float>,
    ) -> PyResult<Float> {
        Ok(super::trapez(y.as_array(), x.as_array()))
    }

    #[pyfunction]
    pub fn simpson_1_3(
        y: PyReadonlyArray1<'_, Float>,
        x: PyReadonlyArray1<'_, Float>,
    ) -> PyResult<Float> {
        Ok(super::simpson_1_3(y.as_array(), x.as_array()))
    }

    #[pyfunction]
    pub fn simpson_3_8(
        y: PyReadonlyArray1<'_, Float>,
        x: PyReadonlyArray1<'_, Float>,
    ) -> PyResult<Float> {
        Ok(super::simpson_3_8(y.as_array(), x.as_array()))
    }

    #[pyfunction]
    pub fn boole(
        y: PyReadonlyArray1<'_, Float>,
        x: PyReadonlyArray1<'_, Float>,
    ) -> PyResult<Float> {
        Ok(super::boole(y.as_array(), x.as_array()))
    }
}
