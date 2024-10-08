use crate::{util::*, KEY_BACKWARD, KEY_FORWARD, KEY_LEFT, KEY_RIGHT, SENSITIVITY};

use sdl2::{keyboard::Keycode, video::FullscreenType};
use serde::{Deserialize, Serialize};

pub const WINDOW_WIDTH: usize = 640;
pub const WINDOW_HEIGHT: usize = 480;

/// List of possible colormaps.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Colormap {
    #[serde(rename = "viridis")]
    Viridis,

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
            "viridis" => Self::Viridis,
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
    pub quit: bool,
    pub export_quit: bool,
    pub keys_down: Vec<Keycode>,
}

impl Default for WindowState {
    fn default() -> Self {
        Self {
            difference_in_height_from_ratio_after_resize: 0,
            pre_record: false,
            pause: false,
            quit: false,
            export_quit: false,
            keys_down: vec![],
        }
    }
}

impl WindowState {
    pub fn toggle_pause(&mut self) -> bool {
        self.pause = !self.pause;
        match self.pause {
            true => println!("Paused."),
            false => println!("Unpaused."),
        }

        self.pause
    }

    pub fn export_quit(&mut self) {
        self.export_quit = true;
    }
}

/// Settings of the window.
#[derive(Clone, Debug)]
pub struct WindowSettings {
    pub debug: bool,
    pub gl_version: (u8, u8),
    pub width: usize,
    pub height: usize,
    pub fullscreen: FullscreenType,
    pub multisampling: Option<u8>,
    pub vsync: bool,
    pub background_color: Vec3,
    pub directional_light_color: Vec3,
    pub ambient_light_color: Vec3,
    pub debug_depth_map: bool,
    pub shadows: bool,
    pub shadow_dpi: usize,
    pub light_position: Vec3,
    pub show_light: bool,
    pub bias_acne: Float,
    pub face_culling: bool,
    pub front_face_culling_for_peter_panning: bool,
    pub colormap: Colormap,
    pub colormap_bounds: (Float, Float),
    pub draw_normals: bool,
    pub normals_magnitude: Float,
    pub wireframe: bool,
    pub wireframe_width: Float,
    pub sensitivity: Float,
    pub forward: Keycode,
    pub left: Keycode,
    pub backward: Keycode,
    pub right: Keycode,
    pub touchpad_controls: bool,

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
            multisampling: Some(16),
            vsync: true,
            background_color: vec3(0.0, 0.0, 0.0),
            directional_light_color: vec3(1.0, 1.0, 1.0),
            ambient_light_color: vec3(0.0, 0.0, 0.0),
            debug_depth_map: false,
            shadows: false,
            shadow_dpi: 100,
            light_position: Vec3::x(),
            show_light: false,
            bias_acne: 1e-2,
            face_culling: true,
            front_face_culling_for_peter_panning: false,
            colormap: Colormap::default(),
            colormap_bounds: (0.0, 1.0),
            draw_normals: false,
            normals_magnitude: 0.1,
            wireframe: false,
            wireframe_width: 1.0,
            sensitivity: SENSITIVITY,
            forward: KEY_FORWARD,
            left: KEY_LEFT,
            backward: KEY_BACKWARD,
            right: KEY_RIGHT,
            touchpad_controls: false,

            dpi: 100,
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

    pub fn toggle_debug(&mut self) -> bool {
        self.debug = !self.debug;
        self.debug
    }
}
