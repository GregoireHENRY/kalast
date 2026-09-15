use std::{cell::RefCell, rc::Rc};

use pyo3::prelude::*;



/// The built-in colormaps, by name.
pub const COLORMAP_NAMES: [&str; 4] = ["viridis", "inferno", "turbo", "grey"];

/// A built-in colormap as a 256x3 array, the way matplotlib hands one over.
///
/// Exists so the built-ins are *data* rather than a magic string only the
/// setter understands: fetched as an array they can be reversed, sliced or
/// concatenated before use.
///
/// ```python
/// app.config.colormap = kalast.app.config.colormap("inferno")[::-1]   # reversed
/// ```
#[pyfunction]
#[pyo3(name = "colormap")]
pub fn colormap_by_name<'py>(
    py: Python<'py>,
    name: &str,
) -> PyResult<Bound<'py, numpy::PyArray2<f32>>> {
    let table = crate::app::config::builtin_colormap(name).ok_or_else(|| {
        pyo3::exceptions::PyValueError::new_err(format!(
            "unknown colormap {name:?}: built-ins are {}",
            COLORMAP_NAMES.join(", ")
        ))
    })?;
    // Resampled to the renderer's own table size, so what comes back is what
    // would be used -- the built-ins are stored as 8 anchors, and handing
    // those out would make a sliced or reversed copy needlessly coarse.
    let table =
        crate::app::config::resample_colormap(&table, crate::app::uniform::COLORMAP_SIZE);
    let rows: Vec<Vec<f32>> = table.iter().map(|c| c.to_vec()).collect();
    Ok(numpy::PyArray2::from_vec2(py, &rows)?)
}

/// Names accepted by `colormap()` and by `config.colormap`.
#[pyfunction]
pub fn colormap_names() -> Vec<String> {
    COLORMAP_NAMES.iter().map(|s| s.to_string()).collect()
}

/// A colormap row needs at least r, g and b; a fourth is alpha and ignored.
fn check_cols(n: usize) -> PyResult<()> {
    if n < 3 {
        return Err(pyo3::exceptions::PyValueError::new_err(format!(
            "colormap rows must have at least 3 columns (r, g, b), got {n}"
        )));
    }
    Ok(())
}

/// One HUD overlay: a template, a corner, and how it looks.
///
/// ```python
/// kalast.app.Hud("{it}/{nit}")                       # top-left, the default
/// kalast.app.Hud("{fps} fps", anchor="bottom-right")
/// kalast.app.Hud("{hud}", x=200, y=120, size=24.0)  # absolute, no anchor needed
/// ```
#[pyclass(from_py_object, unsendable)]
#[derive(Clone)]
pub struct Hud {
    /// Shared with `Config::huds` and `Simulation::huds`, so the object a
    /// script holds *is* the one being drawn -- `sim.huds[0].text = ...` in
    /// `before_render` takes effect on that frame with nothing to reassign.
    pub inner: std::rc::Rc<std::cell::RefCell<crate::app::config::Hud>>,
}

#[pymethods]
impl Hud {
    #[new]
    #[pyo3(signature = (text, anchor="top-left", x=None, y=None, size=18.0, color=None,
                        align_h=None, align_v=None))]
    fn new(
        text: &str,
        anchor: &str,
        x: Option<f32>,
        y: Option<f32>,
        size: f32,
        color: Option<[f32; 4]>,
        align_h: Option<&str>,
        align_v: Option<&str>,
    ) -> PyResult<Self> {
        let anchor = crate::app::config::HudAnchor::parse(anchor).ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err(format!(
                "unknown hud anchor {anchor:?}: expected one of top-left, top-center, \
                 top-right, middle-left, middle-center, middle-right, bottom-left, \
                 bottom-center, bottom-right"
            ))
        })?;
        let align_h = align_h
            .map(|a| {
                crate::app::config::HAlign::parse(a).ok_or_else(|| {
                    pyo3::exceptions::PyValueError::new_err(format!(
                        "unknown align_h {a:?}: expected left, center or right"
                    ))
                })
            })
            .transpose()?;
        let align_v = align_v
            .map(|a| {
                crate::app::config::VAlign::parse(a).ok_or_else(|| {
                    pyo3::exceptions::PyValueError::new_err(format!(
                        "unknown align_v {a:?}: expected top, center or bottom"
                    ))
                })
            })
            .transpose()?;
        let mut inner = crate::app::config::Hud::new(text);
        inner.anchor = anchor;
        // Default inset is the same 8/6 px in from whichever corner, so a
        // bottom-right HUD sits as far from its edges as a top-left one.
        if let Some(x) = x {
            inner.x = x;
        }
        if let Some(y) = y {
            inner.y = y;
        }
        inner.size = size;
        if let Some(c) = color {
            inner.color = c;
        }
        inner.align_h = align_h;
        inner.align_v = align_v;
        Ok(Self {
            inner: std::rc::Rc::new(std::cell::RefCell::new(inner)),
        })
    }

    #[getter]
    /// The template drawn for this HUD. See `Config::huds` for placeholders.
    fn text(&self) -> String {
        self.inner.borrow().text.clone()
    }
    #[setter]
    fn set_text(&mut self, v: &str) {
        self.inner.borrow_mut().text = v.to_string();
    }

    /// Used in place of `text` while it is set, whatever `text` says.
    ///
    /// For taking a HUD off a script and giving it to a person. A callback
    /// that assigns `text` every iteration owns it completely, and a driven
    /// script goes on assigning even while paused -- pausing stops the
    /// iteration counter, not a `while` loop the script owns -- so there is
    /// nowhere else for an edit to stand.
    ///
    /// Typing in the editor's HUDs section sets it; its release button, or
    /// `None` here, gives the HUD back.
    #[getter]
    fn pin(&self) -> Option<String> {
        self.inner.borrow().pin.clone()
    }
    #[setter]
    fn set_pin(&mut self, v: Option<String>) {
        self.inner.borrow_mut().pin = v;
    }

    #[getter]
    /// Which corner `x`/`y` are measured from: `top-left`, `top-right`,
    /// `bottom-left` or `bottom-right`.
    fn anchor(&self) -> String {
        self.inner.borrow().anchor.name().to_string()
    }
    #[setter]
    fn set_anchor(&mut self, v: &str) -> PyResult<()> {
        self.inner.borrow_mut().anchor = crate::app::config::HudAnchor::parse(v).ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err(format!("unknown hud anchor {v:?}"))
        })?;
        Ok(())
    }

    #[getter]
    fn x(&self) -> f32 {
        self.inner.borrow().x
    }
    #[setter]
    fn set_x(&mut self, v: f32) {
        self.inner.borrow_mut().x = v;
    }

    #[getter]
    /// Vertical inset from the anchor, in pixels.
    ///
    /// With the default top-left anchor this is simply the distance from the top.
    fn y(&self) -> f32 {
        self.inner.borrow().y
    }
    #[setter]
    fn set_y(&mut self, v: f32) {
        self.inner.borrow_mut().y = v;
    }

    /// Font size in pixels.
    #[getter]
    fn size(&self) -> f32 {
        self.inner.borrow().size
    }
    #[setter]
    fn set_size(&mut self, v: f32) {
        self.inner.borrow_mut().size = v;
    }

    #[getter]
    fn color(&self) -> [f32; 4] {
        self.inner.borrow().color
    }
    #[setter]
    fn set_color(&mut self, v: [f32; 4]) {
        self.inner.borrow_mut().color = v;
    }

    fn __repr__(&self) -> String {
        let h = self.inner.borrow();
        format!(
            "Hud(text={:?}, anchor={:?}, x={}, y={}, size={})",
            h.text,
            h.anchor.name(),
            h.x,
            h.y,
            h.size
        )
    }
}

/// Settings for the application: the window now, panels and colours later.
///
/// Separate from `simulation.config` because they answer different
/// questions -- this one about the program you are looking at, that one about
/// the thing being simulated.
#[pyclass(from_py_object, unsendable)]
#[derive(Clone)]
pub struct AppConfig {
    pub config: Rc<RefCell<crate::app::config::AppConfig>>,
}

#[pymethods]
impl AppConfig {
    fn __repr__(&self) -> String {
        let c = self.config.borrow();
        format!(
            "AppConfig(editor={}, focus={}, width={}, height={}, toolbar={:?})",
            c.editor, c.focus, c.width, c.height, c.toolbar
        )
    }
}

#[pyclass(unsendable)]
pub struct Config {
    /// The config itself, not the app that owns it. Holding the app would put
    /// every option behind the borrow `start()` takes for the whole run loop,
    /// which is what made `app.config.x = ...` inside `before_render` panic.
    pub config: Rc<RefCell<crate::app::config::Config>>,
    /// Only for `huds`, which live on the simulation.
    pub simulation: Rc<RefCell<crate::app::simulation::Simulation>>,
}

/// The two things on the root that are not a group: the HUD list, which lives
/// on the simulation, and `__repr__`. Every other accessor is generated into
/// `config_gen.rs` from the Rust struct.
#[pymethods]
impl Config {
    /// The on-screen HUDs. Empty (the default) draws none.
    ///
    /// An alias for `app.simulation.huds`, not a second list: they are the
    /// same storage, so declaring them here and editing them there in
    /// `before_render` cannot drift apart. The objects handed back are the
    /// live ones -- setting `.text` on one takes effect on the next frame
    /// with no list to reassign.
    #[getter]
    fn huds(&self) -> Vec<Hud> {
        self.simulation
            .borrow()
            .huds
            .iter()
            .map(|h| Hud { inner: h.clone() })  // Rc clone: the same object
            .collect()
    }

    #[setter]
    fn set_huds(&mut self, v: Vec<Hud>) {
        self.simulation.borrow_mut().huds =
            v.into_iter().map(|h| h.inner).collect();
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self.config.borrow())
    }
}

/// The colormap, by hand: it takes a built-in's name, an Nx3 or Nx4 array in
/// either float width, or a sequence of triples, and resamples to the
/// renderer's table -- none of which a mechanical accessor can express. The
/// field is marked `:py_custom:` so the generator leaves it to this.
#[pymethods]
impl super::config_gen::DataConfig {
    /// Colour lookup table: a built-in name or an Nx3 array of RGB in 0..1.
    ///
    /// `"viridis"`, `"inferno"`, `"turbo"`, `"grey"`, or any matplotlib
    /// colormap passed straight through:
    ///
    /// ```python
    /// app.config.colormap = matplotlib.colormaps["magma"](numpy.linspace(0, 1, 256))[:, :3]
    /// ```
    ///
    /// Resampled to 256 entries, so any length works.
    /// The colour table in use, as a 256x3 array.
    #[getter]
    fn colormap<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, numpy::PyArray2<f32>>> {
        let rows: Vec<Vec<f32>> = self
            .config
            .borrow()
            .data
            .colormap
            .iter()
            .map(|c| c.to_vec())
            .collect();
        Ok(numpy::PyArray2::from_vec2(py, &rows)?)
    }

    #[setter]
    fn set_colormap(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let table = if let Ok(name) = v.extract::<String>() {
            crate::app::config::builtin_colormap(&name).ok_or_else(|| {
                pyo3::exceptions::PyValueError::new_err(format!(
                    "unknown colormap {name:?}: built-ins are viridis, inferno, turbo, grey;                      otherwise pass an Nx3 array"
                ))
            })?
        } else {
            // f64 first: that is numpy's default and what matplotlib returns,
            // so extracting only f32 rejected the very call the docs give as
            // the example. A plain sequence is accepted too, since a colormap
            // written out by hand is a list of triples.
            let rows: Vec<[f32; 3]> = if let Ok(arr) =
                v.extract::<numpy::borrow::PyReadonlyArray2<f64>>()
            {
                let a = arr.as_array();
                check_cols(a.ncols())?;
                (0..a.nrows())
                    .map(|i| [a[[i, 0]] as f32, a[[i, 1]] as f32, a[[i, 2]] as f32])
                    .collect()
            } else if let Ok(arr) = v.extract::<numpy::borrow::PyReadonlyArray2<f32>>() {
                let a = arr.as_array();
                check_cols(a.ncols())?;
                (0..a.nrows())
                    .map(|i| [a[[i, 0]], a[[i, 1]], a[[i, 2]]])
                    .collect()
            } else {
                let seq: Vec<Vec<f32>> = v.extract().map_err(|_| {
                    pyo3::exceptions::PyTypeError::new_err(
                        "colormap must be a built-in name, an Nx3 or Nx4 array, \
                         or a sequence of [r, g, b] triples",
                    )
                })?;
                seq.iter()
                    .map(|row| {
                        check_cols(row.len())?;
                        Ok([row[0], row[1], row[2]])
                    })
                    .collect::<PyResult<Vec<_>>>()?
            };
            if rows.is_empty() {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "colormap is empty",
                ));
            }
            rows
        };
        self.config.borrow_mut().data.colormap = table;
        Ok(())
    }
}
