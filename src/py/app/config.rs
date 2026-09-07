use std::{cell::RefCell, rc::Rc};

use pyo3::prelude::*;

use crate::Float;


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
    /// The config itself, not the app that owns it. Holding the app would put
    /// every option behind the borrow `start()` takes for the whole run loop,
    /// which is what made `app.config.x = ...` inside `before_render` panic.
    pub config: Rc<RefCell<crate::app::config::Config>>,
    /// Only for `huds`, which live on the simulation.
    pub simulation: Rc<RefCell<crate::app::simulation::Simulation>>,
}

#[pymethods]
impl Config {
    #[getter]
    fn debug_app(&self) -> bool {
        self.config.borrow().debug_app
    }

    #[setter]
    fn set_debug_app(&mut self, v: bool) {
        self.config.borrow_mut().debug_app = v;
    }

    #[getter]
    fn debug_window(&self) -> bool {
        self.config.borrow().debug_window
    }

    #[setter]
    fn set_debug_window(&mut self, v: bool) {
        self.config.borrow_mut().debug_window = v;
    }

    #[getter]
    fn debug_window_mesh(&self) -> bool {
        self.config.borrow().debug_window_mesh
    }

    #[setter]
    fn set_debug_window_mesh(&mut self, v: bool) {
        self.config.borrow_mut().debug_window_mesh = v;
    }

    #[getter]
    fn debug_simulation(&self) -> bool {
        self.config.borrow().debug_simulation
    }

    #[setter]
    fn set_debug_simulation(&mut self, v: bool) {
        self.config.borrow_mut().debug_simulation = v;
    }

    #[getter]
    fn debug_depth_show(&self) -> bool {
        self.config.borrow().debug_depth_show
    }

    #[setter]
    fn set_debug_depth_show(&mut self, v: bool) {
        self.config.borrow_mut().debug_depth_show = v;
    }

    #[getter]
    fn debug_light_cube_show(&self) -> bool {
        self.config.borrow().debug_light_cube_show
    }

    #[setter]
    fn set_debug_light_cube_show(&mut self, v: bool) {
        self.config.borrow_mut().debug_light_cube_show = v;
    }

    #[getter]
    fn title(&self) -> String {
        self.config.borrow().title.clone()
    }

    #[setter]
    fn set_title(&mut self, v: &str) {
        self.config.borrow_mut().title = v.to_string();
    }

    #[getter]
    fn width(&self) -> u32 {
        self.config.borrow().width
    }

    #[setter]
    fn set_width(&mut self, v: u32) {
        self.config.borrow_mut().width = v;
    }

    #[getter]
    fn height(&self) -> u32 {
        self.config.borrow().height
    }

    #[setter]
    fn set_height(&mut self, v: u32) {
        self.config.borrow_mut().height = v;
    }

    #[getter]
    pub fn background(&self) -> [Float; 4] {
        let v = self.config.borrow().background;
        [v.r as Float, v.g as Float, v.b as Float, v.a as Float]
    }

    #[setter]
    pub fn set_background(&mut self, v: [Float; 4]) {
        let c = &mut self.config.borrow_mut().background;
        c.r = v[0] as f64;
        c.g = v[1] as f64;
        c.b = v[2] as f64;
        c.a = v[3] as f64;
    }

    /// Font for every HUD: a name (`"Arial"`) or a path, or empty for the
    /// built-in. One that will not resolve warns and falls back. Startup only.
    #[getter]
    fn hud_font(&self) -> String {
        self.config.borrow().hud_font.clone()
    }

    #[setter]
    fn set_hud_font(&mut self, v: &str) {
        self.config.borrow_mut().hud_font = v.to_string();
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

    /// Native fullscreen at startup. See `Config::fullscreen`.
    #[getter]
    fn fullscreen(&self) -> bool {
        self.config.borrow().fullscreen
    }

    #[setter]
    fn set_fullscreen(&mut self, v: bool) {
        self.config.borrow_mut().fullscreen = v;
    }

    #[getter]
    fn render_back_face(&self) -> bool {
        self.config.borrow().render_back_face
    }

    #[setter]
    fn set_render_back_face(&mut self, v: bool) {
        self.config.borrow_mut().render_back_face = v;
    }

    #[getter]
    fn sensitivity_move(&self) -> Float {
        self.config.borrow().sensitivity_move
    }

    #[setter]
    fn set_sensitivity_move(&mut self, v: Float) {
        self.config.borrow_mut().sensitivity_move = v;
    }

    #[getter]
    fn sensitivity_look(&self) -> Float {
        self.config.borrow().sensitivity_look
    }

    #[setter]
    fn set_sensitivity_look(&mut self, v: Float) {
        self.config.borrow_mut().sensitivity_look = v;
    }

    #[getter]
    fn sensitivity_rotate(&self) -> Float {
        self.config.borrow().sensitivity_rotate
    }

    #[setter]
    fn set_sensitivity_rotate(&mut self, v: Float) {
        self.config.borrow_mut().sensitivity_rotate = v;
    }

    #[getter]
    fn sensitivity_zoom(&self) -> Float {
        self.config.borrow().sensitivity_zoom
    }

    #[setter]
    fn set_sensitivity_zoom(&mut self, v: Float) {
        self.config.borrow_mut().sensitivity_zoom = v;
    }

    #[getter]
    pub fn color(&self) -> [Float; 4] {
        let v = self.config.borrow().color;
        [v.r as Float, v.g as Float, v.b as Float, v.a as Float]
    }

    #[setter]
    pub fn set_color(&mut self, v: [Float; 4]) {
        let c = &mut self.config.borrow_mut().color;
        c.r = v[0] as f64;
        c.g = v[1] as f64;
        c.b = v[2] as f64;
        c.a = v[3] as f64;
    }

    #[getter]
    fn color_mode(&self) -> u32 {
        self.config.borrow().color_mode
    }

    #[setter]
    fn set_color_mode(&mut self, v: u32) {
        self.config.borrow_mut().color_mode = v;
    }

    #[getter]
    fn extra(&self) -> u32 {
        self.config.borrow().extra
    }

    #[setter]
    fn set_extra(&mut self, v: u32) {
        self.config.borrow_mut().extra = v;
    }

    #[getter]
    fn srgb_mode(&self) -> u32 {
        self.config.borrow().srgb_mode
    }

    #[setter]
    fn set_srgb_mode(&mut self, v: u32) {
        self.config.borrow_mut().srgb_mode = v;
    }

    #[getter]
    fn gamma(&self) -> Float {
        self.config.borrow().gamma
    }

    #[setter]
    fn set_gamma(&mut self, v: Float) {
        self.config.borrow_mut().gamma = v;
    }

    #[getter]
    fn ambient_strength(&self) -> Float {
        self.config.borrow().ambient_strength
    }

    #[setter]
    fn set_ambient_strength(&mut self, v: Float) {
        self.config.borrow_mut().ambient_strength = v;
    }

    #[getter]
    pub fn light_color(&self) -> [Float; 4] {
        let v = self.config.borrow().light_color;
        [v.r as Float, v.g as Float, v.b as Float, v.a as Float]
    }

    #[setter]
    pub fn set_light_color(&mut self, v: [Float; 4]) {
        let c = &mut self.config.borrow_mut().light_color;
        c.r = v[0] as f64;
        c.g = v[1] as f64;
        c.b = v[2] as f64;
        c.a = v[3] as f64;
    }

    #[getter]
    fn light_cube_scale(&self) -> Float {
        self.config.borrow().light_cube_scale
    }

    #[setter]
    fn set_light_cube_scale(&mut self, v: Float) {
        self.config.borrow_mut().light_cube_scale = v;
    }

    /// Multisample anti-aliasing on the main pass: 1 (off), 2, 4 or 8.
    /// Takes effect when the window is created, so set it before `App.start`.
    #[getter]
    fn msaa(&self) -> u32 {
        self.config.borrow().msaa
    }

    #[setter]
    fn set_msaa(&mut self, v: u32) {
        self.config.borrow_mut().msaa = v;
    }

    #[getter]
    fn shadow_resolution(&self) -> u32 {
        self.config.borrow().shadow_resolution
    }

    #[setter]
    fn set_shadow_resolution(&mut self, v: u32) {
        self.config.borrow_mut().shadow_resolution = v;
    }

    // The three shadow constants below are Option: None (the default) means
    // "derive from the fitted light frustum", and assigning None again puts a
    // manually-pinned one back on automatic.
    #[getter]
    fn shadow_bias_scale(&self) -> Option<f32> {
        self.config.borrow().shadow_bias_scale
    }

    #[setter]
    fn set_shadow_bias_scale(&mut self, v: Option<f32>) {
        self.config.borrow_mut().shadow_bias_scale = v;
    }

    #[getter]
    fn shadow_bias_minimum(&self) -> Option<f32> {
        self.config.borrow().shadow_bias_minimum
    }

    #[setter]
    fn set_shadow_bias_minimum(&mut self, v: Option<f32>) {
        self.config.borrow_mut().shadow_bias_minimum = v;
    }

    #[getter]
    fn shadow_normal_offset_scale(&self) -> Option<f32> {
        self.config.borrow().shadow_normal_offset_scale
    }

    #[setter]
    fn set_shadow_normal_offset_scale(&mut self, v: Option<f32>) {
        self.config.borrow_mut().shadow_normal_offset_scale = v;
    }

    #[getter]
    fn wireframe_mode(&self) -> u32 {
        self.config.borrow().wireframe_mode
    }

    #[setter]
    fn set_wireframe_mode(&mut self, v: u32) {
        self.config.borrow_mut().wireframe_mode = v;
    }

    #[getter]
    fn wireframe_width(&self) -> f32 {
        self.config.borrow().wireframe_width
    }

    #[setter]
    fn set_wireframe_width(&mut self, v: f32) {
        self.config.borrow_mut().wireframe_width = v;
    }

    // `[Float; 4]`, not a Rust tuple: a tuple only extracts from a Python
    // tuple, so `[0.1, 0.1, 0.1, 1.0]` and `numpy.array([...])` were rejected
    // here while `background`, `color` and `light_color` -- which already use
    // an array -- accepted all three. An array extracts from any sequence.
    #[getter]
    fn wireframe_color(&self) -> [Float; 4] {
        let c = self.config.borrow().wireframe_color;
        [c.r as Float, c.g as Float, c.b as Float, c.a as Float]
    }

    #[setter]
    fn set_wireframe_color(&mut self, v: [Float; 4]) {
        self.config.borrow_mut().wireframe_color = wgpu::Color {
            r: v[0] as f64,
            g: v[1] as f64,
            b: v[2] as f64,
            a: v[3] as f64,
        };
    }

    #[getter]
    fn shadow_pcf(&self) -> u32 {
        self.config.borrow().shadow_pcf
    }

    #[setter]
    fn set_shadow_pcf(&mut self, v: u32) {
        self.config.borrow_mut().shadow_pcf = v;
    }

    #[getter]
    fn vsync(&self) -> bool {
        self.config.borrow().vsync
    }

    #[setter]
    fn set_vsync(&mut self, v: bool) {
        self.config.borrow_mut().vsync = v;
    }

    #[getter]
    fn export_sync(&self) -> bool {
        self.config.borrow().export_sync
    }

    #[setter]
    fn set_export_sync(&mut self, v: bool) {
        self.config.borrow_mut().export_sync = v;
    }

    #[getter]
    fn export_max_queued(&self) -> u32 {
        self.config.borrow().export_max_queued
    }

    #[setter]
    fn set_export_max_queued(&mut self, v: u32) {
        self.config.borrow_mut().export_max_queued = v;
    }

    #[getter]
    fn emulate_middle_button(&self) -> bool {
        self.config.borrow().emulate_middle_button
    }

    #[setter]
    fn set_emulate_middle_button(&mut self, v: bool) {
        self.config.borrow_mut().emulate_middle_button = v;
    }

    #[getter]
    fn access_shadow_map(&self) -> bool {
        self.config.borrow().access_shadow_map
    }

    #[setter]
    fn set_access_shadow_map(&mut self, v: bool) {
        self.config.borrow_mut().access_shadow_map = v;
    }

    /// Draw the colour scale. Off by default.
    #[getter]
    fn colorbar(&self) -> bool {
        self.config.borrow().colorbar.enabled
    }

    #[setter]
    fn set_colorbar(&mut self, v: bool) {
        self.config.borrow_mut().colorbar.enabled = v;
    }

    /// Which of the nine anchors the bar sits at.
    ///
    /// Orientation follows: `middle-left`/`middle-right` give a vertical bar,
    /// anything else horizontal. Override with `colorbar_vertical`.
    #[getter]
    fn colorbar_anchor(&self) -> String {
        self.config.borrow().colorbar.anchor.name().to_string()
    }

    #[setter]
    fn set_colorbar_anchor(&mut self, v: &str) -> PyResult<()> {
        let a = crate::app::config::HudAnchor::parse(v).ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err(format!("unknown anchor {v:?}"))
        })?;
        self.config.borrow_mut().colorbar.anchor = a;
        Ok(())
    }

    /// Force the orientation, or `None` to follow the anchor.
    #[getter]
    fn colorbar_vertical(&self) -> Option<bool> {
        self.config.borrow().colorbar.vertical
    }

    #[setter]
    fn set_colorbar_vertical(&mut self, v: Option<bool>) {
        self.config.borrow_mut().colorbar.vertical = v;
    }

    /// Caption above the bar, e.g. `"Surface temperature (K)"`.
    #[getter]
    fn colorbar_label(&self) -> String {
        self.config.borrow().colorbar.label.clone()
    }

    #[setter]
    fn set_colorbar_label(&mut self, v: &str) {
        self.config.borrow_mut().colorbar.label = v.to_string();
    }

    /// Long axis of the bar, pixels.
    #[getter]
    fn colorbar_length(&self) -> f32 {
        self.config.borrow().colorbar.length
    }

    #[setter]
    fn set_colorbar_length(&mut self, v: f32) {
        self.config.borrow_mut().colorbar.length = v;
    }

    /// Short axis of the bar, pixels.
    #[getter]
    fn colorbar_thickness(&self) -> f32 {
        self.config.borrow().colorbar.thickness
    }

    #[setter]
    fn set_colorbar_thickness(&mut self, v: f32) {
        self.config.borrow_mut().colorbar.thickness = v;
    }

    /// Inset from the anchor, pixels.
    #[getter]
    fn colorbar_x(&self) -> f32 {
        self.config.borrow().colorbar.x
    }

    #[setter]
    fn set_colorbar_x(&mut self, v: f32) {
        self.config.borrow_mut().colorbar.x = v;
    }

    /// Inset from the anchor, pixels.
    #[getter]
    fn colorbar_y(&self) -> f32 {
        self.config.borrow().colorbar.y
    }

    #[setter]
    fn set_colorbar_y(&mut self, v: f32) {
        self.config.borrow_mut().colorbar.y = v;
    }

    /// Roughly how many numbered ticks, rounded to a readable step.
    #[getter]
    fn colorbar_ticks(&self) -> usize {
        self.config.borrow().colorbar.ticks
    }

    #[setter]
    fn set_colorbar_ticks(&mut self, v: usize) {
        self.config.borrow_mut().colorbar.ticks = v;
    }

    /// Tick and caption size in pixels.
    #[getter]
    fn colorbar_text_size(&self) -> f32 {
        self.config.borrow().colorbar.text_size
    }

    #[setter]
    fn set_colorbar_text_size(&mut self, v: f32) {
        self.config.borrow_mut().colorbar.text_size = v;
    }

    /// Tick and caption colour, `(r, g, b, a)`.
    #[getter]
    fn colorbar_text_color(&self) -> [f32; 4] {
        self.config.borrow().colorbar.text_color
    }

    #[setter]
    fn set_colorbar_text_color(&mut self, v: [f32; 4]) {
        self.config.borrow_mut().colorbar.text_color = v;
    }

    /// Reference axes: `"off"`, `"box"` (MATLAB), `"panes"` (matplotlib),
    /// `"gizmo"` (three labelled arrows at the origin) or `"blender"`
    /// (ground grid, Z line and gizmo).
    #[getter]
    fn axes(&self) -> String {
        self.config.borrow().axes.name().to_string()
    }

    #[setter]
    fn set_axes(&mut self, v: &str) -> PyResult<()> {
        let style = crate::app::axes::AxesStyle::parse(v).ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err(format!(
                "unknown axes style {v:?}: expected off, box, panes, gizmo or blender"
            ))
        })?;
        self.config.borrow_mut().axes = style;
        Ok(())
    }

    /// Colour of the axis lines and grid, `(r, g, b)`.
    #[getter]
    fn axes_color(&self) -> [f32; 3] {
        self.config.borrow().axes_color
    }

    #[setter]
    fn set_axes_color(&mut self, v: [f32; 3]) {
        self.config.borrow_mut().axes_color = v;
    }

    /// Roughly how many ticks per axis.
    ///
    /// Approximate on purpose: the step is rounded to 1, 2 or 5 times a power
    /// of ten, so the count lands near this rather than on it. Ticks at
    /// 0.0347 would hit the number exactly and be unreadable.
    #[getter]
    fn axes_ticks(&self) -> usize {
        self.config.borrow().axes_ticks
    }

    #[setter]
    fn set_axes_ticks(&mut self, v: usize) {
        self.config.borrow_mut().axes_ticks = v;
    }

    /// Appended to every tick label, e.g. `" km"`.
    ///
    /// The renderer knows the mesh is 0.437 across but not whether that is
    /// metres or kilometres, so the unit has to come from here.
    #[getter]
    fn axes_unit(&self) -> String {
        self.config.borrow().axes_unit.clone()
    }

    #[setter]
    fn set_axes_unit(&mut self, v: &str) {
        self.config.borrow_mut().axes_unit = v.to_string();
    }

    /// Tick label size in pixels.
    #[getter]
    fn axes_label_size(&self) -> f32 {
        self.config.borrow().axes_label_size
    }

    #[setter]
    fn set_axes_label_size(&mut self, v: f32) {
        self.config.borrow_mut().axes_label_size = v;
    }

    /// Tick label colour, `(r, g, b, a)`.
    #[getter]
    fn axes_label_color(&self) -> [f32; 4] {
        self.config.borrow().axes_label_color
    }

    #[setter]
    fn set_axes_label_color(&mut self, v: [f32; 4]) {
        self.config.borrow_mut().axes_label_color = v;
    }

    /// Bottom of the colour scale, or `None` to fit the data each frame.
    ///
    /// **Pin it for anything comparative.** An automatic range rescales
    /// between frames, so two images of the same scene are not on the same
    /// scale and the difference reads as physics rather than bookkeeping.
    #[getter]
    fn value_min(&self) -> Option<f32> {
        self.config.borrow().value_min
    }

    #[setter]
    fn set_value_min(&mut self, v: Option<f32>) {
        self.config.borrow_mut().value_min = v;
    }

    /// Top of the colour scale, or `None` to fit the data. See `value_min`.
    #[getter]
    fn value_max(&self) -> Option<f32> {
        self.config.borrow().value_max
    }

    #[setter]
    fn set_value_max(&mut self, v: Option<f32>) {
        self.config.borrow_mut().value_max = v;
    }

    /// Colour lookup table: a built-in name or an Nx3 array of RGB in 0..1.
    ///
    /// `"viridis"`, `"inferno"`, `"turbo"`, `"grey"`, or any matplotlib
    /// colormap passed straight through:
    ///
    ///     app.config.colormap = matplotlib.colormaps["magma"](numpy.linspace(0, 1, 256))[:, :3]
    ///
    /// Resampled to 256 entries, so any length works.
    /// The colour table in use, as a 256x3 array.
    #[getter]
    fn colormap<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, numpy::PyArray2<f32>>> {
        let rows: Vec<Vec<f32>> = self
            .config
            .borrow()
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
        self.config.borrow_mut().colormap = table;
        Ok(())
    }

    #[getter]
    fn export_hud(&self) -> bool {
        self.config.borrow().export_hud
    }

    #[setter]
    fn set_export_hud(&mut self, v: bool) {
        self.config.borrow_mut().export_hud = v;
    }

    #[getter]
    fn shadow_per_body(&self) -> bool {
        self.config.borrow().shadow_per_body
    }

    #[setter]
    fn set_shadow_per_body(&mut self, v: bool) {
        self.config.borrow_mut().shadow_per_body = v;
    }

    #[getter]
    fn export_dir(&self) -> String {
        self.config.borrow().export_dir.clone()
    }

    #[setter]
    fn set_export_dir(&mut self, v: &str) {
        self.config.borrow_mut().export_dir = v.to_string();
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self.config.borrow())
    }
}
