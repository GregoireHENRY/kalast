use crate::util::*;

use sdl2::video::FullscreenType;
use serde::{Deserialize, Serialize};

pub const WINDOW_WIDTH: usize = 640;
pub const WINDOW_HEIGHT: usize = 480;

/// List of possible colormaps.
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
