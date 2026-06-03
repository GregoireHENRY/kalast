use crate::Float;

#[derive(Clone, Debug)]
pub struct Config {
    pub debug_app: bool,
    pub debug_window: bool,
    pub debug_window_mesh: bool,
    pub debug_simulation: bool,
    pub debug_depth_show: bool,
    pub debug_light_cube_show: bool,

    pub title: String,
    pub width: u32,
    pub height: u32,

    pub background: wgpu::Color,
    pub render_back_face: bool,

    pub sensitivity_move: Float,
    pub sensitivity_look: Float,
    pub sensitivity_rotate: Float,
    pub sensitivity_zoom: Float,

    // See app/uniform.rs Globals struct
    pub global_color: wgpu::Color,
    pub global_color_mode: u32,
    pub global_extra: u32,

    pub ambient_strength: f32,
    pub light_color: wgpu::Color,
    pub light_cube_scale: Float,

    pub shadow_resolution: u32,
    pub shadow_pcf: u32,
    pub shadow_normal_offset_scale: f32,
    pub shadow_bias_scale: f32,
    pub shadow_bias_minimum: f32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            debug_app: false,
            debug_window: false,
            debug_window_mesh: false,
            debug_simulation: false,
            debug_depth_show: false,
            debug_light_cube_show: false,

            title: "kalast".to_string(),
            width: 800,
            height: 600,

            background: wgpu::Color::BLACK,
            render_back_face: false,

            sensitivity_move: 1.0,
            sensitivity_look: 1.0,
            sensitivity_rotate: 1.0,
            sensitivity_zoom: 1.0,

            global_color: wgpu::Color::WHITE,
            global_color_mode: 0,
            global_extra: 0,

            ambient_strength: 0.002,
            light_color: wgpu::Color::WHITE,
            // light_target: Vec3::new(0.0, 0.0, 0.0),
            // light_up: Vec3::new(0.0, 0.0, 1.0),
            // light_side: 10.0,
            // light_znear: 0.1,
            // light_zfar: 100.0,
            light_cube_scale: 0.25,

            shadow_resolution: 8192,
            shadow_pcf: 0,
            shadow_normal_offset_scale: 2e-4,
            shadow_bias_scale: 1e-5,
            shadow_bias_minimum: 1e-5,
        }
    }
}
