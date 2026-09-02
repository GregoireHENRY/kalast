use std::{cell::RefCell, rc::Rc};

use pyo3::prelude::*;

use crate::Float;

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

    #[getter]
    fn wireframe_color(&self) -> (f64, f64, f64, f64) {
        let c = self.app.borrow().config.wireframe_color;
        (c.r, c.g, c.b, c.a)
    }

    #[setter]
    fn set_wireframe_color(&mut self, v: (f64, f64, f64, f64)) {
        self.app.borrow_mut().config.wireframe_color = wgpu::Color {
            r: v.0,
            g: v.1,
            b: v.2,
            a: v.3,
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

    #[getter]
    fn export_hud(&self) -> bool {
        self.app.borrow().config.export_hud
    }

    #[setter]
    fn set_export_hud(&mut self, v: bool) {
        self.app.borrow_mut().config.export_hud = v;
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
