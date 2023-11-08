use crate::prelude::*;
use crate::python::*;
use serde::{Serialize, Deserialize};

pub const WINDOW_WIDTH: usize = 500;
pub const WINDOW_HEIGHT: usize = 500;
// pub const WINDOW_RATIO: Float = WINDOW_WIDTH as Float / WINDOW_HEIGHT as Float;

/// List of possible colormaps.
#[pyclass]
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Colormap {
    #[serde(rename = "cividis")]
    Cividis,

    #[serde(rename = "plasma")]
    Plasma,

    #[serde(rename = "magma")]
    Magma,

    #[serde(rename = "inferno")]
    Inferno,

    #[serde(rename = "gray")]
    Gray,
}

impl Default for Colormap {
    fn default() -> Self {
        Self::Gray
    }
}

impl From<&str> for Colormap {
    fn from(value: &str) -> Self {
        match value {
            "cividis" => Self::Cividis,
            "plasma" => Self::Plasma,
            "magma" => Self::Magma,
            "inferno" => Self::Inferno,
            "gray" => Self::Gray,
            _ => panic!("unknown colormap name."),
        }
    }
}

/// State of the Window that is tracked.
///
/// These are the things that the user do not change manually but they do change for some other reasons.
/// We keep track of them here.
#[derive(Clone, Debug)]
pub struct WindowState {
    pub difference_in_height_from_ratio_after_resize: i32,
    pub pre_record: bool,
    pub pause: bool,
    pub export_quit: bool,
}

impl Default for WindowState {
    fn default() -> Self {
        Self {
            difference_in_height_from_ratio_after_resize: 0,
            pre_record: false,
            pause: false,
            export_quit: false,
        }
    }
}

impl WindowState {
    pub fn toggle_pause(&mut self) -> bool {
        self.pause = !self.pause;
        self.pause
    }
    
    pub fn export_quit(&mut self) {
        self.export_quit = true;
    }
}

/// Settings of the window.
#[pyclass]
#[derive(Clone, Debug)]
pub struct WindowSettings {
    pub debug: bool,
    pub gl_version: (u8, u8),
    pub width: usize,
    pub height: usize,
    pub fullscreen: FullscreenType,
    pub multisampling: Option<u8>,
    pub background_color: Vec3,
    pub directional_light_color: Vec3,
    pub ambient_light_color: Vec3,
    pub debug_depth_map: bool,
    pub shadows: bool,
    pub shadow_dpi: usize,
    pub light_offset: Float,
    pub show_light: bool,
    pub bias_acne: Float,
    pub face_culling: bool,
    pub front_face_culling_for_peter_panning: bool,
    pub colormap: Colormap,
    pub colormap_bounds: (Float, Float),
    pub fov: Float,
    pub ortho: bool,
    pub far_factor: Float,
    pub close_distance: Float,
    pub draw_normals: bool,
    pub normals_magnitude: Float,
    pub camera_speed: Float,
    pub wireframe: bool,
    pub wireframe_width: Float,

    dpi: usize,
}

impl Default for WindowSettings {
    fn default() -> Self {
        Self {
            debug: false,
            gl_version: (4, 1),
            width: WINDOW_WIDTH,
            height: WINDOW_HEIGHT,
            fullscreen: FullscreenType::Off,
            dpi: 100,
            multisampling: Some(16),
            background_color: vec3(0.0, 0.0, 0.0),
            directional_light_color: vec3(1.0, 1.0, 1.0),
            ambient_light_color: vec3(0.0, 0.0, 0.0),
            debug_depth_map: false,
            shadows: false,
            shadow_dpi: 100,
            light_offset: 1.0,
            show_light: false,
            bias_acne: 1e-2,
            face_culling: true,
            front_face_culling_for_peter_panning: false,
            colormap: Colormap::default(),
            colormap_bounds: (0.0, 1.0),
            fov: 30.0,
            ortho: false,
            far_factor: 1.0,
            close_distance: 1e-3,
            draw_normals: false,
            normals_magnitude: 0.1,
            camera_speed: 0.5,
            wireframe: false,
            wireframe_width: 1.0,
        }
    }
}

impl WindowSettings {
    pub fn high_dpi(&mut self) {
        self.dpi = 200;
    }

    pub fn toggle_fullscreen(&mut self) -> FullscreenType {
        if self.fullscreen != FullscreenType::True {
            self.fullscreen = FullscreenType::True;
        } else {
            self.fullscreen = FullscreenType::Off;
        }
        self.fullscreen
    }

    pub fn toggle_fullscreen_windowed(&mut self) -> FullscreenType {
        if self.fullscreen != FullscreenType::Desktop {
            self.fullscreen = FullscreenType::Desktop;
        } else {
            self.fullscreen = FullscreenType::Off;
        }
        self.fullscreen
    }

    pub fn is_high_dpi(&self) -> bool {
        self.dpi == 200
    }

    pub fn width_viewport(&self) -> usize {
        self.width * self.dpi / 100
    }

    pub fn height_viewport(&self) -> usize {
        self.height * self.dpi / 100
    }

    pub fn width_viewport_depthmap(&self) -> usize {
        self.width_viewport() * self.shadow_dpi / 100
    }

    pub fn height_viewport_depthmap(&self) -> usize {
        self.height_viewport() * self.shadow_dpi / 100
    }

    pub fn aspect_ratio(&self) -> Float {
        self.width as Float / self.height as Float
    }
}

#[pymethods]
impl WindowSettings {
    #[new]
    fn new_py() -> Self {
        Self::default()
    }

    #[classmethod]
    #[pyo3(name = "default")]
    #[allow(unused)]
    fn default_py(cls: &PyType) -> Self {
        Self::default()
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self)
    }

    #[getter]
    fn get_width(&self) -> usize {
        self.width
    }

    #[getter]
    fn get_height(&self) -> usize {
        self.height
    }

    #[getter]
    fn get_dpi(&self) -> usize {
        self.dpi
    }

    #[getter]
    fn get_multisampling(&self) -> Option<u8> {
        self.multisampling
    }

    #[getter]
    fn get_debug_depth_map(&self) -> bool {
        self.debug_depth_map
    }

    #[getter]
    fn get_shadow_dpi(&self) -> usize {
        self.shadow_dpi
    }

    #[getter]
    fn get_show_light(&self) -> bool {
        self.show_light
    }

    #[getter]
    fn get_bias_acne(&mut self) -> Float {
        self.bias_acne
    }

    #[getter]
    fn get_face_culling(&self) -> bool {
        self.face_culling
    }

    #[getter]
    fn get_front_face_culling_for_peter_panning(&self) -> bool {
        self.front_face_culling_for_peter_panning
    }

    #[getter]
    fn get_colormap(&self) -> Colormap {
        self.colormap
    }

    #[getter]
    fn get_colormap_bounds(&self) -> (Float, Float) {
        self.colormap_bounds
    }

    #[setter]
    fn set_width(&mut self, width: usize) {
        self.width = width;
    }

    #[setter]
    fn set_height(&mut self, height: usize) {
        self.height = height;
    }

    #[setter]
    fn set_dpi(&mut self, dpi: usize) {
        self.dpi = dpi;
    }

    #[setter]
    fn set_multisampling(&mut self, multisampling: Option<u8>) {
        self.multisampling = multisampling;
    }

    #[setter]
    fn set_debug_depth_map(&mut self, debug: bool) {
        self.debug_depth_map = debug;
    }

    #[setter]
    fn set_shadow_dpi(&mut self, dpi: usize) {
        self.shadow_dpi = dpi;
    }

    #[setter]
    fn set_show_light(&mut self, show: bool) {
        self.show_light = show;
    }

    #[setter]
    fn set_bias_acne(&mut self, bias: Float) {
        self.bias_acne = bias;
    }

    #[setter]
    fn set_face_culling(&mut self, face_culling: bool) {
        self.face_culling = face_culling;
    }

    #[setter]
    fn set_front_face_culling_for_peter_panning(&mut self, face_culling: bool) {
        self.front_face_culling_for_peter_panning = face_culling;
    }

    #[setter]
    fn set_colormap(&mut self, map: Colormap) {
        self.colormap = map;
    }

    #[setter]
    fn set_colormap_bounds(&mut self, bounds: (Float, Float)) {
        self.colormap_bounds = bounds;
    }

    #[pyo3(name = "width_viewport")]
    fn width_viewport_py(&self) -> usize {
        self.width_viewport()
    }

    #[pyo3(name = "height_viewport")]
    fn height_viewport_py(&self) -> usize {
        self.height_viewport()
    }

    #[pyo3(name = "width_viewport_depthmap")]
    fn width_viewport_depthmap_py(&self) -> usize {
        self.width_viewport_depthmap()
    }

    #[pyo3(name = "height_viewport_depthmap")]
    fn height_viewport_depthmap_py(&self) -> usize {
        self.height_viewport_depthmap()
    }

    #[pyo3(name = "ratio")]
    fn ratio_py(&self) -> Float {
        self.aspect_ratio()
    }
}
