//! What surfaces send to one another: scattered sunlight and thermal radiation, with their reflections.
//!
//! A facet in a concavity, or facing another body, receives beside the direct sunlight what the facets it sees
//! scatter and emit. With `F_ij` the view factor from facet `i` to facet `j` (the share of what leaves `i` that
//! reaches `j`; reciprocity makes it also the weight of `j` in what `i` receives), and grey surfaces that scatter
//! and emit without a preferred direction, the radiation leaving each facet, its radiosity, obeys
//!
//! ```text
//! visible:  J_i = A_i (S_i + sum_j F_ij J_j)                       S the sunlight falling on it (W/m2)
//! thermal:  J_i = e_i sigma T_i^4 + (1 - e_i) sum_j F_ij J_j       1 - e what it reflects, by Kirchhoff's law
//! ```
//!
//! and the facet absorbs `(1 - A_i) sum_j F_ij J_j` of the first and `e_i sum_j F_ij J_j` of the second. These are
//! the equations of Lagerros (1998, his 3 to 6) and of Rozitis & Green (2011, their 18 to 20) on the facets of a
//! mesh. The sums are taken over every facet of every body, so that what one body scatters to the other and back
//! is in them: the cross-body terms need no treatment of their own.
//!
//! The radiosity is found by substituting it into itself (`J = source`, then `J = source + r F J`, ...), each pass
//! one more reflection. The series converges by `r` times the largest row sum each time: at once on a convex body,
//! in a few passes in a crater, never for a closed cavity that absorbs nothing.
//!
//! The view factors are kept sparse: for two bodies of 10,000 facets, 0.3 % of the pairs see each other.

use anyhow::{Result, bail};
use ndarray::ArrayView2;

use super::view_factor::{self, Triangle};
use crate::Float;
use crate::util::STEFAN_BOLTZMANN;

/// View factors between facets, the rows the facets that receive, kept as compressed sparse rows.
#[derive(Clone, Debug, PartialEq)]
pub struct ViewFactors {
    columns: usize,
    start: Vec<usize>,
    column: Vec<u32>,
    value: Vec<Float>,
}

impl ViewFactors {
    /// From compressed sparse rows: row `i` holds `value[start[i]..start[i + 1]]` in the columns
    /// `column[start[i]..start[i + 1]]`.
    pub fn from_csr(columns: usize, start: Vec<usize>, column: Vec<u32>, value: Vec<Float>) -> Result<Self> {
        if start.is_empty() || start[0] != 0 || *start.last().unwrap() != value.len() || column.len() != value.len() {
            bail!(
                "compressed sparse rows: {} row starts (the first 0, the last {}) for {} columns and {} values",
                start.len(),
                start.last().copied().unwrap_or(0),
                column.len(),
                value.len()
            );
        }
        if start.windows(2).any(|w| w[1] < w[0]) {
            bail!("compressed sparse rows: the row starts go down");
        }
        if let Some(c) = column.iter().find(|&&c| c as usize >= columns) {
            bail!("a view factor in column {c} of {columns}");
        }
        if let Some(v) = value.iter().find(|v| !(v.is_finite() && **v >= 0.0)) {
            bail!("a view factor of {v}: they are shares, zero or more");
        }
        Ok(Self { columns, start, column, value })
    }

    /// From a full matrix, leaving out what is at or under `threshold`.
    pub fn from_dense(matrix: ArrayView2<Float>, threshold: Float) -> Result<Self> {
        let (mut start, mut column, mut value) = (vec![0], Vec::new(), Vec::new());
        for row in matrix.rows() {
            for (j, &v) in row.iter().enumerate() {
                if v > threshold {
                    column.push(j as u32);
                    value.push(v);
                }
            }
            start.push(value.len());
        }
        Self::from_csr(matrix.ncols(), start, column, value)
    }

    /// Between every pair of triangles, exactly, with nothing looked for between them: right for surfaces that
    /// all see each other (the inside of a convex cavity), and the reference a hemicube is checked against.
    pub fn exact(triangles: &[Triangle]) -> Self {
        let (mut start, mut column, mut value) = (vec![0], Vec::new(), Vec::new());
        for (i, a) in triangles.iter().enumerate() {
            for (j, b) in triangles.iter().enumerate() {
                let f = if i == j { 0.0 } else { view_factor::exact(a, b) };
                if f > 0.0 {
                    column.push(j as u32);
                    value.push(f);
                }
            }
            start.push(value.len());
        }
        Self { columns: triangles.len(), start, column, value }
    }

    pub fn rows(&self) -> usize {
        self.start.len() - 1
    }

    pub fn columns(&self) -> usize {
        self.columns
    }

    pub fn nonzeros(&self) -> usize {
        self.value.len()
    }

    /// The row starts, the columns and the values.
    pub fn csr(&self) -> (&[usize], &[u32], &[Float]) {
        (&self.start, &self.column, &self.value)
    }

    /// The share of what each facet sends that reaches another facet; the rest goes to space.
    pub fn row_sums(&self) -> Vec<Float> {
        self.start.windows(2).map(|w| self.value[w[0]..w[1]].iter().map(|&v| v as f64).sum::<f64>() as Float).collect()
    }

    /// `sum_j F_ij x_j` for every row.
    pub fn dot(&self, x: &[Float]) -> Result<Vec<Float>> {
        if x.len() != self.columns {
            bail!("{} values for {} columns", x.len(), self.columns);
        }
        let wide: Vec<f64> = x.iter().map(|&v| v as f64).collect();
        Ok(self.product(&wide).into_iter().map(|v| v as Float).collect())
    }

    fn product(&self, x: &[f64]) -> Vec<f64> {
        self.start
            .windows(2)
            .map(|w| (w[0]..w[1]).map(|k| self.value[k] as f64 * x[self.column[k] as usize]).sum())
            .collect()
    }
}

/// How far the reflections are followed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounces {
    /// 1: what the other facets send directly. `n`: with `n - 1` reflections more. 0: until a pass changes no
    /// radiosity by more than `tolerance` times the largest source.
    pub most: usize,
    pub tolerance: Float,
}

impl Bounces {
    /// The passes after which a series that has not settled is given up.
    pub const LIMIT: usize = 10_000;
}

impl Default for Bounces {
    fn default() -> Self {
        Self { most: 0, tolerance: 1.0e-6 }
    }
}

/// What every facet absorbs from the others, in W/m2, and the passes each series took.
#[derive(Clone, Debug, PartialEq)]
pub struct Absorbed {
    /// Sunlight scattered by the other facets: `(1 - A_i) sum_j F_ij J_j`.
    pub scattered: Vec<Float>,
    /// Their thermal radiation: `e_i sum_j F_ij J_j`.
    pub thermal: Vec<Float>,
    pub scattered_bounces: usize,
    pub thermal_bounces: usize,
}

/// What every facet absorbs from the others.
///
/// `factors` has a row and a column for each facet of every body. `incident` is the sunlight falling on each facet
/// (W/m2, before its albedo, with the shadows in it); `temperature` each facet's surface temperature (K). Either
/// may be left out, and its part is then zero.
pub fn absorbed(
    factors: &ViewFactors,
    albedo: &[Float],
    emissivity: &[Float],
    incident: Option<&[Float]>,
    temperature: Option<&[Float]>,
    bounces: Bounces,
) -> Result<Absorbed> {
    let n = factors.rows();
    check(factors, albedo, emissivity, incident, temperature, bounces)?;

    let part = |source: Option<Vec<f64>>, reflects: Vec<f64>, absorbs: Vec<f64>, name: &str| -> Result<(Vec<Float>, usize)> {
        let Some(source) = source else { return Ok((vec![0.0; n], 0)) };
        let (radiosity, passes) = radiosity(factors, &source, &reflects, bounces, name)?;
        let arriving = factors.product(&radiosity);
        Ok((arriving.iter().zip(&absorbs).map(|(e, a)| (e * a) as Float).collect(), passes))
    };
    let wide = |v: &[Float]| v.iter().map(|&x| x as f64).collect::<Vec<f64>>();
    let (a, e) = (wide(albedo), wide(emissivity));
    let (scattered, scattered_bounces) = part(
        incident.map(|s| s.iter().zip(&a).map(|(&s, a)| a * s as f64).collect()),
        a.clone(),
        a.iter().map(|a| 1.0 - a).collect(),
        "scattered sunlight",
    )?;
    let (thermal, thermal_bounces) = part(
        temperature.map(|t| t.iter().zip(&e).map(|(&t, e)| e * STEFAN_BOLTZMANN as f64 * (t as f64).powi(4)).collect()),
        e.iter().map(|e| 1.0 - e).collect(),
        e.clone(),
        "thermal radiation",
    )?;
    Ok(Absorbed { scattered, thermal, scattered_bounces, thermal_bounces })
}

/// What `absorbed` refuses, on the CPU and on the GPU alike.
pub(super) fn check(
    factors: &ViewFactors,
    albedo: &[Float],
    emissivity: &[Float],
    incident: Option<&[Float]>,
    temperature: Option<&[Float]>,
    bounces: Bounces,
) -> Result<()> {
    let n = factors.rows();
    if factors.columns() != n {
        bail!("{n} rows of view factors for {} columns: the exchange takes a row and a column for every facet", factors.columns());
    }
    let share = |v: &[Float], name: &str| -> Result<()> {
        if v.len() != n {
            bail!("{} values of {name} for {n} facets", v.len());
        }
        match v.iter().position(|x| !(0.0..=1.0).contains(x)) {
            Some(i) => bail!("facet {i}: a {name} of {}", v[i]),
            None => Ok(()),
        }
    };
    share(albedo, "albedo")?;
    share(emissivity, "emissivity")?;
    if !(bounces.tolerance > 0.0 && bounces.tolerance.is_finite()) {
        bail!("a tolerance of {} on the radiosity", bounces.tolerance);
    }
    let given = |v: Option<&[Float]>, name: &str| -> Result<()> {
        let Some(v) = v else { return Ok(()) };
        if v.len() != n {
            bail!("{} values of {name} for {n} facets", v.len());
        }
        match v.iter().position(|x| !(x.is_finite() && *x >= 0.0)) {
            Some(i) => bail!("facet {i}: {name} of {}", v[i]),
            None => Ok(()),
        }
    };
    given(incident, "sunlight")?;
    given(temperature, "temperature")
}

/// `J = source + reflects F J`, by substitution, and the passes it took (1 for the source alone).
fn radiosity(factors: &ViewFactors, source: &[f64], reflects: &[f64], bounces: Bounces, name: &str) -> Result<(Vec<f64>, usize)> {
    let scale = source.iter().fold(0.0_f64, |m, &s| m.max(s));
    let mut j = source.to_vec();
    let mut passes = 1;
    if scale == 0.0 {
        return Ok((j, passes));
    }
    let most = if bounces.most == 0 { Bounces::LIMIT } else { bounces.most };
    while passes < most {
        let arriving = factors.product(&j);
        let mut change = 0.0_f64;
        for i in 0..j.len() {
            let next = source[i] + reflects[i] * arriving[i];
            change = change.max((next - j[i]).abs());
            j[i] = next;
        }
        passes += 1;
        if change <= bounces.tolerance as f64 * scale {
            return Ok((j, passes));
        }
    }
    if bounces.most == 0 {
        bail!(
            "the {name} did not settle to {} of its largest source in {} reflections: a cavity that absorbs nothing?",
            bounces.tolerance,
            Bounces::LIMIT
        );
    }
    Ok((j, passes))
}

#[cfg(feature = "python")]
pub(crate) mod py {
    use numpy::{PyArray1, PyReadonlyArray2, PyReadonlyArray3, ToPyArray};
    use pyo3::prelude::*;

    use super::super::core::py::{per_facet, value_error};
    use super::Bounces;
    use crate::Float;

    /// View factors between facets, kept sparse: row `i`, column `j` is the
    /// share of what leaves facet `i` that reaches facet `j`.
    ///
    /// ```python
    /// from kalast.tpm import exchange
    /// vf = exchange.ViewFactors.from_csr(m.data, m.indices, m.indptr, m.shape[1])   # a scipy.sparse.csr_matrix
    /// vf = exchange.ViewFactors.from_dense(rows)                                     # what sim.hemicube() returns
    /// vf = exchange.ViewFactors.exact(triangles)                                     # (n, 3, 3): every pair, exactly
    /// ```
    ///
    /// `exact` looks for nothing between two triangles: it is for surfaces
    /// that all see each other, and the reference for a hemicube.
    #[pyclass(module = "kalast._rs.tpm.exchange")]
    pub struct ViewFactors {
        pub(crate) inner: super::ViewFactors,
    }

    #[pymethods]
    impl ViewFactors {
        #[staticmethod]
        fn from_csr(data: &Bound<'_, PyAny>, indices: &Bound<'_, PyAny>, indptr: &Bound<'_, PyAny>, columns: usize) -> PyResult<Self> {
            let whole = |v: &Bound<'_, PyAny>, name: &str| -> PyResult<Vec<usize>> {
                v.call_method0("tolist")
                    .and_then(|l| l.extract::<Vec<usize>>())
                    .or_else(|_| v.extract::<Vec<usize>>())
                    .map_err(|_| pyo3::exceptions::PyTypeError::new_err(format!("{name} must be whole numbers, zero or more")))
            };
            let value = per_facet(data, "data")?.to_vec();
            let column: Vec<u32> = whole(indices, "indices")?.into_iter().map(|c| c.min(u32::MAX as usize) as u32).collect();
            let inner = super::ViewFactors::from_csr(columns, whole(indptr, "indptr")?, column, value).map_err(value_error)?;
            Ok(Self { inner })
        }

        #[staticmethod]
        #[pyo3(signature = (matrix, threshold=1.0e-7))]
        fn from_dense(matrix: PyReadonlyArray2<'_, Float>, threshold: Float) -> PyResult<Self> {
            Ok(Self { inner: super::ViewFactors::from_dense(matrix.as_array(), threshold).map_err(value_error)? })
        }

        #[staticmethod]
        fn exact(triangles: PyReadonlyArray3<'_, Float>) -> PyResult<Self> {
            let t = triangles.as_array();
            if t.dim().1 != 3 || t.dim().2 != 3 {
                return Err(pyo3::exceptions::PyValueError::new_err(format!("triangles are an (n, 3, 3) array of vertices, got {:?}", t.dim())));
            }
            let corner = |i: usize, k: usize| crate::Vec3::new(t[[i, k, 0]], t[[i, k, 1]], t[[i, k, 2]]);
            let list: Vec<super::Triangle> = (0..t.dim().0).map(|i| [corner(i, 0), corner(i, 1), corner(i, 2)]).collect();
            Ok(Self { inner: super::ViewFactors::exact(&list) })
        }

        /// `(rows, columns)`.
        #[getter]
        fn shape(&self) -> (usize, usize) {
            (self.inner.rows(), self.inner.columns())
        }

        /// The view factors kept.
        #[getter]
        fn nnz(&self) -> usize {
            self.inner.nonzeros()
        }

        /// `(data, indices, indptr)`, as `scipy.sparse.csr_matrix` takes them.
        fn csr<'py>(&self, py: Python<'py>) -> (Bound<'py, PyArray1<Float>>, Bound<'py, PyArray1<u32>>, Bound<'py, PyArray1<u64>>) {
            let (start, column, value) = self.inner.csr();
            let start: Vec<u64> = start.iter().map(|&s| s as u64).collect();
            (value.to_pyarray(py), column.to_pyarray(py), start.to_pyarray(py))
        }

        /// The share of what each facet sends that reaches another facet.
        fn row_sums<'py>(&self, py: Python<'py>) -> Bound<'py, PyArray1<Float>> {
            self.inner.row_sums().to_pyarray(py)
        }

        /// `sum_j F_ij x_j` for every row.
        fn dot<'py>(&self, py: Python<'py>, x: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyArray1<Float>>> {
            let x = per_facet(x, "x")?;
            Ok(self.inner.dot(x.as_slice().unwrap()).map_err(value_error)?.to_pyarray(py))
        }

        fn __repr__(&self) -> String {
            format!("ViewFactors({} x {}, {} kept)", self.inner.rows(), self.inner.columns(), self.inner.nonzeros())
        }
    }

    /// One value a facet from a number or from an array.
    pub(crate) fn spread(v: &Bound<'_, PyAny>, n: usize, name: &str) -> PyResult<Vec<Float>> {
        if let Ok(x) = v.extract::<Float>() {
            return Ok(vec![x; n]);
        }
        Ok(per_facet(v, name)?.to_vec())
    }

    /// What every facet absorbs from the others (W/m2): the sunlight they
    /// scatter and their thermal radiation, with the reflections.
    ///
    /// ```python
    /// scattered, thermal = exchange.absorbed(vf, albedo, emissivity, incident=sunlight, temperature=t[0])
    /// core.flux_bc(t, (1 - albedo) * sunlight + scattered + thermal, prop, dz)
    /// ```
    ///
    /// `vf` has a row and a column for each facet of every body, so that
    /// what two bodies send to each other and back is in it. `incident` is
    /// the sunlight falling on each facet (before its albedo, with the
    /// shadows in it), `temperature` each facet's surface temperature;
    /// either may be left out. `albedo` and `emissivity` are a number or one
    /// a facet.
    ///
    /// `bounces=1` keeps what the other facets send directly, `bounces=n`
    /// `n - 1` reflections more, and `bounces=0` follows them until a pass
    /// changes no radiosity by more than `tolerance` times the largest
    /// source.
    #[pyfunction]
    #[pyo3(signature = (factors, albedo, emissivity, incident=None, temperature=None, bounces=0, tolerance=1.0e-6))]
    #[allow(clippy::too_many_arguments, clippy::type_complexity)]
    pub fn absorbed<'py>(
        py: Python<'py>,
        factors: PyRef<'py, ViewFactors>,
        albedo: &Bound<'py, PyAny>,
        emissivity: &Bound<'py, PyAny>,
        incident: Option<&Bound<'py, PyAny>>,
        temperature: Option<&Bound<'py, PyAny>>,
        bounces: usize,
        tolerance: Float,
    ) -> PyResult<(Bound<'py, PyArray1<Float>>, Bound<'py, PyArray1<Float>>)> {
        let n = factors.inner.rows();
        let albedo = spread(albedo, n, "albedo")?;
        let emissivity = spread(emissivity, n, "emissivity")?;
        let incident = incident.map(|v| per_facet(v, "incident")).transpose()?;
        let temperature = temperature.map(|v| per_facet(v, "temperature")).transpose()?;
        let got = super::absorbed(
            &factors.inner,
            &albedo,
            &emissivity,
            incident.as_ref().map(|v| v.as_slice().unwrap()),
            temperature.as_ref().map(|v| v.as_slice().unwrap()),
            Bounces { most: bounces, tolerance },
        )
        .map_err(value_error)?;
        Ok((got.scattered.to_pyarray(py), got.thermal.to_pyarray(py)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Vec3;

    fn measured() -> bool {
        std::env::var_os("KALAST_MEASURE").is_some()
    }

    /// A box looking inward, as twelve triangles, and their areas.
    fn closed_box() -> (Vec<Triangle>, Vec<f64>) {
        let (x, y, z) = (Vec3::X * 1.3, Vec3::Y * 0.8, Vec3::Z * 2.1);
        let o = Vec3::new(-0.4, 0.2, 0.1);
        let face = |origin: Vec3, e1: Vec3, e2: Vec3| [[origin, origin + e1, origin + e1 + e2], [origin, origin + e1 + e2, origin + e2]];
        let faces = [face(o, x, y), face(o + z, y, x), face(o, z, x), face(o + y, x, z), face(o, y, z), face(o + x, z, y)];
        let all: Vec<Triangle> = faces.iter().flatten().copied().collect();
        let area = all.iter().map(|t| 0.5 * (t[1] - t[0]).cross(t[2] - t[0]).length() as f64).collect();
        (all, area)
    }

    #[test]
    fn two_facets_have_their_closed_form() {
        // two facets that see each other and nothing else: a 2 by 2 system
        let (f12, f21) = (0.3, 0.12);
        let factors = ViewFactors::from_csr(2, vec![0, 1, 2], vec![1, 0], vec![f12, f21]).unwrap();
        let (albedo, emissivity) = ([0.4, 0.25], [0.9, 0.7]);
        let (sun, temperature) = ([1000.0, 0.0], [300.0, 250.0]);
        let got = absorbed(&factors, &albedo, &emissivity, Some(&sun), Some(&temperature), Bounces::default()).unwrap();

        let (a1, a2, s1) = (0.4_f64, 0.25_f64, 1000.0_f64);
        let (f12, f21) = (f12 as f64, f21 as f64);
        let j1 = a1 * s1 / (1.0 - a1 * a2 * f12 * f21);
        let j2 = a2 * f21 * j1;
        let scattered = [(1.0 - a1) * f12 * j2, (1.0 - a2) * f21 * j1];
        let (e1, e2) = (0.9_f64, 0.7_f64);
        let sigma = STEFAN_BOLTZMANN as f64;
        let (b1, b2) = (e1 * sigma * 300.0_f64.powi(4), e2 * sigma * 250.0_f64.powi(4));
        let (r1, r2) = (1.0 - e1, 1.0 - e2);
        // J1 = b1 + r1 f12 J2 and J2 = b2 + r2 f21 J1
        let t1 = (b1 + r1 * f12 * b2) / (1.0 - r1 * r2 * f12 * f21);
        let t2 = b2 + r2 * f21 * t1;
        let thermal = [e1 * f12 * t2, e2 * f21 * t1];
        for i in 0..2 {
            assert!((got.scattered[i] as f64 - scattered[i]).abs() < 1.0e-4 * (1.0 + scattered[i]), "{:?} against {scattered:?}", got.scattered);
            assert!((got.thermal[i] as f64 - thermal[i]).abs() < 1.0e-4 * thermal[i], "{:?} against {thermal:?}", got.thermal);
        }

        // one bounce is what the other sends directly
        let direct = absorbed(&factors, &albedo, &emissivity, Some(&sun), Some(&temperature), Bounces { most: 1, ..Default::default() }).unwrap();
        assert_eq!((direct.scattered_bounces, direct.thermal_bounces), (1, 1));
        assert_eq!(direct.scattered[0], 0.0);
        assert!((direct.scattered[1] as f64 - (1.0 - a2) * f21 * a1 * s1).abs() < 1.0e-3);
        assert!((direct.thermal[0] as f64 - e1 * f12 * b2).abs() < 1.0e-4 * b2);
        // and each bounce more brings the sum nearer
        let mut before = f64::INFINITY;
        for most in 1..6 {
            let part = absorbed(&factors, &albedo, &emissivity, Some(&sun), None, Bounces { most, ..Default::default() }).unwrap();
            let miss = (part.scattered[0] as f64 - scattered[0]).abs() + (part.scattered[1] as f64 - scattered[1]).abs();
            assert!(miss <= before + 1.0e-9, "{most} bounces: {miss} after {before}");
            before = miss;
        }
        if measured() {
            println!("two facets: scattered {:?} thermal {:?} in {} and {} passes", got.scattered, got.thermal, got.scattered_bounces, got.thermal_bounces);
        }
    }

    #[test]
    fn an_isothermal_closed_box_absorbs_what_it_emits() {
        // inside a closed cavity at one temperature the radiation is the black body's, whatever the emissivity:
        // each facet absorbs e sigma T^4, which is what it emits
        let (triangles, _) = closed_box();
        let factors = ViewFactors::exact(&triangles);
        let n = triangles.len();
        for sum in factors.row_sums() {
            assert!((sum - 1.0).abs() < 2.0e-6, "a row sum of {sum}");
        }
        let temperature = vec![280.0; n];
        let black = STEFAN_BOLTZMANN as f64 * 280.0_f64.powi(4);
        let mut worst = 0.0_f64;
        for e in [1.0, 0.9, 0.5, 0.2] {
            let got = absorbed(&factors, &vec![0.1; n], &vec![e; n], None, Some(&temperature), Bounces { most: 0, tolerance: 1.0e-9 }).unwrap();
            for v in &got.thermal {
                worst = worst.max((*v as f64 / (e as f64 * black) - 1.0).abs());
            }
            assert!(got.scattered.iter().all(|&v| v == 0.0));
            // followed one bounce only, it is short by what the walls reflect: e^2 sigma T^4
            let direct = absorbed(&factors, &vec![0.1; n], &vec![e; n], None, Some(&temperature), Bounces { most: 1, ..Default::default() }).unwrap();
            assert!((direct.thermal[0] as f64 / (e as f64 * e as f64 * black) - 1.0).abs() < 1.0e-5);
        }
        if measured() {
            println!("isothermal box: absorbed within {worst:.1e} of e sigma T^4 at emissivities 1, 0.9, 0.5 and 0.2");
        }
        assert!(worst < 1.0e-5, "off by {worst}");
    }

    #[test]
    fn light_let_into_a_closed_box_is_all_absorbed() {
        // sunlight put on two of the twelve facets: with no way out, the facets absorb all of it between them
        let (triangles, area) = closed_box();
        let factors = ViewFactors::exact(&triangles);
        let n = triangles.len();
        let mut sun = vec![0.0; n];
        sun[0] = 900.0;
        sun[5] = 300.0;
        for a in [0.1_f32, 0.6, 0.95] {
            let albedo = vec![a; n];
            let got = absorbed(&factors, &albedo, &vec![0.9; n], Some(&sun), None, Bounces { most: 0, tolerance: 1.0e-10 }).unwrap();
            let entered: f64 = (0..n).map(|i| area[i] * sun[i] as f64).sum();
            let kept: f64 = (0..n).map(|i| area[i] * ((1.0 - a as f64) * sun[i] as f64 + got.scattered[i] as f64)).sum();
            if measured() {
                println!("closed box, albedo {a}: {kept:.4} W absorbed of {entered:.4} W, {} passes", got.scattered_bounces);
            }
            assert!((kept / entered - 1.0).abs() < 2.0e-5, "albedo {a}: {kept} of {entered}");
        }
        // a cavity that absorbs nothing never settles, and says so
        let err = absorbed(&factors, &vec![1.0; n], &vec![0.9; n], Some(&sun), None, Bounces::default()).unwrap_err();
        assert!(err.to_string().contains("did not settle"), "{err}");
    }

    #[test]
    fn the_sparse_rows_are_the_matrix() {
        let dense = ndarray::arr2(&[[0.0, 0.2, 0.0, 1.0e-9], [0.3, 0.0, 0.1, 0.0], [0.0, 0.0, 0.0, 0.0]]);
        let factors = ViewFactors::from_dense(dense.view(), 1.0e-7).unwrap();
        assert_eq!((factors.rows(), factors.columns(), factors.nonzeros()), (3, 4, 3));
        assert_eq!(factors.csr(), (&[0, 1, 3, 3][..], &[1, 0, 2][..], &[0.2, 0.3, 0.1][..]));
        let x = [1.0, 10.0, 100.0, 1000.0];
        let got = factors.dot(&x).unwrap();
        assert!((got[0] - 2.0).abs() < 1.0e-6 && (got[1] - 10.3).abs() < 1.0e-5 && got[2] == 0.0, "{got:?}");
        let sums = factors.row_sums();
        assert!((sums[0] - 0.2).abs() < 1.0e-7 && (sums[1] - 0.4).abs() < 1.0e-7 && sums[2] == 0.0);
        assert!(factors.dot(&x[..3]).is_err());
    }

    #[test]
    fn what_makes_no_sense_is_refused() {
        let square = ViewFactors::from_csr(2, vec![0, 1, 2], vec![1, 0], vec![0.3, 0.1]).unwrap();
        let ok = |a: &[Float], e: &[Float], s: Option<&[Float]>, t: Option<&[Float]>| absorbed(&square, a, e, s, t, Bounces::default());
        assert!(ok(&[0.1, 0.1], &[0.9, 0.9], Some(&[1.0, 2.0]), Some(&[200.0, 300.0])).is_ok());
        // nothing given: nothing absorbed
        let none = ok(&[0.1, 0.1], &[0.9, 0.9], None, None).unwrap();
        assert!(none.scattered == vec![0.0, 0.0] && none.thermal == vec![0.0, 0.0]);
        for err in [
            ok(&[0.1], &[0.9, 0.9], None, None),
            ok(&[0.1, 1.5], &[0.9, 0.9], None, None),
            ok(&[0.1, 0.1], &[0.9, -0.1], None, None),
            ok(&[0.1, 0.1], &[0.9, 0.9], Some(&[1.0, -2.0]), None),
            ok(&[0.1, 0.1], &[0.9, 0.9], Some(&[1.0]), None),
            ok(&[0.1, 0.1], &[0.9, 0.9], None, Some(&[200.0, Float::NAN])),
            absorbed(&square, &[0.1, 0.1], &[0.9, 0.9], None, None, Bounces { most: 0, tolerance: 0.0 }),
        ] {
            assert!(err.is_err());
        }
        // rows of one body over the columns of two are not what the exchange takes
        let wide = ViewFactors::from_csr(3, vec![0, 1, 2], vec![1, 2], vec![0.3, 0.1]).unwrap();
        let err = absorbed(&wide, &[0.1, 0.1], &[0.9, 0.9], None, None, Bounces::default()).unwrap_err();
        assert!(err.to_string().contains("a row and a column for every facet"), "{err}");
        // and rows that are not rows
        assert!(ViewFactors::from_csr(2, vec![0, 2, 1], vec![0, 1], vec![0.1, 0.1]).is_err());
        assert!(ViewFactors::from_csr(2, vec![0, 1, 2], vec![0, 2], vec![0.1, 0.1]).is_err());
        assert!(ViewFactors::from_csr(2, vec![0, 1, 2], vec![0, 1], vec![0.1, -0.1]).is_err());
        assert!(ViewFactors::from_csr(2, vec![1, 1, 2], vec![0, 1], vec![0.1, 0.1]).is_err());
    }
}
