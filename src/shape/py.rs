//! `kalast.shape`: maplets from images, for Python.

use numpy::{AllowTypeChange, PyArray1, PyArray2, PyArray3, PyArrayLike2, PyArrayMethods, PyReadonlyArrayDyn};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use super::{M3, V3};

fn err(e: String) -> PyErr {
    PyValueError::new_err(e)
}

/// One image of a surface, and its camera and Sun.
///
/// `image` is `(height, width)`: I/F, or anything proportional to it -- each
/// view's scale is fitted. A pinhole camera at `position`, in the body's
/// frame and the surface's unit, its `axes` the rows of a 3 x 3: `x` along
/// the image's rows (to the next column), `y` down its columns (to the next
/// row), `z` the boresight, in the body's frame -- mirrored is allowed, as
/// an archive may store an image. `focal` is the focal length in pixels,
/// one over a pixel's angle; `centre` where the boresight falls, `(x, y)` in
/// pixels from the image's outer corner -- `(width / 2, height / 2)` by
/// default. `sun` points toward the Sun from the surface, in the body's
/// frame.
///
/// `valid`, `(height, width)`, true for the pixels to use: what the maplet
/// cannot know from its own grid -- the shape model's cast shadows from
/// beyond it, terrain beyond it between it and the camera (a render of the
/// shape model in this geometry has both: its lit pixels whose facet is
/// among `Maplet.facets`) -- and stars, hot pixels, saturation.
///
/// `weight`, 1 by default, the view's share against the others: a fraction
/// each for images taken seconds apart under one geometry.
#[pyclass(name = "View", module = "kalast.shape", from_py_object)]
#[derive(Clone)]
pub struct View {
    pub(crate) inner: super::View,
}

#[pymethods]
impl View {
    #[new]
    #[pyo3(signature = (image, position, axes, focal, sun, centre=None, valid=None, name=String::new(), weight=1.0))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        image: PyArrayLike2<'_, f32, AllowTypeChange>,
        position: [f64; 3],
        axes: [[f64; 3]; 3],
        focal: f64,
        sun: [f64; 3],
        centre: Option<[f64; 2]>,
        valid: Option<PyArrayLike2<'_, u8, AllowTypeChange>>,
        name: String,
        weight: f64,
    ) -> PyResult<Self> {
        let a = image.as_array();
        let (height, width) = (a.shape()[0], a.shape()[1]);
        let pixels: Vec<f32> = a.iter().copied().collect();
        let mask = match valid {
            Some(v) => {
                let v = v.as_array();
                if v.shape() != a.shape() {
                    return Err(err(format!("valid is {:?} for an image of {:?}", v.shape(), a.shape())));
                }
                v.iter().copied().collect()
            }
            None => vec![],
        };
        let sun = V3::from(sun);
        let inner = super::View {
            name,
            image: pixels,
            width,
            height,
            valid: mask,
            position: V3::from(position),
            axes: M3::from_cols(V3::from(axes[0]), V3::from(axes[1]), V3::from(axes[2])).transpose(),
            focal,
            centre: centre.unwrap_or([0.5 * width as f64, 0.5 * height as f64]),
            sun: sun.normalize_or_zero(),
            weight,
        };
        inner.check().map_err(err)?;
        Ok(View { inner })
    }

    #[getter]
    fn name(&self) -> String {
        self.inner.name.clone()
    }

    #[getter]
    fn width(&self) -> usize {
        self.inner.width
    }

    #[getter]
    fn height(&self) -> usize {
        self.inner.height
    }

    /// The camera's centre in the body's frame.
    #[getter]
    fn position(&self) -> [f64; 3] {
        self.inner.position.to_array()
    }

    /// The camera's `x`, `y` and `z` axes in the body's frame, by rows.
    #[getter]
    fn axes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyArray2<f64>>> {
        let m = self.inner.axes;
        Ok(PyArray2::from_vec2(py, &[m.row(0).to_array().to_vec(), m.row(1).to_array().to_vec(), m.row(2).to_array().to_vec()])?)
    }

    #[getter]
    fn focal(&self) -> f64 {
        self.inner.focal
    }

    #[getter]
    fn centre(&self) -> [f64; 2] {
        self.inner.centre
    }

    /// Toward the Sun, unit.
    #[getter]
    fn sun(&self) -> [f64; 3] {
        self.inner.sun.to_array()
    }

    /// Where `points`, `(n, 3)` in the body's frame, fall in the image:
    /// `(n, 3)` of `x`, `y` in pixels and the depth along the boresight;
    /// not a number behind the camera.
    fn project<'py>(&self, py: Python<'py>, points: PyArrayLike2<'py, f64, AllowTypeChange>) -> PyResult<Bound<'py, PyArray2<f64>>> {
        let p = points.as_array();
        if p.shape()[1] != 3 {
            return Err(err("points are (n, 3)".into()));
        }
        let rows: Vec<Vec<f64>> = p
            .rows()
            .into_iter()
            .map(|r| match self.inner.project(V3::new(r[0], r[1], r[2])) {
                Some((x, y, d)) => vec![x, y, d],
                None => vec![f64::NAN; 3],
            })
            .collect();
        if rows.is_empty() {
            return Ok(PyArray2::zeros(py, [0, 3], false));
        }
        Ok(PyArray2::from_vec2(py, &rows)?)
    }

    fn __repr__(&self) -> String {
        format!("View({:?}, {} x {})", self.inner.name, self.inner.width, self.inner.height)
    }
}

/// A square grid of heights and albedo over a patch of a body's surface,
/// along its outward normal `w`: point `(i, j)` at `centre + (i - c) spacing
/// u + (j - c) spacing v + heights[j, i] w`, `c = (size - 1) / 2`, `u x v =
/// w`. Arrays are `(size, size)`, indexed `[j, i]`.
///
/// `Maplet(centre, normal, spacing, heights, albedo=None)` makes one of
/// given heights -- a synthetic terrain; `Maplet.from_surface` one over a
/// shape model, to `solve` from images.
#[pyclass(name = "Maplet", module = "kalast.shape", from_py_object)]
#[derive(Clone)]
pub struct Maplet {
    pub(crate) inner: super::Maplet,
}

fn grid<'py>(py: Python<'py>, values: &[f64], n: usize) -> Bound<'py, PyArray2<f64>> {
    PyArray1::from_slice(py, values).reshape([n, n]).expect("n x n")
}

fn take_grid(values: PyArrayLike2<'_, f64, AllowTypeChange>, n: usize, what: &str) -> PyResult<Vec<f64>> {
    let a = values.as_array();
    if a.shape() != [n, n] {
        return Err(err(format!("{what} is {:?}, the maplet {n} x {n}", a.shape())));
    }
    Ok(a.iter().copied().collect())
}

#[pymethods]
impl Maplet {
    #[new]
    #[pyo3(signature = (centre, normal, spacing, heights, albedo=None))]
    fn new(
        centre: [f64; 3],
        normal: [f64; 3],
        spacing: f64,
        heights: PyArrayLike2<'_, f64, AllowTypeChange>,
        albedo: Option<PyArrayLike2<'_, f64, AllowTypeChange>>,
    ) -> PyResult<Self> {
        let n = heights.as_array().shape()[0];
        let h = take_grid(heights, n, "heights")?;
        let a = match albedo {
            Some(a) => take_grid(a, n, "albedo")?,
            None => vec![1.0; n * n],
        };
        let inner = super::Maplet::new(V3::from(centre), V3::from(normal), n, spacing, h, a).map_err(err)?;
        Ok(Maplet { inner })
    }

    /// The maplet over a surface -- `vertices` `(v, 3)` and `indices` `(f,
    /// 3)` or flat, as `mesh.positions` and `mesh.indices` -- around
    /// `centre`, `size` points on a side `spacing` apart, along `normal`, or
    /// the surface's mean normal there. Each point's height is where a ray
    /// down the normal first meets a facet facing up; that is its
    /// `reference` too, which `solve` holds the long wavelengths to.
    #[staticmethod]
    #[pyo3(signature = (vertices, indices, centre, size, spacing, normal=None))]
    fn from_surface(
        py: Python<'_>,
        vertices: PyArrayLike2<'_, f64, AllowTypeChange>,
        indices: PyReadonlyArrayDyn<'_, u32>,
        centre: [f64; 3],
        size: usize,
        spacing: f64,
        normal: Option<[f64; 3]>,
    ) -> PyResult<Self> {
        let v = vertices.as_array();
        if v.shape()[1] != 3 {
            return Err(err("vertices are (n, 3)".into()));
        }
        let owned;
        let flat: &[f64] = match v.as_slice() {
            Some(s) => s,
            None => {
                owned = v.iter().copied().collect::<Vec<f64>>();
                &owned
            }
        };
        let positions: &[V3] = bytemuck::try_cast_slice(flat).map_err(|e| err(format!("vertices: {e}")))?;
        let idx = indices.as_slice().map_err(|_| err("indices must be contiguous".into()))?;
        let inner = py
            .detach(|| super::Maplet::from_surface(positions, idx, V3::from(centre), normal.map(V3::from), size, spacing))
            .map_err(err)?;
        Ok(Maplet { inner })
    }

    /// The grid's centre at height 0, in the body's frame.
    #[getter]
    fn centre(&self) -> [f64; 3] {
        self.inner.centre.to_array()
    }

    /// `u`, `v`, `w` in the body's frame, by rows.
    #[getter]
    fn axes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyArray2<f64>>> {
        let rows: Vec<Vec<f64>> = self.inner.axes.iter().map(|a| a.to_array().to_vec()).collect();
        Ok(PyArray2::from_vec2(py, &rows)?)
    }

    /// Points on a side.
    #[getter]
    fn size(&self) -> usize {
        self.inner.n
    }

    #[getter]
    fn spacing(&self) -> f64 {
        self.inner.spacing
    }

    /// Along the normal, `(size, size)`; a copy -- assign to change.
    #[getter]
    fn heights<'py>(&self, py: Python<'py>) -> Bound<'py, PyArray2<f64>> {
        grid(py, &self.inner.heights, self.inner.n)
    }

    #[setter]
    fn set_heights(&mut self, values: PyArrayLike2<'_, f64, AllowTypeChange>) -> PyResult<()> {
        self.inner.heights = take_grid(values, self.inner.n, "heights")?;
        Ok(())
    }

    /// Relative albedo, the I/F's factor on the law's: about 1.
    #[getter]
    fn albedo<'py>(&self, py: Python<'py>) -> Bound<'py, PyArray2<f64>> {
        grid(py, &self.inner.albedo, self.inner.n)
    }

    #[setter]
    fn set_albedo(&mut self, values: PyArrayLike2<'_, f64, AllowTypeChange>) -> PyResult<()> {
        self.inner.albedo = take_grid(values, self.inner.n, "albedo")?;
        Ok(())
    }

    /// The shape model's heights, not a number where none was found.
    #[getter]
    fn reference<'py>(&self, py: Python<'py>) -> Bound<'py, PyArray2<f64>> {
        grid(py, &self.inner.reference, self.inner.n)
    }

    #[setter]
    fn set_reference(&mut self, values: PyArrayLike2<'_, f64, AllowTypeChange>) -> PyResult<()> {
        self.inner.reference = take_grid(values, self.inner.n, "reference")?;
        Ok(())
    }

    /// The shape model's facets under the grid.
    #[getter]
    fn facets<'py>(&self, py: Python<'py>) -> Bound<'py, PyArray1<u32>> {
        PyArray1::from_slice(py, &self.inner.facets)
    }

    /// `(size, size, 3)`: every point in the body's frame.
    fn points<'py>(&self, py: Python<'py>) -> Bound<'py, PyArray3<f64>> {
        let n = self.inner.n;
        let flat: Vec<f64> = self.inner.points().iter().flat_map(|p| p.to_array()).collect();
        PyArray1::from_vec(py, flat).reshape([n, n, 3]).expect("n x n x 3")
    }

    /// `(size, size, 3)`: the outward normal at each point.
    fn normals<'py>(&self, py: Python<'py>) -> Bound<'py, PyArray3<f64>> {
        let n = self.inner.n;
        let flat: Vec<f64> = self.inner.normals().iter().flat_map(|p| p.to_array()).collect();
        PyArray1::from_vec(py, flat).reshape([n, n, 3]).expect("n x n x 3")
    }

    /// The grid as a mesh, two facets a cell facing out: `(vertices (v, 3),
    /// indices (f, 3))`, to load or to save.
    fn mesh<'py>(&self, py: Python<'py>) -> (Bound<'py, PyArray2<f64>>, Bound<'py, PyArray2<u32>>) {
        let (points, indices) = self.inner.mesh();
        let flat: Vec<f64> = points.iter().flat_map(|p| p.to_array()).collect();
        let v = PyArray1::from_vec(py, flat).reshape([points.len(), 3]).expect("v x 3");
        let f = indices.len() / 3;
        let i = PyArray1::from_vec(py, indices).reshape([f, 3]).expect("f x 3");
        (v, i)
    }

    /// The same patch at `size` points `spacing` apart.
    fn resampled(&self, size: usize, spacing: f64) -> PyResult<Self> {
        if size < 3 || !(spacing > 0.0) {
            return Err(err(format!("a maplet of {size} points {spacing} apart")));
        }
        Ok(Maplet {
            inner: self.inner.resampled(size, spacing),
        })
    }

    /// The maplet as `view` sees it under `law` (a `Hapke` or a
    /// `LommelSeeligerLambert`): `(height, width)` I/F, each pixel the mean
    /// of `oversample` squared rays, its facets lit as the maplet's own
    /// shadows have them; not a number where a ray meets none of it.
    #[pyo3(signature = (view, law, oversample=3))]
    fn render<'py>(&self, py: Python<'py>, view: PyRef<View>, law: &Bound<'py, PyAny>, oversample: usize) -> PyResult<Bound<'py, PyArray2<f32>>> {
        let law = crate::lightcurve::py::law_of(law)?;
        law.check().map_err(err)?;
        let v = view.inner.clone();
        let image = py.detach(|| self.inner.render(&v, &law, oversample));
        Ok(PyArray1::from_vec(py, image).reshape([v.height, v.width]).expect("h x w"))
    }

    /// The heights and albedo from `views` under `law` (a `Hapke` or a
    /// `LommelSeeligerLambert`), from the heights as they are. Each pass
    /// registers every view to the maplet's shading, then fits the heights
    /// and the albedo to every point each view shows, lit or in shadow: the
    /// shading, the cast shadows (their edges `softness` spacings wide,
    /// their agreement weighted by `shadows`) and the parallax (`stereo`),
    /// by damped Gauss-Newton, the curvature held by `curvature` -- or, with
    /// `direct=False`, each point's slopes and albedo from the views that
    /// see it lit, the heights integrated from the slopes. Either way the
    /// heights keep `reference` above `anchor` (a wavelength in the
    /// surface's unit, 25 spacings by default). `levels` > 1 starts that
    /// many times coarser by halves. A view's first registration looks
    /// `search` pixels away, or `reach` in the surface's unit if farther;
    /// `follow` moves the maplet where the views' pointing puts it. What is
    /// darker than `dark` of its view's lit ground is taken for a shadow the
    /// maplet does not cast. Each view weighs `1 - incidence_weight
    /// cos(i)`, `i` its Sun's incidence on the maplet, as SPC weighs its
    /// images: a high Sun's shading says more of the albedo than of the
    /// slopes. Angles in radians. See
    /// `notes/2026-10-14_shape_from_images/` and
    /// `notes/2026-10-15_shape_direct/`.
    #[pyo3(signature = (
        views, law, iterations=6, levels=1, anchor=0.0, register=true, search=4.0, follow=true, reach=0.0, resolution=2.0,
        incidence=80.0 * super::RPD, emission=75.0 * super::RPD,
        slope_sigma=0.5, albedo_sigma=0.3, offset=false, incidence_weight=0.75, dark=0.1, direct=true, curvature=0.002, softness=0.6, shadows=1.0, stereo=1.0,
        verbose=false
    ))]
    #[allow(clippy::too_many_arguments)]
    fn solve(
        &mut self,
        py: Python<'_>,
        views: Vec<PyRef<View>>,
        law: &Bound<'_, PyAny>,
        iterations: usize,
        levels: usize,
        anchor: f64,
        register: bool,
        search: f64,
        follow: bool,
        reach: f64,
        resolution: f64,
        incidence: f64,
        emission: f64,
        slope_sigma: f64,
        albedo_sigma: f64,
        offset: bool,
        incidence_weight: f64,
        dark: f64,
        direct: bool,
        curvature: f64,
        softness: f64,
        shadows: f64,
        stereo: f64,
        verbose: bool,
    ) -> PyResult<Report> {
        let law = crate::lightcurve::py::law_of(law)?;
        let views: Vec<super::View> = views.iter().map(|v| v.inner.clone()).collect();
        let options = super::Options {
            iterations,
            levels,
            anchor,
            register,
            search,
            follow,
            reach,
            resolution,
            incidence,
            emission,
            slope_sigma,
            albedo_sigma,
            offset,
            incidence_weight,
            dark,
            direct,
            curvature,
            softness,
            shadows,
            stereo,
            verbose,
        };
        let inner = &mut self.inner;
        let report = py.detach(|| inner.solve(&views, &law, &options)).map_err(err)?;
        Ok(Report::from(report))
    }

    fn __repr__(&self) -> String {
        let m = &self.inner;
        format!(
            "Maplet({} x {}, spacing {}, centre [{:.6}, {:.6}, {:.6}])",
            m.n, m.n, m.spacing, m.centre.x, m.centre.y, m.centre.z
        )
    }
}

/// What a view gave, in the last pass.
#[pyclass(name = "ViewReport", module = "kalast.shape", get_all, from_py_object)]
#[derive(Clone)]
pub struct ViewReport {
    /// The view's name.
    pub name: String,
    /// Whether it saw enough of the maplet, lit and fine enough, to be used.
    pub used: bool,
    /// A pixel's size at the maplet's centre, in the surface's unit.
    pub pixel: f64,
    /// Where the maplet's points fall in the image against where its camera
    /// puts them, `(x, y)` in pixels.
    pub shift: [f64; 2],
    /// The image over the maplet's I/F.
    pub scale: f64,
    /// The image's offset, with `offset=True`.
    pub offset: f64,
    /// The maplet's points it saw lit.
    pub points: usize,
    /// The RMS of the image less the maplet's, over the image's mean.
    pub residual: f64,
    /// The correlation of the image with the maplet's shading where it saw
    /// it: as the shape model had it, and after the last pass.
    pub correlation: [f64; 2],
}

#[pymethods]
impl ViewReport {
    fn __repr__(&self) -> String {
        format!(
            "ViewReport({:?}, used={}, pixel={:.3e}, shift=[{:.2}, {:.2}], scale={:.3}, points={}, residual={:.3}, correlation=[{:.3}, {:.3}])",
            self.name,
            if self.used { "True" } else { "False" },
            self.pixel,
            self.shift[0],
            self.shift[1],
            self.scale,
            self.points,
            self.residual,
            self.correlation[0],
            self.correlation[1]
        )
    }
}

/// What `Maplet.solve` did.
#[pyclass(name = "Report", module = "kalast.shape", get_all, from_py_object)]
#[derive(Clone)]
pub struct Report {
    /// One a view, in the order given.
    pub views: Vec<ViewReport>,
    /// Per pass, the images less the maplet's, RMS over their mean: at the
    /// pass's start, the last after the last pass.
    pub residual: Vec<f64>,
    /// Per pass, the RMS change of the heights.
    pub change: Vec<f64>,
    /// Per pass, the spacing it ran at.
    pub spacing: Vec<f64>,
}

impl From<super::Report> for Report {
    fn from(r: super::Report) -> Self {
        Report {
            views: r
                .views
                .into_iter()
                .map(|v| ViewReport {
                    name: v.name,
                    used: v.used,
                    pixel: v.pixel,
                    shift: v.shift,
                    scale: v.scale,
                    offset: v.offset,
                    points: v.points,
                    residual: v.residual,
                    correlation: v.correlation,
                })
                .collect(),
            residual: r.residual,
            change: r.change,
            spacing: r.spacing,
        }
    }
}

#[pymethods]
impl Report {
    fn __repr__(&self) -> String {
        let used = self.views.iter().filter(|v| v.used).count();
        format!(
            "Report({used} of {} views used, residual {:.4} -> {:.4})",
            self.views.len(),
            self.residual.first().copied().unwrap_or(f64::NAN),
            self.residual.last().copied().unwrap_or(f64::NAN)
        )
    }
}
