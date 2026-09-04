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
    /// Text colour, `(r, g, b, a)`.
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

/// The built-in colour tables, by name.
///
/// Four is deliberate rather than a full matplotlib set: any other colormap
/// can be passed as an array, so shipping more would be duplicating a
/// dependency the user already has. These are the ones worth having without
/// it -- three perceptually uniform, and grey for print.
///
/// Sampled at 8 anchor points and interpolated to 256 on upload, which is
/// within a colour step of the originals and keeps the table readable here.
pub fn builtin_colormap(name: &str) -> Option<Vec<[f32; 3]>> {
    let anchors: &[[f32; 3]] = match name.to_ascii_lowercase().as_str() {
        "viridis" => &[
            [0.267, 0.005, 0.329], [0.283, 0.141, 0.458], [0.254, 0.265, 0.530],
            [0.207, 0.372, 0.553], [0.164, 0.471, 0.558], [0.128, 0.567, 0.551],
            [0.135, 0.659, 0.518], [0.267, 0.749, 0.441],
        ],
        "inferno" => &[
            [0.001, 0.000, 0.014], [0.159, 0.044, 0.329], [0.354, 0.078, 0.432],
            [0.542, 0.144, 0.387], [0.730, 0.216, 0.302], [0.882, 0.352, 0.164],
            [0.969, 0.559, 0.035], [0.988, 0.809, 0.145],
        ],
        "turbo" => &[
            [0.190, 0.072, 0.232], [0.276, 0.418, 0.897], [0.147, 0.723, 0.795],
            [0.339, 0.907, 0.469], [0.699, 0.985, 0.212], [0.973, 0.816, 0.190],
            [0.977, 0.494, 0.115], [0.788, 0.148, 0.023],
        ],
        "grey" | "gray" => &[
            [0.0, 0.0, 0.0], [0.143, 0.143, 0.143], [0.286, 0.286, 0.286],
            [0.429, 0.429, 0.429], [0.571, 0.571, 0.571], [0.714, 0.714, 0.714],
            [0.857, 0.857, 0.857], [1.0, 1.0, 1.0],
        ],
        _ => return None,
    };
    Some(anchors.to_vec())
}

#[derive(Clone, Debug)]
pub struct Config {
    /// Print app lifecycle events: pause and camera-mode changes.
    pub debug_app: bool,
    /// Print window and GPU setup: chosen surface format, adapter and device
    /// features, and **the present modes the surface supports**.
    ///
    /// Worth enabling once on any new machine -- it is how the vsync cap that
    /// invalidated a whole benchmark was identified.
    pub debug_window: bool,
    /// Print per-mesh detail as meshes are uploaded.
    pub debug_window_mesh: bool,
    /// **Does nothing.** The field exists and is settable from Python, but no code
    /// reads it. Left as a placeholder.
    pub debug_simulation: bool,
    /// Draw the shadow/depth map as an overlay instead of leaving it offscreen.
    ///
    /// Only mirrors the main pass's depth at `msaa = 1`; above that the pass writes
    /// its own multisampled depth buffer and the debug view is not it.
    pub debug_depth_show: bool,
    /// Draw a cube at the light's position, so the Sun is visible.
    ///
    /// Size comes from `light_cube_scale`. The camera's far plane fits to scene
    /// bounds, which exclude the Sun, so seeing it usually means pinning
    /// `camera.projection.far` past it.
    pub debug_light_cube_show: bool,

    /// The OS window title.
    pub title: String,
    /// Initial window width in pixels.
    ///
    /// Also the render-target size and therefore the resolution of exported PNGs --
    /// though the exporter reads the *live* surface size, so resizing mid-run
    /// changes the size of subsequent exports.
    pub width: u32,
    /// Initial window height in pixels. See `width`.
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

    /// Colour the frame is cleared to, `(r, g, b, a)`.
    ///
    /// Accepts any 4-element sequence: tuple, list or `numpy.array`.
    pub background: wgpu::Color,
    /// Draw triangles facing away from the camera.
    ///
    /// Leave it off for closed shape models -- back faces are invisible there, so
    /// culling them is free performance. Measured on the full-resolution
    /// Didymos/Dimorphos meshes: culled against unculled differs in 5 pixels of
    /// 1,040,400, all on silhouette edges.
    ///
    /// Turn it on for geometry that is *not* closed -- open craters, clipped
    /// sections, single-sided surfaces -- where the inside of the shell must be
    /// visible from outside. Note the shading is single-sided regardless: normals
    /// are not flipped for back faces, so an underside is lit as though it were the
    /// top.
    ///
    /// The shadow pass stays unculled either way, so open geometry still casts
    /// correctly from whichever side faces the light.
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

    /// Multiplier for WASD movement speed.
    pub sensitivity_move: Float,
    /// Multiplier for mouse-look speed in WASD mode.
    pub sensitivity_look: Float,
    /// Multiplier for arcball orbit speed.
    pub sensitivity_rotate: Float,
    /// Multiplier for scroll and pinch zoom speed.
    pub sensitivity_zoom: Float,

    // See app/uniform.rs Globals struct for shader
    /// Flat colour used when `color_mode` is 2, `(r, g, b, a)`.
    pub color: wgpu::Color,
    /// What the fragment shader outputs.
    ///
    /// | | |
    /// |---|---|
    /// | 0 | vertex/instance colour, lit, with shadows (the default) |
    /// | 1 | raw vertex/instance colour, no lighting |
    /// | 2 | the flat `color` |
    /// | 3 | as 0 but with shadows disabled |
    pub color_mode: u32,
    /// Free integer passed through to the shader, for one-off experiments.
    pub extra: u32,

    /// 0 converts sRGB to linear before shading; 1 treats colours as already
    /// linear.
    pub srgb_mode: u32,
    /// Exponent used by the sRGB conversion when `srgb_mode` is 0.
    pub gamma: Float,

    /// Light added to every fragment regardless of shadowing.
    ///
    /// Deliberately tiny by default: a shadowed facet on an airless body receives
    /// almost nothing, and a visible ambient term would be inventing light that is
    /// not there.
    pub ambient_strength: f32,
    /// Colour of the Sun, `(r, g, b, a)`.
    pub light_color: wgpu::Color,
    /// Size of the debug light cube, in world units.
    ///
    /// Only drawn when `debug_light_cube_show` is on.
    pub light_cube_scale: Float,

    /// Side length of each square shadow map, in texels.
    ///
    /// The array is always allocated at all 8 layers, so the cost is
    /// `resolution^2 x 4 bytes x 8` -- 2.1 GB at the default 8192, 8.6 GB at 16384.
    /// Dropping to 2048 is the first thing to try when VRAM is tight or interactive
    /// frame times matter.
    ///
    /// It also feeds the automatic bias, which is expressed relative to one texel,
    /// so changing it changes the shadow bias with it.
    pub shadow_resolution: u32,
    /// Percentage-closer-filtering kernel *radius*: 0 is a single hardware 2x2
    /// comparison, N is a `(2N+1)^2` grid averaged.
    ///
    /// Cost grows quadratically and is per-fragment, so it scales with pixel count:
    /// `shadow_pcf = 4` costs +2.5 ms at 800x600 and +7.8 ms at 3024x1964. Benchmark
    /// it at the resolution you actually run.
    ///
    /// The normal offset scales with this, since an N-radius kernel reaches N texels
    /// away and a one-texel offset would let those taps flip. `shadow_pcf = 0` is
    /// bit-identical to the pre-scaling behaviour.
    pub shadow_pcf: u32,

    // None means "derive from the fitted light frustum and shadow_resolution"
    // (the default). These are scale-dependent -- values tuned for a 780 m
    // body seen from 25 km are wrong for any other scene -- so deriving them
    // per frame is both more correct and less work than hand-tuning. Set one
    // to pin it and leave the rest automatic; see app/frame.rs::fit_shadow.
    /// Push the sample along the surface normal before the shadow lookup, in
    /// world units. `None` fits it per frame from the layer's own texel size.
    ///
    /// Scaled by the PCF kernel radius, since an N-radius kernel reaches N
    /// texels away and a one-texel offset would let those taps flip.
    ///
    /// Pinning it is now worse than leaving it automatic: with per-body shadow
    /// layers a pinned value replaces the fitted one on *every* layer, and
    /// those differ by the ratio of the bodies' sizes -- 403x between Mars and
    /// Deimos in the same scene.
    pub shadow_normal_offset_scale: Option<f32>,

    /// Slope-dependent term of the depth-comparison bias. `None` fits it per
    /// frame. Combined in the shader as
    /// `max(shadow_bias_scale * k, shadow_bias_minimum)`.
    ///
    /// See `shadow_normal_offset_scale` for why pinning is discouraged.
    pub shadow_bias_scale: Option<f32>,
    /// Floor on the depth-comparison bias, for surfaces facing the light
    /// head-on. `None` fits it per frame.
    ///
    /// Measured to be the *ineffective* knob for the crater-floor PCF leak --
    /// auto, 1e-4 and 1e-3 all gave identical results, while the normal offset
    /// moved it 9x. Reach for that one first.
    pub shadow_bias_minimum: Option<f32>,

    // Wireframe overlay. 0 = shaded mesh only, 1 = wireframe only,
    // 2 = wireframe drawn over the shaded mesh.
    /// 0 shaded only, 1 wireframe only, 2 wireframe over the shaded mesh.
    ///
    /// Barycentric edge detection in the main fragment shader, so the overlay
    /// cannot z-fight. Needs a flattened mesh -- indexed meshes share vertices, so
    /// the barycentrics are meaningless and the CPU side warns once.
    pub wireframe_mode: u32,
    /// Wireframe colour, `(r, g, b, a)`; alpha is dropped.
    ///
    /// Mode 2 blends by edge coverage and is antialiased; mode 1 thresholds instead,
    /// because the pipeline blend state is REPLACE and a fractional alpha would be
    /// ignored.
    pub wireframe_color: wgpu::Color,
    // Line half-width in pixels. Screen-space, so thickness stays constant
    // regardless of distance or zoom.
    /// Wireframe half-width in screen pixels.
    pub wireframe_width: f32,

    // Present with vsync (wgpu Fifo) instead of uncapped (Immediate).
    // On a fast GPU vsync silently caps the render loop at the display
    // refresh rate, which makes render benchmarks measure the monitor
    // rather than the scene -- set false when timing. Falls back to the
    // surface's preferred mode if the requested one isn't supported.
    /// Cap the frame rate to the display refresh.
    ///
    /// **Set this to `False` for any performance measurement.** With it on, a GPU
    /// faster than the display simply reports the refresh rate: a 239 Hz panel
    /// measured exactly 239.46 it/s regardless of scene complexity, which made a
    /// 3.1M-facet scene look identical to a 100k one.
    pub vsync: bool,

    // Export frames synchronously: block the render loop on each frame's
    // GPU->CPU copy, PNG encode and disk write instead of handing them to
    // the background worker pool. Much slower, but bounds memory to a
    // single frame buffer -- the async path's queue is unbounded, so if
    // the render loop outruns the encoders (easy on a fast GPU) the
    // backlog grows without limit, each queued frame pinning a mapped
    // buffer, and anything still queued when the process dies is lost.
    /// Encode and write each exported frame on the render thread instead of a
    /// worker pool.
    ///
    /// Slower, but the file is on disk before the frame returns -- which is what a
    /// script needs if it exports and then reads the file immediately.
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
    /// How many frames may be waiting to be encoded before the render loop blocks.
    ///
    /// Unbounded, this reached 30 GB RSS growing at ~2 GB/s while only ~5.6 frames
    /// per second actually reached disk, and the loop still claimed 626 it/s --
    /// measuring queue growth rather than work done.
    pub export_max_queued: u32,

    // Directory frame exports (export/export_once) are written to, as
    // "{export_dir}/{N:06}.png", zero-padded so lexicographic and numeric
    // order agree. Override per-app (e.g. to a scratch
    // directory) to keep test/dev runs from colliding with real ones
    // sharing the default "out/frames".
    /// Directory exported frames are written to, as `{export_dir}/{N:06}.png`.
    ///
    /// **Redirect this for any test or benchmark run.** The default is shared, so a
    /// benchmark left at it writes into whatever a real run is using, and two
    /// exporters pointed at one directory race on the startup index scan as well as
    /// on cleanup.
    pub export_dir: String,

    /// Read the shadow map back per facet: computes solar occlusion for every
    /// body each frame, readable from `after_render` via
    /// `Simulation::facet_shadow`.
    ///
    /// Off by default because it is not free: the query costs ~1.6 ms per
    /// body at 100k facets and ~7.3 ms at 3.1M, dominated by the blocking
    /// readback. Turn it on for thermophysical or radiance work; leave it off
    /// when you only want images.
    /// Compute per-facet solar occlusion for every body, every frame.
    ///
    /// Read it back with `sim.facet_shadow(body)` from `after_render`. Leave it off
    /// unless something consumes it: it is a compute pass and a readback per frame.
    pub access_shadow_map: bool,

    /// Burn the HUD text into exported frames as well as drawing it on screen.
    ///
    /// Off by default, and that is the right default for a data product: the
    /// HUD is drawn onto the swapchain after the scene has been copied out, so
    /// exports carry the render alone. Turn it on for a screen-capture-style
    /// movie where the run state should be visible in the frames themselves --
    /// it costs one extra text pass, on exported frames only.
    /// Burn the HUD text into exported frames as well as the window.
    ///
    /// Off by default: the HUD is drawn straight onto the swapchain after the blit,
    /// so it stays out of `render_texture` and therefore out of exports. Turning it
    /// on adds a separate pass that draws it into the exported image too.
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

    /// Colour facets from `mesh.values` through `colormap` instead of from
    /// their vertex colour.
    ///
    /// Orthogonal to `color_mode`, which still decides whether the result is
    /// lit: with `color_mode = 0` the data map is shaded, with `1` it is flat,
    /// which is what a quantitative figure usually wants.
    pub value_mode: bool,

    /// Range the colormap spans, or `None` to fit the loaded values each
    /// frame.
    ///
    /// Automatic is the sane default for exploring, but pin it for anything
    /// comparative: an auto range silently rescales between frames, so two
    /// images of the same scene are not on the same colour scale and the
    /// difference between them reads as physics rather than as bookkeeping.
    pub value_min: Option<f32>,
    pub value_max: Option<f32>,

    /// Colour lookup table, 256 RGB entries in 0..1.
    ///
    /// Set by name (`"viridis"`, `"inferno"`, `"turbo"`, `"grey"`) or from any
    /// 256x3 array, so a matplotlib colormap can be handed over unchanged.
    /// Defaults to greyscale.
    pub colormap: Vec<[f32; 3]>,

    /// Treat alt + left-drag as a middle-drag, so the arcball can be orbited
    /// on hardware with no middle button. Blender calls the same setting
    /// "Emulate 3 Button Mouse". Defaults on for macOS, where a trackpad is
    /// the common case, and off elsewhere.
    /// Let `Option`/`Alt` + left-drag stand in for a middle-drag.
    ///
    /// Defaults on for macOS, matching Blender's "Emulate 3 Button Mouse". It exists
    /// because a trackpad has no middle button, which once made the arcball
    /// completely unusable there.
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

            value_mode: false,
            value_min: None,
            value_max: None,
            colormap: Vec::new(),

            emulate_middle_button: cfg!(target_os = "macos"),
        }
    }
}
