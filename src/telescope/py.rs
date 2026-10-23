//! `kalast.telescope`: a night's frames as a telescope records them, and
//! their photometry. The classes are the engine's own; here their
//! constructors, a frame, and the functions.

use numpy::ndarray::Array2;
use numpy::IntoPyArray;
use pyo3::exceptions::{PyIOError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;

use super::fits;
use super::photometry::Aperture;
use super::{Band, Effects, Exposure, Reduction, Sky, Source, Telescope};

#[pymethods]
impl Telescope {
    /// The instrument. The defaults are TRAPPIST-North's: its published
    /// camera, and an assumed obstruction, throughput and bias.
    #[new]
    #[pyo3(signature = (
        name = String::from("TRAPPIST-North"),
        camera = String::from("Andor iKon-L BEX2-DD"),
        width = 2048,
        height = 2048,
        pixel_scale = 0.60,
        aperture = 0.60,
        obstruction = 0.35,
        throughput = 0.60,
        gain = 1.1,
        read_noise = 7.0,
        dark = 0.15,
        bias = 1000.0,
        full_well = 100_000.0,
    ))]
    #[allow(clippy::too_many_arguments)]
    fn py_new(
        name: String,
        camera: String,
        width: usize,
        height: usize,
        pixel_scale: f64,
        aperture: f64,
        obstruction: f64,
        throughput: f64,
        gain: f64,
        read_noise: f64,
        dark: f64,
        bias: f64,
        full_well: f64,
    ) -> Self {
        Self { name, camera, width, height, pixel_scale, aperture, obstruction, throughput, gain, read_noise, dark, bias, full_well }
    }

    /// Electrons a second from a star of magnitude zero in `band`, above
    /// the atmosphere.
    #[pyo3(name = "zero_rate")]
    fn py_zero_rate(&self, band: &Band) -> f64 {
        self.zero_rate(band)
    }

    fn __repr__(&self) -> String {
        format!("{self:?}")
    }
}

#[pymethods]
impl Band {
    /// A band by name -- "B", "V", "Rc" or "Ic" -- or one of your own with
    /// `sun`, the Sun's magnitude in it at 1 au, and `photons`, a
    /// magnitude-zero star's photons a second a square centimetre.
    #[new]
    #[pyo3(signature = (name = "Rc", sun = None, photons = None))]
    fn py_new(name: &str, sun: Option<f64>, photons: Option<f64>) -> PyResult<Self> {
        match (Band::named(name), sun, photons) {
            (_, Some(sun), Some(photons)) => Ok(Self { name: name.to_string(), sun, photons }),
            (Some(mut band), sun, photons) => {
                band.sun = sun.unwrap_or(band.sun);
                band.photons = photons.unwrap_or(band.photons);
                Ok(band)
            }
            (None, _, _) => Err(PyValueError::new_err(format!(
                "no band {name:?}: B, V, Rc or Ic, or give its `sun` and `photons`"
            ))),
        }
    }

    /// A target's magnitude in the band from its disc integral `flux` (km²
    /// for a mesh in km, as `kalast.lightcurve.flux` and `sim.fluxes()`
    /// give it), `r` its distance from the Sun in au and `delta` from the
    /// observer in km.
    #[pyo3(name = "magnitude")]
    fn py_magnitude(&self, flux: f64, r: f64, delta: f64) -> f64 {
        self.magnitude(flux, r, delta)
    }

    fn __repr__(&self) -> String {
        format!("{self:?}")
    }
}

#[pymethods]
impl Sky {
    /// The site and the night; the defaults a dark night at Oukaïmeden in
    /// Rc, assumed.
    #[new]
    #[pyo3(signature = (seeing = 2.0, beta = 3.0, brightness = 20.5, extinction = 0.10, altitude = 2751.0))]
    fn py_new(seeing: f64, beta: f64, brightness: f64, extinction: f64, altitude: f64) -> Self {
        Self { seeing, beta, brightness, extinction, altitude }
    }

    fn __repr__(&self) -> String {
        format!("{self:?}")
    }
}

#[pymethods]
impl Effects {
    /// What a frame has of what a real one would: each on unless switched
    /// off here, `Effects(stars=False)`.
    #[new]
    #[pyo3(signature = (
        psf = true,
        trail = true,
        sky = true,
        stars = true,
        extinction = true,
        scintillation = true,
        photon_noise = true,
        dark = true,
        read_noise = true,
        bias = true,
        saturation = true,
    ))]
    #[allow(clippy::too_many_arguments)]
    fn py_new(
        psf: bool,
        trail: bool,
        sky: bool,
        stars: bool,
        extinction: bool,
        scintillation: bool,
        photon_noise: bool,
        dark: bool,
        read_noise: bool,
        bias: bool,
        saturation: bool,
    ) -> Self {
        Self { psf, trail, sky, stars, extinction, scintillation, photon_noise, dark, read_noise, bias, saturation }
    }

    fn __repr__(&self) -> String {
        format!("Effects({})", self.names().join(", "))
    }
}

#[pymethods]
impl Source {
    /// A point on the sky, degrees, and its magnitude above the atmosphere;
    /// `rate` its motion against the stars, arcseconds a second east and
    /// north.
    #[new]
    #[pyo3(signature = (ra, dec, mag, rate = (0.0, 0.0), comparison = false))]
    fn py_new(ra: f64, dec: f64, mag: f64, rate: (f64, f64), comparison: bool) -> Self {
        Self { ra, dec, mag, rate, comparison }
    }

    fn __repr__(&self) -> String {
        format!("{self:?}")
    }
}

#[pymethods]
impl Exposure {
    /// One exposure: its start, UTC ISO 8601, its length in seconds, the
    /// frame's centre in degrees, the target's airmass, the noise's seed.
    #[new]
    #[pyo3(signature = (start, duration, ra, dec, airmass = 1.0, seed = 0))]
    fn py_new(start: String, duration: f64, ra: f64, dec: f64, airmass: f64, seed: u64) -> Self {
        Self { start, duration, ra, dec, airmass, seed }
    }

    fn __repr__(&self) -> String {
        format!("{self:?}")
    }
}

#[pymethods]
impl Aperture {
    /// The circle the light is summed in and the ring the sky is read from,
    /// radii in pixels.
    #[new]
    #[pyo3(signature = (radius, inner, outer))]
    fn py_new(radius: f64, inner: f64, outer: f64) -> PyResult<Self> {
        if !(radius > 0.0 && inner >= radius && outer > inner) {
            return Err(PyValueError::new_err("the radii go aperture <= inner < outer"));
        }
        Ok(Self { radius, inner, outer })
    }

    fn __repr__(&self) -> String {
        format!("{self:?}")
    }
}

#[pymethods]
impl Reduction {
    fn __repr__(&self) -> String {
        format!(
            "Reduction(time={:.6}, mag={:.4}, error={:.4}, zero_point={:.4}, comparisons={})",
            self.time, self.mag, self.error, self.zero_point, self.comparisons
        )
    }
}

/// A frame: its counts, ADU, and its header.
#[pyclass(module = "kalast.telescope")]
pub struct Frame {
    pub image: fits::Image,
}

fn value(py: Python<'_>, v: &fits::Value) -> PyResult<Py<PyAny>> {
    Ok(match v {
        fits::Value::Bool(b) => b.into_pyobject(py)?.to_owned().into_any().unbind(),
        fits::Value::Int(i) => i.into_pyobject(py)?.into_any().unbind(),
        fits::Value::Float(f) => f.into_pyobject(py)?.into_any().unbind(),
        fits::Value::Str(s) => s.into_pyobject(py)?.into_any().unbind(),
    })
}

#[pymethods]
impl Frame {
    #[getter]
    /// Its counts, `(height, width)`, row 0 the FITS file's first: as
    /// astropy indexes it.
    fn data<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, numpy::PyArray2<f32>>> {
        let a = Array2::from_shape_vec((self.image.height, self.image.width), self.image.data.clone())
            .map_err(|e| PyValueError::new_err(e.to_string()))?;
        Ok(a.into_pyarray(py))
    }

    #[getter]
    /// Its header's cards, the structural ones aside, as a dict.
    fn header<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let d = PyDict::new(py);
        for (key, v, _) in &self.image.header.cards {
            d.set_item(key, value(py, v)?)?;
        }
        Ok(d)
    }

    #[getter]
    fn width(&self) -> usize {
        self.image.width
    }

    #[getter]
    fn height(&self) -> usize {
        self.image.height
    }

    /// Write it as a FITS file: unsigned 16-bit counts, as the camera does
    /// (`"u16"`), or 32-bit floats (`"f32"`).
    #[pyo3(signature = (path, storage = "u16"))]
    fn write(&self, path: std::path::PathBuf, storage: &str) -> PyResult<()> {
        let storage = match storage {
            "u16" => fits::Storage::U16,
            "f32" => fits::Storage::F32,
            _ => return Err(PyValueError::new_err("storage is \"u16\" or \"f32\"")),
        };
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir).map_err(|e| PyIOError::new_err(format!("{}: {e}", dir.display())))?;
        }
        let i = &self.image;
        fits::write(&path, i.width, i.height, &i.data, &i.header, storage)
            .map_err(|e| PyIOError::new_err(format!("{}: {e}", path.display())))
    }

    fn __repr__(&self) -> String {
        let h = &self.image.header;
        format!(
            "Frame({} x {}, {}, {} s, {})",
            self.image.width,
            self.image.height,
            h.str("DATE-OBS").unwrap_or("?"),
            h.f64("EXPTIME").unwrap_or(f64::NAN),
            h.str("FILTER").unwrap_or("?")
        )
    }
}

/// A frame of `target` and `stars` through `telescope` in `band` under
/// `sky`, with `effects`: counts as the camera stores them, a header saying
/// when, through what, under which sky, where the target was predicted and
/// which effects it has, and the frame's WCS. Python's lock is let go
/// meanwhile.
#[pyfunction]
#[pyo3(signature = (telescope, band, sky, effects, exposure, target, stars, object = ""))]
#[allow(clippy::too_many_arguments)]
pub fn expose(
    py: Python<'_>,
    telescope: Telescope,
    band: Band,
    sky: Sky,
    effects: Effects,
    exposure: Exposure,
    target: Source,
    stars: Vec<Source>,
    object: &str,
) -> Frame {
    let object = object.to_string();
    let image = py.detach(move || super::expose(&telescope, &band, &sky, &effects, &exposure, &target, &stars, &object));
    Frame { image }
}

/// A FITS file's primary image and its header: kalast's frames, or a real
/// night's.
#[pyfunction]
pub fn read(path: std::path::PathBuf) -> PyResult<Frame> {
    let image = fits::read(&path).map_err(|e| PyIOError::new_err(e.to_string()))?;
    Ok(Frame { image })
}

/// A frame reduced as a night's frames are: the target measured where
/// `target` (degrees) puts it -- the header's `TARGRA` and `TARGDEC` by
/// default -- its comparison stars where `stars` puts them, its zero point
/// from them. `None` where the target or fewer than two stars can be
/// measured.
#[pyfunction]
#[pyo3(name = "reduce", signature = (frame, stars, aperture, target = None))]
pub fn py_reduce(
    py: Python<'_>,
    frame: &Frame,
    stars: Vec<Source>,
    aperture: Aperture,
    target: Option<(f64, f64)>,
) -> Option<Reduction> {
    let image = &frame.image;
    py.detach(|| super::reduce(image, &stars, &aperture, target))
}

/// The field's stars from Gaia DR3 within `radius` degrees of (`ra`, `dec`)
/// and brighter than `g_max` in G, as sources in `band` -- Riello et al.
/// (2021)'s magnitudes, comparison stars where those are known -- carried
/// by their proper motions to `year` (Julian years), Gaia's 2016.0 by
/// default. Asked of ESA's Gaia archive, or read from `cache` when that
/// file holds them; the archive's answer is written there the first time.
#[pyfunction]
#[pyo3(signature = (ra, dec, radius, g_max = 19.0, band = "Rc", year = 2016.0, cache = None))]
pub fn gaia(
    py: Python<'_>,
    ra: f64,
    dec: f64,
    radius: f64,
    g_max: f64,
    band: &str,
    year: f64,
    cache: Option<std::path::PathBuf>,
) -> PyResult<Vec<Source>> {
    let stars = py
        .detach(|| super::gaia::cone(ra, dec, radius, g_max, cache.as_deref()))
        .map_err(PyIOError::new_err)?;
    Ok(stars.iter().map(|s| s.source(band, year)).collect())
}

/// Airmass at an altitude above the horizon, degrees (Kasten & Young
/// 1989).
#[pyfunction]
#[pyo3(name = "airmass")]
pub fn py_airmass(altitude: f64) -> f64 {
    super::airmass(altitude)
}

/// The Julian date of an ISO 8601 UTC time.
#[pyfunction]
#[pyo3(name = "julian_date")]
pub fn py_julian_date(iso: &str) -> PyResult<f64> {
    super::julian_date(iso).ok_or_else(|| PyValueError::new_err(format!("not an ISO time: {iso:?}")))
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Telescope>()?;
    m.add_class::<Band>()?;
    m.add_class::<Sky>()?;
    m.add_class::<Effects>()?;
    m.add_class::<Source>()?;
    m.add_class::<Exposure>()?;
    m.add_class::<Aperture>()?;
    m.add_class::<Reduction>()?;
    m.add_class::<Frame>()?;
    m.add_function(wrap_pyfunction!(expose, m)?)?;
    m.add_function(wrap_pyfunction!(read, m)?)?;
    m.add_function(wrap_pyfunction!(py_reduce, m)?)?;
    m.add_function(wrap_pyfunction!(gaia, m)?)?;
    m.add_function(wrap_pyfunction!(py_airmass, m)?)?;
    m.add_function(wrap_pyfunction!(py_julian_date, m)?)?;
    Ok(())
}
