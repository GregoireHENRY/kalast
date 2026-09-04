use crate::Float;

/// Which corner a HUD is measured from.
///
/// `x`/`y` on `Hud` are an inset *from* the anchor, so the same offset means
/// "8 px in from my corner" whichever corner that is, and a bottom-anchored
/// HUD does not move when the window is resized.
///
/// There is deliberately no `Custom`: the top-left anchor *is* the origin, so
/// `Hud(text, x=200, y=120)` already places a HUD at exactly (200, 120). A
/// separate variant for that would have been a second name for the default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HudAnchor {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl HudAnchor {
    /// Parsed from Python, where these are plain strings. Hyphen, underscore
    /// and space all work, since all three get typed.
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().replace(['_', ' '], "-").as_str() {
            "top-left" => Some(Self::TopLeft),
            "top-right" => Some(Self::TopRight),
            "bottom-left" => Some(Self::BottomLeft),
            "bottom-right" => Some(Self::BottomRight),
            _ => None,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::TopLeft => "top-left",
            Self::TopRight => "top-right",
            Self::BottomLeft => "bottom-left",
            Self::BottomRight => "bottom-right",
        }
    }
}

/// One HUD overlay: a template, where it sits, and how it looks.
#[derive(Debug, Clone)]
pub struct Hud {
    /// Template text; see `Config::huds` for the placeholders.
    pub text: String,
    pub anchor: HudAnchor,
    /// Inset from the anchor in pixels, or absolute position for `Custom`.
    pub x: f32,
    pub y: f32,
    /// Font size in pixels.
    pub size: f32,
    pub color: [f32; 4],
}

impl Hud {
    pub fn new(text: &str) -> Self {
        Self {
            text: text.to_string(),
            anchor: HudAnchor::TopLeft,
            x: 8.0,
            y: 6.0,
            size: 18.0,
            color: [1.0, 1.0, 1.0, 0.9],
        }
    }
}

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

    /// Font for the HUD: a **name** or a **path**, or empty for the built-in
    /// DejaVu Sans.
    ///
    /// `"Arial"`, `"arial"`, `"Times New Roman"` and
    /// `"/Library/Fonts/Arial.ttf"` all work. Anything that exists on disk is
    /// treated as a path; anything else is looked up by name in the
    /// platform's font directories.
    ///
    /// **Matching is on the filename, not the family name inside the font.**
    /// Reading real family names needs a font-database dependency, which an
    /// overlay does not justify; filenames cover the names people type. A
    /// family whose file is named differently -- "Helvetica Neue" living in
    /// `HelveticaNeue.ttc` -- resolves, since punctuation and case are
    /// ignored, but one named nothing like its family will not.
    ///
    /// One font for all HUDs: each additional font needs its own glyph cache
    /// and draw, and per-HUD fonts are not worth that for an overlay. Per-HUD
    /// *size* is free by comparison and lives on `Hud::size`.
    ///
    /// A path that cannot be read or parsed warns once and falls back to the
    /// built-in font, rather than leaving the run with no HUD at all.
    ///
    /// Startup only: the glyph cache is built with the window.
    pub hud_font: String,

    /// Open the window in native fullscreen (borderless, current monitor).
    ///
    /// On macOS this is the same mode the green button gives -- its own
    /// Space -- which is worth knowing because it is not equivalent to a
    /// maximised window: the compositor hands out drawables differently
    /// there, and a stall that only appears fullscreen will not reproduce
    /// maximised.
    ///
    /// Startup only: applied when the window is created.
    pub fullscreen: bool,

    pub background: wgpu::Color,
    pub render_back_face: bool,

    /// Multisample anti-aliasing for the main render pass: 1 (off), 2, 4 or 8.
    ///
    /// Geometry edges are the whole point here. Every silhouette in this
    /// renderer is a science measurement -- a limb, a terminator, a body's
    /// apparent diameter -- and at one sample per pixel each of those is
    /// quantised to whole pixels, which both looks wrong beside other tools
    /// and biases any centroid or radius fitted from an exported frame.
    ///
    /// Only the main pass is multisampled. The shadow map, the facet-id and
    /// hemicube passes stay single-sampled on purpose: they carry ids and
    /// depths, not colour, and averaging those across samples would be
    /// meaningless. Exports are unaffected in shape or size -- the pass
    /// resolves into the same single-sample target that was always exported.
    ///
    /// Counts the adapter does not support fall back to 4, then to 1. Note
    /// that `debug_depth_show` only mirrors the main pass's depth at 1: above
    /// that the pass writes its own multisampled depth buffer instead.
    pub msaa: u32,

    pub sensitivity_move: Float,
    pub sensitivity_look: Float,
    pub sensitivity_rotate: Float,
    pub sensitivity_zoom: Float,

    // See app/uniform.rs Globals struct for shader
    pub color: wgpu::Color,
    pub color_mode: u32,
    pub extra: u32,

    pub srgb_mode: u32,
    pub gamma: Float,

    pub ambient_strength: f32,
    pub light_color: wgpu::Color,
    pub light_cube_scale: Float,

    pub shadow_resolution: u32,
    pub shadow_pcf: u32,

    // None means "derive from the fitted light frustum and shadow_resolution"
    // (the default). These are scale-dependent -- values tuned for a 780 m
    // body seen from 25 km are wrong for any other scene -- so deriving them
    // per frame is both more correct and less work than hand-tuning. Set one
    // to pin it and leave the rest automatic; see app/frame.rs::fit_shadow.
    pub shadow_normal_offset_scale: Option<f32>,
    pub shadow_bias_scale: Option<f32>,
    pub shadow_bias_minimum: Option<f32>,

    // Wireframe overlay. 0 = shaded mesh only, 1 = wireframe only,
    // 2 = wireframe drawn over the shaded mesh.
    pub wireframe_mode: u32,
    pub wireframe_color: wgpu::Color,
    // Line half-width in pixels. Screen-space, so thickness stays constant
    // regardless of distance or zoom.
    pub wireframe_width: f32,

    // Present with vsync (wgpu Fifo) instead of uncapped (Immediate).
    // On a fast GPU vsync silently caps the render loop at the display
    // refresh rate, which makes render benchmarks measure the monitor
    // rather than the scene -- set false when timing. Falls back to the
    // surface's preferred mode if the requested one isn't supported.
    pub vsync: bool,

    // Export frames synchronously: block the render loop on each frame's
    // GPU->CPU copy, PNG encode and disk write instead of handing them to
    // the background worker pool. Much slower, but bounds memory to a
    // single frame buffer -- the async path's queue is unbounded, so if
    // the render loop outruns the encoders (easy on a fast GPU) the
    // backlog grows without limit, each queued frame pinning a mapped
    // buffer, and anything still queued when the process dies is lost.
    pub export_sync: bool,

    // Upper bound on frames queued for export but not yet written, before
    // export_frame blocks the render loop waiting for the encoders to catch
    // up. Each outstanding frame pins a mapped GPU buffer of one frame, so
    // this caps export memory at roughly export_max_queued * width * height
    // * 4 bytes. 0 disables the bound entirely (the original behaviour):
    // measured at 1020x1020 with vsync off, an unbounded queue grew ~2 GB/s
    // and reached 30 GB in 16 s, because the render loop outran the PNG
    // encoders by ~100x. Blocking is what makes the reported frame rate
    // honest -- it becomes the rate frames actually reach disk.
    pub export_max_queued: u32,

    // Directory frame exports (export/export_once) are written to, as
    // "{export_dir}/{N:06}.png", zero-padded so lexicographic and numeric
    // order agree. Override per-app (e.g. to a scratch
    // directory) to keep test/dev runs from colliding with real ones
    // sharing the default "out/frames".
    pub export_dir: String,

    /// Read the shadow map back per facet: computes solar occlusion for every
    /// body each frame, readable from `after_render` via
    /// `Simulation::facet_shadow`.
    ///
    /// Off by default because it is not free: the query costs ~1.6 ms per
    /// body at 100k facets and ~7.3 ms at 3.1M, dominated by the blocking
    /// readback. Turn it on for thermophysical or radiance work; leave it off
    /// when you only want images.
    pub access_shadow_map: bool,

    /// Burn the HUD text into exported frames as well as drawing it on screen.
    ///
    /// Off by default, and that is the right default for a data product: the
    /// HUD is drawn onto the swapchain after the scene has been copied out, so
    /// exports carry the render alone. Turn it on for a screen-capture-style
    /// movie where the run state should be visible in the frames themselves --
    /// it costs one extra text pass, on exported frames only.
    pub export_hud: bool,

    /// Fit a shadow map per body instead of one fitted to the whole scene.
    ///
    /// On by default, because one shared map is fitted to the scene's extent
    /// and a small body beside a large one then gets almost no texels -- 6 km
    /// Deimos next to 3,396 km Mars is the case that forced this. Each layer
    /// is aimed at its own body and sized to it, while its depth range still
    /// spans the scene, so mutual shadowing is unaffected: anything between
    /// the Sun and a body still casts into that body's layer.
    ///
    /// Costs one shadow pass per body. Turn it off to get the old single
    /// scene-fitted map back, which is only worth doing to reproduce older
    /// output or when every body is a similar size.
    pub shadow_per_body: bool,

    /// Treat alt + left-drag as a middle-drag, so the arcball can be orbited
    /// on hardware with no middle button. Blender calls the same setting
    /// "Emulate 3 Button Mouse". Defaults on for macOS, where a trackpad is
    /// the common case, and off elsewhere.
    pub emulate_middle_button: bool,
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
            hud_font: String::new(),
            fullscreen: false,
            render_back_face: false,

            sensitivity_move: 1.0,
            sensitivity_look: 1.0,
            sensitivity_rotate: 1.0,
            sensitivity_zoom: 1.0,

            color: wgpu::Color::WHITE,
            color_mode: 0,
            extra: 0,

            srgb_mode: 0,
            gamma: 2.2,

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
            shadow_normal_offset_scale: None,
            shadow_bias_scale: None,
            shadow_bias_minimum: None,

            msaa: 4,

            wireframe_mode: 0,
            wireframe_color: wgpu::Color::BLACK,
            wireframe_width: 1.0,

            vsync: true,
            export_sync: false,
            export_max_queued: 64,

            export_dir: "out/frames".to_string(),

            access_shadow_map: false,

            export_hud: false,

            shadow_per_body: true,

            emulate_middle_button: cfg!(target_os = "macos"),
        }
    }
}
