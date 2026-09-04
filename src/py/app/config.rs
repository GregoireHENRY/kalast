use std::{cell::RefCell, rc::Rc};

use pyo3::prelude::*;

use crate::Float;


/// One HUD overlay: a template, a corner, and how it looks.
///
/// ```python
/// kalast.app.Hud("{it}/{nit}")                       # top-left, the default
/// kalast.app.Hud("{fps} fps", anchor="bottom-right")
/// kalast.app.Hud("{hud}", x=200, y=120, size=24.0)  # absolute, no anchor needed
/// ```
#[pyclass(unsendable)]
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
    #[pyo3(signature = (text, anchor="top-left", x=None, y=None, size=18.0, color=None))]
    fn new(
        text: &str,
        anchor: &str,
        x: Option<f32>,
        y: Option<f32>,
        size: f32,
        color: Option<[f32; 4]>,
    ) -> PyResult<Self> {
        let anchor = crate::app::config::HudAnchor::parse(anchor).ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err(format!(
                "unknown hud anchor {anchor:?}: expected one of \
                 top-left, top-right, bottom-left, bottom-right"
            ))
        })?;
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

#[pyclass(unsendable)]
pub struct Config {
    pub app: Rc<RefCell<crate::app::App>>,
}

#[pymethods]
impl Config {
    #[getter]
    fn debug_app(&self) -> bool {
        self.app.borrow().config.debug_app
    }

    #[setter]
    fn set_debug_app(&mut self, v: bool) {
        self.app.borrow_mut().config.debug_app = v;
    }

    #[getter]
    fn debug_window(&self) -> bool {
        self.app.borrow().config.debug_window
    }

    #[setter]
    fn set_debug_window(&mut self, v: bool) {
        self.app.borrow_mut().config.debug_window = v;
    }

    #[getter]
    fn debug_window_mesh(&self) -> bool {
        self.app.borrow().config.debug_window_mesh
    }

    #[setter]
    fn set_debug_window_mesh(&mut self, v: bool) {
        self.app.borrow_mut().config.debug_window_mesh = v;
    }

    #[getter]
    fn debug_simulation(&self) -> bool {
        self.app.borrow().config.debug_simulation
    }

    #[setter]
    fn set_debug_simulation(&mut self, v: bool) {
        self.app.borrow_mut().config.debug_simulation = v;
    }

    #[getter]
    fn debug_depth_show(&self) -> bool {
        self.app.borrow().config.debug_depth_show
    }

    #[setter]
    fn set_debug_depth_show(&mut self, v: bool) {
        self.app.borrow_mut().config.debug_depth_show = v;
    }

    #[getter]
    fn debug_light_cube_show(&self) -> bool {
        self.app.borrow().config.debug_light_cube_show
    }

    #[setter]
    fn set_debug_light_cube_show(&mut self, v: bool) {
        self.app.borrow_mut().config.debug_light_cube_show = v;
    }

    #[getter]
    fn title(&self) -> String {
        self.app.borrow().config.title.clone()
    }

    #[setter]
    fn set_title(&mut self, v: &str) {
        self.app.borrow_mut().config.title = v.to_string();
    }

    #[getter]
    fn width(&self) -> u32 {
        self.app.borrow().config.width
    }

    #[setter]
    fn set_width(&mut self, v: u32) {
        self.app.borrow_mut().config.width = v;
    }

    #[getter]
    fn height(&self) -> u32 {
        self.app.borrow().config.height
    }

    #[setter]
    fn set_height(&mut self, v: u32) {
        self.app.borrow_mut().config.height = v;
    }

    #[getter]
    pub fn background(&self) -> [Float; 4] {
        let v = self.app.borrow().config.background;
        [v.r as Float, v.g as Float, v.b as Float, v.a as Float]
    }

    #[setter]
    pub fn set_background(&mut self, v: [Float; 4]) {
        let c = &mut self.app.borrow_mut().config.background;
        c.r = v[0] as f64;
        c.g = v[1] as f64;
        c.b = v[2] as f64;
        c.a = v[3] as f64;
    }

    /// Font for every HUD: a name (`"Arial"`) or a path, or empty for the
    /// built-in. One that will not resolve warns and falls back. Startup only.
    #[getter]
    fn hud_font(&self) -> String {
        self.app.borrow().config.hud_font.clone()
    }

    #[setter]
    fn set_hud_font(&mut self, v: &str) {
        self.app.borrow_mut().config.hud_font = v.to_string();
    }

    /// The on-screen HUDs. Empty (the default) draws none.
    ///
    /// An alias for `app.simulation.huds`, not a second list: they are the
    /// same storage, so declaring them here and editing them there in
    /// `before_render` cannot drift apart. The objects handed back are the
    /// live ones -- setting `.text` on one takes effect on the next frame
    /// with no list to reassign.
    #[getter]
    fn huds(&self) -> Vec<Hud> {
        self.app
            .borrow()
            .simulation
            .borrow()
            .huds
            .iter()
            .map(|h| Hud { inner: h.clone() })  // Rc clone: the same object
            .collect()
    }

    #[setter]
    fn set_huds(&mut self, v: Vec<Hud>) {
        self.app.borrow().simulation.borrow_mut().huds =
            v.into_iter().map(|h| h.inner).collect();
    }

    /// Native fullscreen at startup. See `Config::fullscreen`.
    #[getter]
    fn fullscreen(&self) -> bool {
        self.app.borrow().config.fullscreen
    }

    #[setter]
    fn set_fullscreen(&mut self, v: bool) {
        self.app.borrow_mut().config.fullscreen = v;
    }

    #[getter]
    fn render_back_face(&self) -> bool {
        self.app.borrow().config.render_back_face
    }

    #[setter]
    fn set_render_back_face(&mut self, v: bool) {
        self.app.borrow_mut().config.render_back_face = v;
    }

    #[getter]
    fn sensitivity_move(&self) -> Float {
        self.app.borrow().config.sensitivity_move
    }

    #[setter]
    fn set_sensitivity_move(&mut self, v: Float) {
        self.app.borrow_mut().config.sensitivity_move = v;
    }

    #[getter]
    fn sensitivity_look(&self) -> Float {
        self.app.borrow().config.sensitivity_look
    }

    #[setter]
    fn set_sensitivity_look(&mut self, v: Float) {
        self.app.borrow_mut().config.sensitivity_look = v;
    }

    #[getter]
    fn sensitivity_rotate(&self) -> Float {
        self.app.borrow().config.sensitivity_rotate
    }

    #[setter]
    fn set_sensitivity_rotate(&mut self, v: Float) {
        self.app.borrow_mut().config.sensitivity_rotate = v;
    }

    #[getter]
    fn sensitivity_zoom(&self) -> Float {
        self.app.borrow().config.sensitivity_zoom
    }

    #[setter]
    fn set_sensitivity_zoom(&mut self, v: Float) {
        self.app.borrow_mut().config.sensitivity_zoom = v;
    }

    #[getter]
    pub fn color(&self) -> [Float; 4] {
        let v = self.app.borrow().config.color;
        [v.r as Float, v.g as Float, v.b as Float, v.a as Float]
    }

    #[setter]
    pub fn set_color(&mut self, v: [Float; 4]) {
        let c = &mut self.app.borrow_mut().config.color;
        c.r = v[0] as f64;
        c.g = v[1] as f64;
        c.b = v[2] as f64;
        c.a = v[3] as f64;
    }

    #[getter]
    fn color_mode(&self) -> u32 {
        self.app.borrow().config.color_mode
    }

    #[setter]
    fn set_color_mode(&mut self, v: u32) {
        self.app.borrow_mut().config.color_mode = v;
    }

    #[getter]
    fn extra(&self) -> u32 {
        self.app.borrow().config.extra
    }

    #[setter]
    fn set_extra(&mut self, v: u32) {
        self.app.borrow_mut().config.extra = v;
    }

    #[getter]
    fn srgb_mode(&self) -> u32 {
        self.app.borrow().config.srgb_mode
    }

    #[setter]
    fn set_srgb_mode(&mut self, v: u32) {
        self.app.borrow_mut().config.srgb_mode = v;
    }

    #[getter]
    fn gamma(&self) -> Float {
        self.app.borrow().config.gamma
    }

    #[setter]
    fn set_gamma(&mut self, v: Float) {
        self.app.borrow_mut().config.gamma = v;
    }

    #[getter]
    fn ambient_strength(&self) -> Float {
        self.app.borrow().config.ambient_strength
    }

    #[setter]
    fn set_ambient_strength(&mut self, v: Float) {
        self.app.borrow_mut().config.ambient_strength = v;
    }

    #[getter]
    pub fn light_color(&self) -> [Float; 4] {
        let v = self.app.borrow().config.light_color;
        [v.r as Float, v.g as Float, v.b as Float, v.a as Float]
    }

    #[setter]
    pub fn set_light_color(&mut self, v: [Float; 4]) {
        let c = &mut self.app.borrow_mut().config.light_color;
        c.r = v[0] as f64;
        c.g = v[1] as f64;
        c.b = v[2] as f64;
        c.a = v[3] as f64;
    }

    #[getter]
    fn light_cube_scale(&self) -> Float {
        self.app.borrow().config.light_cube_scale
    }

    #[setter]
    fn set_light_cube_scale(&mut self, v: Float) {
        self.app.borrow_mut().config.light_cube_scale = v;
    }

    /// Multisample anti-aliasing on the main pass: 1 (off), 2, 4 or 8.
    /// Takes effect when the window is created, so set it before `App.start`.
    #[getter]
    fn msaa(&self) -> u32 {
        self.app.borrow().config.msaa
    }

    #[setter]
    fn set_msaa(&mut self, v: u32) {
        self.app.borrow_mut().config.msaa = v;
    }

    #[getter]
    fn shadow_resolution(&self) -> u32 {
        self.app.borrow().config.shadow_resolution
    }

    #[setter]
    fn set_shadow_resolution(&mut self, v: u32) {
        self.app.borrow_mut().config.shadow_resolution = v;
    }

    // The three shadow constants below are Option: None (the default) means
    // "derive from the fitted light frustum", and assigning None again puts a
    // manually-pinned one back on automatic.
    #[getter]
    fn shadow_bias_scale(&self) -> Option<f32> {
        self.app.borrow().config.shadow_bias_scale
    }

    #[setter]
    fn set_shadow_bias_scale(&mut self, v: Option<f32>) {
        self.app.borrow_mut().config.shadow_bias_scale = v;
    }

    #[getter]
    fn shadow_bias_minimum(&self) -> Option<f32> {
        self.app.borrow().config.shadow_bias_minimum
    }

    #[setter]
    fn set_shadow_bias_minimum(&mut self, v: Option<f32>) {
        self.app.borrow_mut().config.shadow_bias_minimum = v;
    }

    #[getter]
    fn shadow_normal_offset_scale(&self) -> Option<f32> {
        self.app.borrow().config.shadow_normal_offset_scale
    }

    #[setter]
    fn set_shadow_normal_offset_scale(&mut self, v: Option<f32>) {
        self.app.borrow_mut().config.shadow_normal_offset_scale = v;
    }

    #[getter]
    fn wireframe_mode(&self) -> u32 {
        self.app.borrow().config.wireframe_mode
    }

    #[setter]
    fn set_wireframe_mode(&mut self, v: u32) {
        self.app.borrow_mut().config.wireframe_mode = v;
    }

    #[getter]
    fn wireframe_width(&self) -> f32 {
        self.app.borrow().config.wireframe_width
    }

    #[setter]
    fn set_wireframe_width(&mut self, v: f32) {
        self.app.borrow_mut().config.wireframe_width = v;
    }

    // `[Float; 4]`, not a Rust tuple: a tuple only extracts from a Python
    // tuple, so `[0.1, 0.1, 0.1, 1.0]` and `numpy.array([...])` were rejected
    // here while `background`, `color` and `light_color` -- which already use
    // an array -- accepted all three. An array extracts from any sequence.
    #[getter]
    fn wireframe_color(&self) -> [Float; 4] {
        let c = self.app.borrow().config.wireframe_color;
        [c.r as Float, c.g as Float, c.b as Float, c.a as Float]
    }

    #[setter]
    fn set_wireframe_color(&mut self, v: [Float; 4]) {
        self.app.borrow_mut().config.wireframe_color = wgpu::Color {
            r: v[0] as f64,
            g: v[1] as f64,
            b: v[2] as f64,
            a: v[3] as f64,
        };
    }

    #[getter]
    fn shadow_pcf(&self) -> u32 {
        self.app.borrow().config.shadow_pcf
    }

    #[setter]
    fn set_shadow_pcf(&mut self, v: u32) {
        self.app.borrow_mut().config.shadow_pcf = v;
    }

    #[getter]
    fn vsync(&self) -> bool {
        self.app.borrow().config.vsync
    }

    #[setter]
    fn set_vsync(&mut self, v: bool) {
        self.app.borrow_mut().config.vsync = v;
    }

    #[getter]
    fn export_sync(&self) -> bool {
        self.app.borrow().config.export_sync
    }

    #[setter]
    fn set_export_sync(&mut self, v: bool) {
        self.app.borrow_mut().config.export_sync = v;
    }

    #[getter]
    fn export_max_queued(&self) -> u32 {
        self.app.borrow().config.export_max_queued
    }

    #[setter]
    fn set_export_max_queued(&mut self, v: u32) {
        self.app.borrow_mut().config.export_max_queued = v;
    }

    #[getter]
    fn emulate_middle_button(&self) -> bool {
        self.app.borrow().config.emulate_middle_button
    }

    #[setter]
    fn set_emulate_middle_button(&mut self, v: bool) {
        self.app.borrow_mut().config.emulate_middle_button = v;
    }

    #[getter]
    fn access_shadow_map(&self) -> bool {
        self.app.borrow().config.access_shadow_map
    }

    #[setter]
    fn set_access_shadow_map(&mut self, v: bool) {
        self.app.borrow_mut().config.access_shadow_map = v;
    }

    /// Colour facets from `mesh.values` through `colormap`.
    ///
    /// Orthogonal to `color_mode`: with `0` the data map is shaded, with `1`
    /// it is flat, which is usually what a quantitative figure wants.
    #[getter]
    fn value_mode(&self) -> bool {
        self.app.borrow().config.value_mode
    }

    #[setter]
    fn set_value_mode(&mut self, v: bool) {
        self.app.borrow_mut().config.value_mode = v;
    }

    /// Bottom of the colour scale, or `None` to fit the data each frame.
    ///
    /// **Pin it for anything comparative.** An automatic range rescales
    /// between frames, so two images of the same scene are not on the same
    /// scale and the difference reads as physics rather than bookkeeping.
    #[getter]
    fn value_min(&self) -> Option<f32> {
        self.app.borrow().config.value_min
    }

    #[setter]
    fn set_value_min(&mut self, v: Option<f32>) {
        self.app.borrow_mut().config.value_min = v;
    }

    /// Top of the colour scale, or `None` to fit the data. See `value_min`.
    #[getter]
    fn value_max(&self) -> Option<f32> {
        self.app.borrow().config.value_max
    }

    #[setter]
    fn set_value_max(&mut self, v: Option<f32>) {
        self.app.borrow_mut().config.value_max = v;
    }

    /// Colour lookup table: a built-in name or an Nx3 array of RGB in 0..1.
    ///
    /// `"viridis"`, `"inferno"`, `"turbo"`, `"grey"`, or any matplotlib
    /// colormap passed straight through:
    ///
    ///     app.config.colormap = matplotlib.colormaps["magma"](numpy.linspace(0, 1, 256))[:, :3]
    ///
    /// Resampled to 256 entries, so any length works.
    #[setter]
    fn set_colormap(&mut self, v: &Bound<'_, PyAny>) -> PyResult<()> {
        let table = if let Ok(name) = v.extract::<String>() {
            crate::app::config::builtin_colormap(&name).ok_or_else(|| {
                pyo3::exceptions::PyValueError::new_err(format!(
                    "unknown colormap {name:?}: built-ins are viridis, inferno, turbo, grey;                      otherwise pass an Nx3 array"
                ))
            })?
        } else {
            let arr: numpy::borrow::PyReadonlyArray2<f32> = v.extract()?;
            let a = arr.as_array();
            if a.ncols() < 3 {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "colormap array must be Nx3 (or Nx4, alpha ignored)",
                ));
            }
            (0..a.nrows())
                .map(|i| [a[[i, 0]], a[[i, 1]], a[[i, 2]]])
                .collect()
        };
        self.app.borrow_mut().config.colormap = table;
        Ok(())
    }

    #[getter]
    fn export_hud(&self) -> bool {
        self.app.borrow().config.export_hud
    }

    #[setter]
    fn set_export_hud(&mut self, v: bool) {
        self.app.borrow_mut().config.export_hud = v;
    }

    #[getter]
    fn shadow_per_body(&self) -> bool {
        self.app.borrow().config.shadow_per_body
    }

    #[setter]
    fn set_shadow_per_body(&mut self, v: bool) {
        self.app.borrow_mut().config.shadow_per_body = v;
    }

    #[getter]
    fn export_dir(&self) -> String {
        self.app.borrow().config.export_dir.clone()
    }

    #[setter]
    fn set_export_dir(&mut self, v: &str) {
        self.app.borrow_mut().config.export_dir = v.to_string();
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self.app.borrow().config)
    }
}
