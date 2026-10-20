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
    /// slopes. A view whose pixel is finer than `triangles` spacings is
    /// compared, where the maplet lights it, triangle by triangle -- each
    /// cell's two, as `mesh` cuts them -- with the mean of the pixels each
    /// covers, rather than smoothed to the points -- its shadows stay with
    /// the points; 0 for none. Angles in radians. See
    /// `notes/2026-10-14_shape_from_images/` and
    /// `notes/2026-10-15_shape_direct/`.
    #[pyo3(signature = (
        views, law, iterations=6, levels=1, anchor=0.0, register=true, search=4.0, follow=true, reach=0.0, resolution=2.0,
        incidence=80.0 * super::RPD, emission=75.0 * super::RPD,
        slope_sigma=0.5, albedo_sigma=0.3, offset=false, incidence_weight=0.75, dark=0.1, direct=true, curvature=0.002, softness=0.6, shadows=1.0, stereo=1.0,
        triangles=0.4, verbose=false
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
        triangles: f64,
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
            triangles,
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

/// A shape model whole -- `vertices` `(v, 3)`, `indices` `(f, 3)` or flat
/// -- and rays against it: what of it an image sees lit, where maplets go
/// to cover it, and what the rest of it hides of a maplet from an image or
/// from the Sun. A reduced model does: what lies beyond a maplet only
/// shadows it, or stands in front of it.
#[pyclass(name = "Surface", module = "kalast.shape")]
pub struct Surface {
    inner: super::Surface,
}

fn vectors(a: PyArrayLike2<'_, f64, AllowTypeChange>, what: &str) -> PyResult<Vec<V3>> {
    let a = a.as_array();
    if a.shape()[1] != 3 {
        return Err(err(format!("{what} are (n, 3)")));
    }
    Ok(a.rows().into_iter().map(|r| V3::new(r[0], r[1], r[2])).collect())
}

fn rows3<'py>(py: Python<'py>, v: &[V3]) -> Bound<'py, PyArray2<f64>> {
    let flat: Vec<f64> = v.iter().flat_map(|p| p.to_array()).collect();
    PyArray1::from_vec(py, flat).reshape([v.len(), 3]).expect("n x 3")
}

#[pymethods]
impl Surface {
    #[new]
    fn new(py: Python<'_>, vertices: PyArrayLike2<'_, f64, AllowTypeChange>, indices: PyReadonlyArrayDyn<'_, u32>) -> PyResult<Self> {
        let positions = vectors(vertices, "vertices")?;
        let idx = indices.as_slice().map_err(|_| err("indices must be contiguous".into()))?.to_vec();
        let inner = py.detach(|| super::Surface::new(positions, idx)).map_err(err)?;
        Ok(Surface { inner })
    }

    /// The surface in cubes `size` on a side (its unit): `(points,
    /// normals, areas)`, a point on the surface a cube -- two where it
    /// crosses the cube twice -- the facets' mean normal there, and their
    /// area.
    fn samples<'py>(&self, py: Python<'py>, size: f64) -> (Bound<'py, PyArray2<f64>>, Bound<'py, PyArray2<f64>>, Bound<'py, PyArray1<f64>>) {
        let s = py.detach(|| self.inner.samples(size));
        (rows3(py, &s.points), rows3(py, &s.normals), PyArray1::from_vec(py, s.areas))
    }

    /// What a camera sees of `points` `(n, 3)` on the surface, facing along
    /// `normals`: the camera as `View` takes it, without the image -- its
    /// `size` `(width, height)` in pixels. `(flags, scale, incidence,
    /// emission)`: bits 1 in the frame, 2 facing the camera, 4 nothing
    /// between them, 8 lit -- the last two only for a point in the frame and
    /// facing it; a pixel's size there; the Sun's and the camera's angles
    /// to the normal, radians.
    #[pyo3(signature = (points, normals, position, axes, focal, size, sun, centre=None))]
    #[allow(clippy::too_many_arguments)]
    fn sees<'py>(
        &self,
        py: Python<'py>,
        points: PyArrayLike2<'_, f64, AllowTypeChange>,
        normals: PyArrayLike2<'_, f64, AllowTypeChange>,
        position: [f64; 3],
        axes: [[f64; 3]; 3],
        focal: f64,
        size: (usize, usize),
        sun: [f64; 3],
        centre: Option<[f64; 2]>,
    ) -> PyResult<(Bound<'py, PyArray1<u8>>, Bound<'py, PyArray1<f64>>, Bound<'py, PyArray1<f64>>, Bound<'py, PyArray1<f64>>)> {
        let (p, n) = (vectors(points, "points")?, vectors(normals, "normals")?);
        if p.len() != n.len() {
            return Err(err(format!("{} points, {} normals", p.len(), n.len())));
        }
        let camera = super::View {
            name: String::new(),
            image: vec![],
            width: size.0,
            height: size.1,
            valid: vec![],
            position: V3::from(position),
            axes: M3::from_cols(V3::from(axes[0]), V3::from(axes[1]), V3::from(axes[2])).transpose(),
            focal,
            centre: centre.unwrap_or([0.5 * size.0 as f64, 0.5 * size.1 as f64]),
            sun: V3::from(sun).normalize_or_zero(),
            weight: 1.0,
        };
        let seen = py.detach(|| self.inner.sees(&p, &n, &camera));
        Ok((
            PyArray1::from_iter(py, seen.iter().map(|s| s.flags)),
            PyArray1::from_iter(py, seen.iter().map(|s| s.scale)),
            PyArray1::from_iter(py, seen.iter().map(|s| s.incidence)),
            PyArray1::from_iter(py, seen.iter().map(|s| s.emission)),
        ))
    }

    /// The pixels of `view` that show `maplet` as its own grid can tell it,
    /// `(height, width)`: false where the rest of the surface hides it or
    /// shadows it, and `margin` (the surface's unit) around, for the shape
    /// model's error; true elsewhere. For `View`'s `valid`, with the
    /// image's own.
    #[pyo3(signature = (maplet, view, margin=0.0))]
    fn mask<'py>(&self, py: Python<'py>, maplet: PyRef<Maplet>, view: PyRef<View>, margin: f64) -> Bound<'py, PyArray2<bool>> {
        let (m, v) = (&maplet.inner, &view.inner);
        let valid = py.detach(|| self.inner.mask(m, v, margin));
        PyArray1::from_iter(py, valid.iter().map(|&b| b != 0)).reshape([v.height, v.width]).expect("image")
    }

    /// What `view`'s camera sees of the surface, `(height, width)`: 0
    /// nothing, 1 the surface unlit -- facing away from the Sun or in a
    /// shadow -- 2 lit; a ray through each pixel's middle, the view's image
    /// unused. To register an image on the surface by `align`.
    fn draw<'py>(&self, py: Python<'py>, view: PyRef<View>) -> Bound<'py, PyArray2<u8>> {
        let v = &view.inner;
        let drawn = py.detach(|| self.inner.draw(v));
        PyArray1::from_vec(py, drawn).reshape([v.height, v.width]).expect("image")
    }

    /// Maplets of `size` points on a side over `samples` `(points, normals,
    /// areas)` of the surface, each at the finest spacing its images allow
    /// -- `finest`, a sample's, not a number where none does -- overlapping
    /// by `overlap` of a maplet's width: `(m, 7)`, each a maplet's centre
    /// on the surface, its normal and its spacing. Finest first, the
    /// maplets of a spacing laid as a lattice grows over the surface, until
    /// each sample of that spacing or finer is on one -- facing within 60
    /// degrees of its normal, not under an overhang -- and where at least
    /// half of what a maplet covers allows its spacing. For
    /// `Maplet.from_surface`.
    #[staticmethod]
    #[pyo3(signature = (points, normals, areas, finest, size=241, overlap=0.2))]
    fn tiles<'py>(
        py: Python<'py>,
        points: PyArrayLike2<'_, f64, AllowTypeChange>,
        normals: PyArrayLike2<'_, f64, AllowTypeChange>,
        areas: numpy::PyArrayLike1<'_, f64, AllowTypeChange>,
        finest: numpy::PyArrayLike1<'_, f64, AllowTypeChange>,
        size: usize,
        overlap: f64,
    ) -> PyResult<Bound<'py, PyArray2<f64>>> {
        let samples = super::Samples {
            points: vectors(points, "points")?,
            normals: vectors(normals, "normals")?,
            areas: areas.as_array().iter().copied().collect(),
        };
        let finest: Vec<f64> = finest.as_array().iter().copied().collect();
        let tiles = py.detach(|| super::plan(&samples, &finest, size, overlap)).map_err(err)?;
        let flat: Vec<f64> = tiles.iter().flat_map(|t| [t.centre.x, t.centre.y, t.centre.z, t.normal.x, t.normal.y, t.normal.z, t.spacing]).collect();
        Ok(PyArray1::from_vec(py, flat).reshape([tiles.len(), 7]).expect("m x 7"))
    }

    fn __repr__(&self) -> String {
        format!("Surface({} vertices, {} facets)", self.inner.positions.len(), self.inner.indices.len() / 3)
    }
}

/// A body's visual hull: its silhouettes in images that show it against
/// the sky, carved into a closed mesh -- the start of a shape model made
/// from images alone.
///
/// `radius`, in the surface's unit: a sphere about the frame's origin the
/// body is inside -- nothing beyond it is the body, which keeps stars and
/// a coma's glow out. `add` each image as a `View`; `mesh` carves them.
#[pyclass(name = "Hull", module = "kalast.shape")]
pub struct Hull {
    inner: super::Hull,
}

#[pymethods]
impl Hull {
    #[new]
    fn new(radius: f64) -> PyResult<Self> {
        if !(radius > 0.0 && radius.is_finite()) {
            return Err(err(format!("a radius of {radius}")));
        }
        Ok(Hull { inner: super::Hull::new(radius) })
    }

    #[getter]
    fn radius(&self) -> f64 {
        self.inner.radius
    }

    /// The views' names, in the order added.
    #[getter]
    fn names(&self) -> Vec<String> {
        self.inner.silhouettes.iter().map(|s| s.name.clone()).collect()
    }

    /// Where each view's boresight falls, `(n, 2)` pixels, as `shift`
    /// left it.
    #[getter]
    fn centres<'py>(&self, py: Python<'py>) -> Bound<'py, PyArray2<f64>> {
        let flat: Vec<f64> = self.inner.silhouettes.iter().flat_map(|s| s.centre).collect();
        PyArray1::from_vec(py, flat).reshape([self.inner.silhouettes.len(), 2]).expect("n x 2")
    }

    /// `view`'s silhouette added: what the hull takes of each pixel,
    /// `(height, width)` -- 1 the body, or what may be: lit (brighter than
    /// `threshold` of the image's lit ground, its 99.5th percentile), or
    /// behind lit ground seen from the Sun, the night side and the shadows;
    /// -1 the sky; 0 not valid in the view. The body is within the bounding
    /// sphere, or with `within`, `(vertices, facets)`, within that closed
    /// mesh -- an earlier hull -- widened by `margin`, the surface's unit:
    /// what lets a view that does not hold the whole body carve where the
    /// sphere runs past its frame.
    #[pyo3(signature = (view, threshold=0.1, within=None, margin=0.05))]
    fn add<'py>(
        &mut self,
        py: Python<'py>,
        view: PyRef<View>,
        threshold: f64,
        within: Option<(PyArrayLike2<'_, f64, AllowTypeChange>, PyReadonlyArrayDyn<'_, u32>)>,
        margin: f64,
    ) -> PyResult<Bound<'py, PyArray2<i8>>> {
        let v = &view.inner;
        let bound = match within {
            Some((vertices, facets)) => {
                let vertices = vectors(vertices, "within's vertices")?;
                let facets: Vec<u32> = facets.as_array().iter().copied().collect();
                if facets.len() % 3 != 0 || facets.iter().any(|&i| i as usize >= vertices.len()) {
                    return Err(err("within's facets come in threes, each a vertex".into()));
                }
                Some((vertices, facets))
            }
            None => None,
        };
        let hull = &mut self.inner;
        let classes = py
            .detach(|| hull.add(v, threshold, bound.as_ref().map(|(p, f)| (p.as_slice(), f.as_slice(), margin))))
            .map_err(err)?;
        Ok(PyArray1::from_vec(py, classes).reshape([v.height, v.width]).expect("image"))
    }

    /// `add` for each of `views`, over the machine's cores: their classes,
    /// in order.
    #[pyo3(signature = (views, threshold=0.1, within=None, margin=0.05))]
    fn add_many<'py>(
        &mut self,
        py: Python<'py>,
        views: Vec<PyRef<'py, View>>,
        threshold: f64,
        within: Option<(PyArrayLike2<'_, f64, AllowTypeChange>, PyReadonlyArrayDyn<'_, u32>)>,
        margin: f64,
    ) -> PyResult<Vec<Bound<'py, PyArray2<i8>>>> {
        let bound = match within {
            Some((vertices, facets)) => {
                let vertices = vectors(vertices, "within's vertices")?;
                let facets: Vec<u32> = facets.as_array().iter().copied().collect();
                if facets.len() % 3 != 0 || facets.iter().any(|&i| i as usize >= vertices.len()) {
                    return Err(err("within's facets come in threes, each a vertex".into()));
                }
                Some((vertices, facets))
            }
            None => None,
        };
        let inner: Vec<&super::View> = views.iter().map(|v| &v.inner).collect();
        let sizes: Vec<(usize, usize)> = inner.iter().map(|v| (v.height, v.width)).collect();
        let hull = &mut self.inner;
        let classes = py
            .detach(|| hull.add_many(&inner, threshold, bound.as_ref().map(|(p, f)| (p.as_slice(), f.as_slice(), margin))))
            .map_err(err)?;
        Ok(classes.into_iter().zip(sizes).map(|(c, (h, w))| PyArray1::from_vec(py, c).reshape([h, w]).expect("image")).collect())
    }

    /// View `index`'s pointing moved: `(dx, dy)` pixels added to where its
    /// boresight falls, as `align` finds them.
    fn shift(&mut self, index: usize, dx: f64, dy: f64) -> PyResult<()> {
        let n = self.inner.silhouettes.len();
        let s = self.inner.silhouettes.get_mut(index).ok_or_else(|| err(format!("view {index} of {n}")))?;
        s.centre[0] += dx;
        s.centre[1] += dy;
        Ok(())
    }

    /// The hull's signed distance at `points` `(n, 3)`, in the surface's
    /// unit, positive inside: of each view's distance to its silhouette's
    /// edge at the point's projection, the `rank`-th least (1 to 8).
    #[pyo3(signature = (points, rank=2))]
    fn value<'py>(&self, py: Python<'py>, points: PyArrayLike2<'_, f64, AllowTypeChange>, rank: usize) -> PyResult<Bound<'py, PyArray1<f64>>> {
        let points = vectors(points, "points")?;
        let hull = &self.inner;
        let chunk = 4096;
        let values = py.detach(|| super::par_map(points.len().div_ceil(chunk), |c| hull.values(&points[c * chunk..((c + 1) * chunk).min(points.len())], rank)).concat());
        Ok(PyArray1::from_vec(py, values))
    }

    /// Each view's own signed distance at `points` `(n, 3)`, `(views, n)`:
    /// which views carve where -- NaN behind a view's camera.
    fn distances<'py>(&self, py: Python<'py>, points: PyArrayLike2<'_, f64, AllowTypeChange>) -> PyResult<Bound<'py, PyArray2<f64>>> {
        let points = vectors(points, "points")?;
        let hull = &self.inner;
        let rows = py.detach(|| super::par_map(hull.silhouettes.len(), |k| points.iter().map(|p| hull.silhouettes[k].at(*p).unwrap_or(f64::NAN)).collect::<Vec<f64>>()));
        let n = points.len();
        Ok(PyArray1::from_vec(py, rows.concat()).reshape([hull.silhouettes.len(), n]).expect("views x n"))
    }

    /// The hull as a closed mesh, `(vertices (v, 3), facets (f, 3), report)`:
    /// cubes of `size` over the bounding sphere, the surface where `value`
    /// crosses zero by marching tetrahedra, its largest piece kept; then
    /// `relax` rounds of each vertex moved toward its neighbours' middle
    /// across the surface. `rank` 2: one view wrong carves nothing alone.
    /// `smooth`, the surface's unit, a Gaussian the distance is smoothed by
    /// first: what is thinner -- a tunnel small errors cut, a fin -- closes,
    /// for a shape of one piece and no handle. `report`: `cubes` the
    /// grid's, `visited` those evaluated, `dropped` the facets of the other
    /// pieces, `genus` (0 for a body with no hole through it), `volume`.
    #[pyo3(signature = (size, rank=2, relax=3, smooth=0.0))]
    #[allow(clippy::type_complexity)]
    fn mesh<'py>(&self, py: Python<'py>, size: f64, rank: usize, relax: usize, smooth: f64) -> PyResult<(Bound<'py, PyArray2<f64>>, Bound<'py, PyArray2<u32>>, Bound<'py, pyo3::types::PyDict>)> {
        let hull = &self.inner;
        let (v, f, r) = py.detach(|| hull.mesh(size, rank, relax, smooth)).map_err(err)?;
        let report = pyo3::types::PyDict::new(py);
        report.set_item("cubes", r.cubes)?;
        report.set_item("visited", r.visited)?;
        report.set_item("dropped", r.dropped)?;
        report.set_item("genus", r.genus)?;
        report.set_item("volume", r.volume)?;
        let faces = f.len() / 3;
        Ok((rows3(py, &v), PyArray1::from_vec(py, f).reshape([faces, 3]).expect("f x 3"), report))
    }

    fn __len__(&self) -> usize {
        self.inner.silhouettes.len()
    }

    fn __repr__(&self) -> String {
        format!("Hull({} views, radius {})", self.inner.silhouettes.len(), self.inner.radius)
    }
}

/// The shift that best lays mask `b` on the signs `a`, both `(height,
/// width)`: `((dx, dy), score)`, `b` moved by `(dx, dy)` pixels holding most
/// where `a` is 1 and least where it is -1 -- 0 for no matter -- and that
/// as a share of `a`'s nonzero pixels. For a surface's outline drawn in an
/// image's geometry (`Surface.draw(view) > 0`) against the image -- 1 where
/// it shows the body lit, -1 on the sky -- what to add to the camera's
/// centre. Whole pixels within `search`, then to half a pixel.
#[pyfunction]
#[pyo3(signature = (a, b, search=16))]
pub fn align(py: Python<'_>, a: PyArrayLike2<'_, i8, AllowTypeChange>, b: PyArrayLike2<'_, bool, AllowTypeChange>, search: usize) -> PyResult<((f64, f64), f64)> {
    let (a, b) = (a.as_array(), b.as_array());
    if a.shape() != b.shape() {
        return Err(err(format!("masks of {:?} and {:?}", a.shape(), b.shape())));
    }
    let (h, w) = (a.shape()[0], a.shape()[1]);
    let (a, b): (Vec<i8>, Vec<bool>) = (a.iter().copied().collect(), b.iter().copied().collect());
    let ([dx, dy], score) = py.detach(|| super::align(&a, &b, w, h, search));
    Ok(((dx, dy), score))
}

/// The shape model `vertices` `(v, 3)`, `indices` `(f, 3)` or flat, with
/// `maplets` in it, one mesh: `(vertices, indices, albedo)`. A point of the
/// surface is the finest maplet's over it, and among those the one it is
/// most in the middle of; each maplet keeps the cells that are its own and
/// one more all round, so that two meet without a crack, and the shape
/// model loses each facet whose corners are all a maplet's. The shape
/// model's vertices come first; the albedo is 1 on them and each maplet's
/// own on its. `spacing`, if coarser than a maplet's, draws it at that: for
/// an image a few times coarser, which sees no more of it. Only a maplet's
/// cells over the shape model are kept, where its `reference` is known.
#[pyfunction]
#[pyo3(signature = (vertices, indices, maplets, spacing=0.0))]
pub fn compose<'py>(
    py: Python<'py>,
    vertices: PyArrayLike2<'_, f64, AllowTypeChange>,
    indices: PyReadonlyArrayDyn<'_, u32>,
    maplets: Vec<PyRef<'py, Maplet>>,
    spacing: f64,
) -> PyResult<(Bound<'py, PyArray2<f64>>, Bound<'py, PyArray2<u32>>, Bound<'py, PyArray1<f64>>)> {
    let positions = vectors(vertices, "vertices")?;
    let idx = indices.as_slice().map_err(|_| err("indices must be contiguous".into()))?;
    if idx.len() % 3 != 0 || idx.iter().any(|&i| i as usize >= positions.len()) {
        return Err(err("indices come in threes, each a vertex".into()));
    }
    let inner: Vec<super::Maplet> = maplets.iter().map(|m| m.inner.clone()).collect();
    let (v, f, a) = py.detach(|| super::compose(&positions, idx, &inner, spacing));
    let faces = f.len() / 3;
    Ok((rows3(py, &v), PyArray1::from_vec(py, f).reshape([faces, 3]).expect("f x 3"), PyArray1::from_vec(py, a)))
}

/// The maplets' relief laid into the shape model itself, `vertices` `(v,
/// 3)`, `indices` `(f, 3)` or flat: each vertex moved along the normal of
/// each maplet over it to the maplet's surface there, the maplets weighed
/// toward their middles; a vertex facing away from a maplet, or far off its
/// surface, not its. The facets are kept, and the mesh as closed as it was:
/// what the next round of maplets is laid on. Then `relax` rounds of each
/// vertex moved halfway toward its neighbours' middle across the surface.
/// Gives `(vertices (v, 3), moved (v,) along each vertex's normal, outward
/// positive, cover (v,) the maplets' summed weight, 0 where none is)`.
#[pyfunction]
#[pyo3(signature = (vertices, indices, maplets, relax=2))]
#[allow(clippy::type_complexity)]
pub fn fuse<'py>(
    py: Python<'py>,
    vertices: PyArrayLike2<'_, f64, AllowTypeChange>,
    indices: PyReadonlyArrayDyn<'_, u32>,
    maplets: Vec<PyRef<'py, Maplet>>,
    relax: usize,
) -> PyResult<(Bound<'py, PyArray2<f64>>, Bound<'py, PyArray1<f64>>, Bound<'py, PyArray1<f64>>)> {
    let positions = vectors(vertices, "vertices")?;
    let idx = indices.as_slice().map_err(|_| err("indices must be contiguous".into()))?;
    if idx.len() % 3 != 0 || idx.iter().any(|&i| i as usize >= positions.len()) {
        return Err(err("indices come in threes, each a vertex".into()));
    }
    let inner: Vec<super::Maplet> = maplets.iter().map(|m| m.inner.clone()).collect();
    let (v, moved, cover) = py.detach(|| super::fuse(&positions, idx, &inner, relax));
    Ok((rows3(py, &v), PyArray1::from_vec(py, moved), PyArray1::from_vec(py, cover)))
}

/// The images a maplet takes of its candidates, as MPCD keeps them: finest
/// first, each only if it differs from every one taken by `angle` (radians)
/// of Sun or of view, or by `ratio` of scale, or if most of the maplet's
/// ground it sees -- `seen`, a list of sorted sample indices an image --
/// the one it repeats does not; at most `cap`. `scales` `(n,)` a pixel's
/// size at the maplet, `suns` and `views` `(n, 3)` toward the Sun and the
/// camera. Indices into the candidates, in the order taken.
#[pyfunction]
#[pyo3(signature = (scales, suns, views, seen=None, cap=70, angle=10.0 * super::RPD, ratio=1.5))]
#[allow(clippy::too_many_arguments)]
pub fn choose(
    scales: numpy::PyArrayLike1<'_, f64, AllowTypeChange>,
    suns: PyArrayLike2<'_, f64, AllowTypeChange>,
    views: PyArrayLike2<'_, f64, AllowTypeChange>,
    seen: Option<Vec<Vec<u32>>>,
    cap: usize,
    angle: f64,
    ratio: f64,
) -> PyResult<Vec<usize>> {
    let scales: Vec<f64> = scales.as_array().iter().copied().collect();
    let (suns, views) = (vectors(suns, "suns")?, vectors(views, "views")?);
    if suns.len() != scales.len() || views.len() != scales.len() || seen.as_ref().is_some_and(|s| s.len() != scales.len()) {
        return Err(err(format!("{} scales, {} suns, {} views", scales.len(), suns.len(), views.len())));
    }
    let unit = |v: Vec<V3>| -> Vec<V3> { v.into_iter().map(|x| x.normalize_or_zero()).collect() };
    let seen = seen.map(|s| s.into_iter().map(|mut x| { x.sort_unstable(); x }).collect::<Vec<_>>());
    Ok(super::choose(&scales, &unit(suns), &unit(views), seen.as_deref(), cap, angle, ratio))
}

/// Each image's pointing, from its registrations in many maplets: every
/// maplet's shift in every image fitted at once as the image's pointing and
/// the maplet's offset on the ground (`s = P + J o`), as SPC fixes an
/// image's pointing from all the surface. Cameras: `positions` `(n, 3)`,
/// `axes` `(n, 3, 3)` by rows as a `View`'s, `focal` a number or `(n,)`.
/// Maplets: `centres` and `normals` `(m, 3)`. Registrations: `image`,
/// `tile` `(p,)` indices into those, `shift` `(p, 2)` as a `ViewReport`'s,
/// `weight` `(p,)`. `offset_sigma`, in the surface's unit, how far a
/// maplet may move for one pixel of residual; `huber`, pixels, past which
/// a residual weighs as its square root. `height_sigma`, if positive, frees
/// each maplet's height too -- the parallax across its images tells it,
/// where the surface under the maplets is a first guess -- held so to
/// zero. Gives `(pointing (n, 2) pixels to add to each camera's centre,
/// behind (n,) registrations each image's rests on, offsets (m, 2) -- (m,
/// 3) with the heights, along each maplet's u, v and w -- residuals (p,))`.
#[pyfunction]
#[pyo3(signature = (positions, axes, focal, centres, normals, image, tile, shift, weight, offset_sigma, huber=3.0, iterations=25, height_sigma=0.0))]
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn pointing<'py>(
    py: Python<'py>,
    positions: PyArrayLike2<'_, f64, AllowTypeChange>,
    axes: numpy::PyArrayLike3<'_, f64, AllowTypeChange>,
    focal: &Bound<'py, PyAny>,
    centres: PyArrayLike2<'_, f64, AllowTypeChange>,
    normals: PyArrayLike2<'_, f64, AllowTypeChange>,
    image: numpy::PyArrayLike1<'_, i64, AllowTypeChange>,
    tile: numpy::PyArrayLike1<'_, i64, AllowTypeChange>,
    shift: PyArrayLike2<'_, f64, AllowTypeChange>,
    weight: numpy::PyArrayLike1<'_, f64, AllowTypeChange>,
    offset_sigma: f64,
    huber: f64,
    iterations: usize,
    height_sigma: f64,
) -> PyResult<(Bound<'py, PyArray2<f64>>, Bound<'py, PyArray1<i64>>, Bound<'py, PyArray2<f64>>, Bound<'py, PyArray1<f64>>)> {
    let positions = vectors(positions, "positions")?;
    let axes = axes.as_array();
    if axes.shape() != [positions.len(), 3, 3] {
        return Err(err(format!("axes are {:?} for {} cameras: (n, 3, 3)", axes.shape(), positions.len())));
    }
    let focal: Vec<f64> = match focal.extract::<f64>() {
        Ok(f) => vec![f; positions.len()],
        Err(_) => focal.extract::<Vec<f64>>().map_err(|_| err("focal is a number or (n,)".into()))?,
    };
    if focal.len() != positions.len() {
        return Err(err(format!("{} focal lengths for {} cameras", focal.len(), positions.len())));
    }
    let cameras: Vec<super::Camera> = (0..positions.len())
        .map(|k| {
            let row = |r: usize| V3::new(axes[[k, r, 0]], axes[[k, r, 1]], axes[[k, r, 2]]);
            super::Camera { position: positions[k], axes: M3::from_cols(row(0), row(1), row(2)).transpose(), focal: focal[k] }
        })
        .collect();
    let (centres, normals) = (vectors(centres, "centres")?, vectors(normals, "normals")?);
    if centres.len() != normals.len() {
        return Err(err(format!("{} centres, {} normals", centres.len(), normals.len())));
    }
    let tiles: Vec<(V3, V3)> = centres.into_iter().zip(normals).collect();
    let (image, tile, weight) = (image.as_array(), tile.as_array(), weight.as_array());
    let shift = shift.as_array();
    let p = image.len();
    if tile.len() != p || weight.len() != p || shift.shape() != [p, 2] {
        return Err(err(format!("{p} images, {} tiles, {} weights, shifts {:?}: all (p,), shifts (p, 2)", tile.len(), weight.len(), shift.shape())));
    }
    let mut registrations = Vec::with_capacity(p);
    for i in 0..p {
        let (k, t) = (image[i], tile[i]);
        if k < 0 || t < 0 {
            return Err(err(format!("registration {i}: image {k}, tile {t}")));
        }
        registrations.push(super::Registration { image: k as usize, tile: t as usize, shift: [shift[[i, 0]], shift[[i, 1]]], weight: weight[i] });
    }
    let fit = py
        .detach(|| super::pointing(&cameras, &tiles, &registrations, offset_sigma, huber, iterations, height_sigma))
        .map_err(err)?;
    let two = |v: &[[f64; 2]]| PyArray1::from_vec(py, v.iter().flatten().copied().collect()).reshape([v.len(), 2]).expect("n x 2");
    let columns = if height_sigma > 0.0 { 3 } else { 2 };
    let offsets = PyArray1::from_vec(py, fit.offsets.iter().flat_map(|o| o[..columns].to_vec()).collect())
        .reshape([fit.offsets.len(), columns])
        .expect("m x 2 or 3");
    Ok((two(&fit.images), PyArray1::from_vec(py, fit.behind.iter().map(|&b| b as i64).collect()), offsets, PyArray1::from_vec(py, fit.residuals)))
}
