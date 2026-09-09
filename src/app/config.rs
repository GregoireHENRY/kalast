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
    TopCenter,
    TopRight,
    MiddleLeft,
    MiddleCenter,
    MiddleRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

/// Horizontal alignment of the text within its block.
///
/// Separate from the anchor because the two answer different questions: the
/// anchor is *where the block sits*, alignment is *how lines sit inside it*.
/// A bottom-centre block of three lines still has to decide whether those
/// lines are ragged-right or centred on each other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HAlign {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VAlign {
    Top,
    Center,
    Bottom,
}

impl HAlign {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "left" => Some(Self::Left),
            "center" | "centre" | "centered" | "centred" | "middle" => Some(Self::Center),
            "right" => Some(Self::Right),
            _ => None,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Center => "center",
            Self::Right => "right",
        }
    }
}

impl VAlign {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "top" => Some(Self::Top),
            "center" | "centre" | "centered" | "centred" | "middle" => Some(Self::Center),
            "bottom" => Some(Self::Bottom),
            _ => None,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Top => "top",
            Self::Center => "center",
            Self::Bottom => "bottom",
        }
    }
}

impl HudAnchor {
    /// Parsed from Python, where these are plain strings. Hyphen, underscore
    /// and space all work, since all three get typed.
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().replace(['_', ' '], "-").as_str() {
            "top-left" => Some(Self::TopLeft),
            "top-center" | "top-centre" | "top" => Some(Self::TopCenter),
            "top-right" => Some(Self::TopRight),
            "middle-left" | "left" => Some(Self::MiddleLeft),
            "middle-center" | "middle-centre" | "center" | "centre" | "middle" => {
                Some(Self::MiddleCenter)
            }
            "middle-right" | "right" => Some(Self::MiddleRight),
            "bottom-left" => Some(Self::BottomLeft),
            "bottom-center" | "bottom-centre" | "bottom" => Some(Self::BottomCenter),
            "bottom-right" => Some(Self::BottomRight),
            _ => None,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::TopLeft => "top-left",
            Self::TopCenter => "top-center",
            Self::TopRight => "top-right",
            Self::MiddleLeft => "middle-left",
            Self::MiddleCenter => "middle-center",
            Self::MiddleRight => "middle-right",
            Self::BottomLeft => "bottom-left",
            Self::BottomCenter => "bottom-center",
            Self::BottomRight => "bottom-right",
        }
    }

    /// The alignment a HUD gets when it does not set one: text reads outward
    /// from the edge it is anchored to, which is what looks right without
    /// being asked for.
    pub fn default_align(&self) -> (HAlign, VAlign) {
        let h = match self {
            Self::TopLeft | Self::MiddleLeft | Self::BottomLeft => HAlign::Left,
            Self::TopCenter | Self::MiddleCenter | Self::BottomCenter => HAlign::Center,
            Self::TopRight | Self::MiddleRight | Self::BottomRight => HAlign::Right,
        };
        let v = match self {
            Self::TopLeft | Self::TopCenter | Self::TopRight => VAlign::Top,
            Self::MiddleLeft | Self::MiddleCenter | Self::MiddleRight => VAlign::Center,
            Self::BottomLeft | Self::BottomCenter | Self::BottomRight => VAlign::Bottom,
        };
        (h, v)
    }
}

/// What a colorbar is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorbarSource {
    /// The `mesh.values` colormap, labelled in the data's own units. Reads the
    /// same lookup table the surface does, so the two cannot disagree.
    Values,
    /// The diffuse shading itself, labelled 0..1 -- `ambient + cos(i) * visibility`,
    /// i.e. normalised direct insolation including shadowing.
    ///
    /// Not radiance and not temperature, and it carries the `ambient_strength`
    /// floor. Label it accordingly.
    Lighting,
}

/// A colour scale drawn over the render.
///
/// Horizontal or vertical is inferred from the anchor -- a bar anchored to a
/// side is vertical, one anchored top or bottom is horizontal -- since that is
/// the only orientation that fits in either place. `vertical` overrides it.
#[derive(Debug, Clone)]
pub struct Colorbar {
    pub enabled: bool,
    pub anchor: HudAnchor,
    /// Inset from the anchor, pixels.
    /// :range: 0.0..=1.0
    pub x: f32,
    /// :range: 0.0..=1.0
    pub y: f32,
    /// Long and short axis of the bar, pixels.
    /// :range: 0.0..=1.0
    pub length: f32,
    /// :range: 0.0..=0.5
    pub thickness: f32,
    /// `None` infers from the anchor.
    pub vertical: Option<bool>,
    /// Caption, e.g. `"Surface temperature (K)"`.
    pub label: String,
    /// Roughly how many numbered ticks; rounded to a readable step as the axes
    /// are.
    /// :range: 1..=20
    pub ticks: usize,
    /// :range: 4.0..=64.0
    pub text_size: f32,
    pub text_color: [f32; 4],
    /// Outline drawn around the strip, so it reads as a scale rather than as
    /// part of the scene when it sits over a dark body.
    pub border: bool,
}

impl Default for Colorbar {
    fn default() -> Self {
        Self {
            enabled: false,
            anchor: HudAnchor::BottomCenter,
            x: 0.0,
            y: 48.0,
            length: 320.0,
            thickness: 18.0,
            vertical: None,
            label: String::new(),
            ticks: 5,
            text_size: 13.0,
            text_color: [0.92, 0.92, 0.92, 1.0],
            border: true,
        }
    }
}

impl Colorbar {
    /// Horizontal at top/bottom, vertical at the sides, unless overridden.
    pub fn is_vertical(&self) -> bool {
        self.vertical.unwrap_or(matches!(
            self.anchor,
            HudAnchor::MiddleLeft | HudAnchor::MiddleRight
        ))
    }
}

/// One HUD overlay: a template, where it sits, and how it looks.
#[derive(Debug, Clone)]
pub struct Hud {
    /// Template text; see `Config::huds` for the placeholders.
    pub text: String,

    /// Used in place of `text` while it is set, whatever `text` says.
    ///
    /// For taking a HUD off a script and giving it to a person. A callback
    /// that assigns `text` every iteration owns it completely -- an edit
    /// made anywhere else is gone by the next frame, and a *driven* script
    /// keeps assigning even while the simulation is paused, since pausing
    /// stops the iteration counter and not a `while` loop the script owns.
    /// There is nowhere to make an edit stand except beside `text`.
    ///
    /// Set by typing in the editor's HUDs section, cleared by its release
    /// button. The script goes on writing `text`, harmlessly, and gets the
    /// HUD back the moment this is `None` again.
    pub pin: Option<String>,
    pub anchor: HudAnchor,
    /// Inset from the anchor in pixels. With the default top-left anchor
    /// this is simply the position, since that anchor is the origin.
    pub x: f32,
    pub y: f32,
    /// Font size in pixels.
    pub size: f32,
    /// Text colour, `(r, g, b, a)`.
    pub color: [f32; 4],
    /// Alignment within the block, or `None` to follow the anchor.
    pub align_h: Option<HAlign>,
    pub align_v: Option<VAlign>,
}

impl Hud {
    pub fn new(text: &str) -> Self {
        Self {
            text: text.to_string(),
            pin: None,
            anchor: HudAnchor::TopLeft,
            x: 8.0,
            y: 6.0,
            size: 18.0,
            color: [1.0, 1.0, 1.0, 0.9],
            align_h: None,
            align_v: None,
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
/// Resample a colour table to `n` entries, interpolating between them.
///
/// Interpolated, not nearest: nearest turned the 8-anchor built-ins into 8
/// visible bands -- tolerable on a shaded body where lighting hides it,
/// obvious on a colour scale, which is a flat ramp with nothing to hide
/// behind.
///
/// Shared with the GPU upload so a table fetched from Python is the one the
/// renderer will use, rather than a second implementation that can drift.
pub fn resample_colormap(table: &[[f32; 3]], n: usize) -> Vec<[f32; 3]> {
    if table.is_empty() || n == 0 {
        return Vec::new();
    }
    let len = table.len();
    (0..n)
        .map(|i| {
            let t = if n > 1 { i as f32 / (n - 1) as f32 } else { 0.0 };
            let x = t * (len - 1) as f32;
            let lo = x.floor() as usize;
            let hi = (lo + 1).min(len - 1);
            let f = x - lo as f32;
            let (a, b) = (table[lo], table[hi]);
            [
                a[0] + (b[0] - a[0]) * f,
                a[1] + (b[1] - a[1]) * f,
                a[2] + (b[2] - a[2]) * f,
            ]
        })
        .collect()
}

pub fn builtin_colormap(name: &str) -> Option<Vec<[f32; 3]>> {
    // Sampled from matplotlib at 24 even points including both ends, then
    // interpolated to the 256-entry LUT.
    //
    // The previous tables were eight points that stopped short of t = 1:
    // viridis ended at its mid-green rather than its yellow, leaving the top
    // quarter of every scale unreachable and the rendered colours off by up
    // to 0.73 out of 0..1 against matplotlib. Now under 0.015 for all four.
    let anchors: &[[f32; 3]] = match name.to_ascii_lowercase().as_str() {
        "viridis" => &[
            [0.267, 0.005, 0.329],     [0.280, 0.068, 0.392],     [0.283, 0.126, 0.445],
            [0.278, 0.180, 0.487],     [0.265, 0.233, 0.517],     [0.247, 0.283, 0.536],
            [0.226, 0.331, 0.547],     [0.205, 0.376, 0.554],     [0.184, 0.422, 0.557],
            [0.167, 0.464, 0.558],     [0.150, 0.504, 0.557],     [0.135, 0.545, 0.554],
            [0.123, 0.585, 0.547],     [0.121, 0.626, 0.533],     [0.140, 0.666, 0.513],
            [0.186, 0.705, 0.485],     [0.260, 0.745, 0.444],     [0.344, 0.780, 0.397],
            [0.440, 0.811, 0.341],     [0.546, 0.838, 0.276],     [0.658, 0.860, 0.203],
            [0.773, 0.878, 0.131],     [0.886, 0.892, 0.095],     [0.993, 0.906, 0.144],
        ],
        "inferno" => &[
            [0.001, 0.000, 0.014],     [0.022, 0.017, 0.097],     [0.071, 0.040, 0.196],
            [0.136, 0.047, 0.300],     [0.211, 0.037, 0.379],     [0.284, 0.044, 0.417],
            [0.354, 0.067, 0.431],     [0.423, 0.093, 0.433],     [0.497, 0.119, 0.424],
            [0.566, 0.144, 0.408],     [0.634, 0.169, 0.384],     [0.701, 0.198, 0.351],
            [0.764, 0.233, 0.311],     [0.822, 0.275, 0.266],     [0.874, 0.327, 0.217],
            [0.916, 0.387, 0.165],     [0.952, 0.462, 0.105],     [0.974, 0.537, 0.048],
            [0.986, 0.616, 0.026],     [0.987, 0.698, 0.088],     [0.977, 0.782, 0.186],
            [0.959, 0.867, 0.311],     [0.947, 0.943, 0.475],     [0.988, 0.998, 0.645],
        ],
        "turbo" => &[
            [0.190, 0.072, 0.232],     [0.236, 0.197, 0.524],     [0.265, 0.317, 0.747],
            [0.277, 0.431, 0.903],     [0.271, 0.540, 0.989],     [0.220, 0.649, 0.984],
            [0.145, 0.754, 0.905],     [0.095, 0.845, 0.793],     [0.127, 0.917, 0.676],
            [0.248, 0.964, 0.543],     [0.412, 0.993, 0.398],     [0.574, 0.998, 0.277],
            [0.695, 0.976, 0.213],     [0.805, 0.925, 0.205],     [0.899, 0.851, 0.220],
            [0.965, 0.764, 0.228],     [0.995, 0.653, 0.196],     [0.990, 0.529, 0.144],
            [0.958, 0.400, 0.088],     [0.905, 0.287, 0.045],     [0.832, 0.199, 0.021],
            [0.737, 0.125, 0.008],     [0.619, 0.064, 0.004],     [0.480, 0.016, 0.011],
        ],
        "grey" | "gray" => &[
            [0.000, 0.000, 0.000],     [0.043, 0.043, 0.043],     [0.086, 0.086, 0.086],
            [0.129, 0.129, 0.129],     [0.173, 0.173, 0.173],     [0.216, 0.216, 0.216],
            [0.259, 0.259, 0.259],     [0.302, 0.302, 0.302],     [0.349, 0.349, 0.349],
            [0.392, 0.392, 0.392],     [0.435, 0.435, 0.435],     [0.478, 0.478, 0.478],
            [0.522, 0.522, 0.522],     [0.565, 0.565, 0.565],     [0.608, 0.608, 0.608],
            [0.651, 0.651, 0.651],     [0.698, 0.698, 0.698],     [0.741, 0.741, 0.741],
            [0.784, 0.784, 0.784],     [0.827, 0.827, 0.827],     [0.871, 0.871, 0.871],
            [0.914, 0.914, 0.914],     [0.957, 0.957, 0.957],     [1.000, 1.000, 1.000],
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
    /// Time each GPU pass with timestamp queries, into `sim.gpu_timings()`.
    ///
    /// Off by default: the queries themselves are nearly free, but reading them
    /// back costs a buffer map per frame, and nothing needs it unless someone is
    /// asking where a frame goes. Silently inert where the adapter has no
    /// `TIMESTAMP_QUERY` -- `sim.gpu_timings()` returns an empty dict there.
    ///
    /// :group: Debug
    /// Draw the shadow/depth map as an overlay instead of leaving it offscreen.
    ///
    /// Only mirrors the main pass's depth at `msaa = 1`; above that the pass writes
    /// its own multisampled depth buffer and the debug view is not it.
    pub gpu_timing: bool,

    /// Count what each body actually drew, with occlusion queries.
    ///
    /// The Visibility panel otherwise tests bounding boxes against the
    /// frustum, so "visible" means "could be seen", and a body wholly behind
    /// another still counts. This draws each body's box after the scene, with
    /// the depth test on and depth writes off, and asks the GPU how many
    /// samples survived. Zero means it put nothing on screen.
    ///
    /// Off by default: it costs a readback every frame, for a diagnostic.
    /// A box is a conservative stand-in for its body, so this can still call
    /// a body visible when only its box is -- the same direction the frustum
    /// test errs in.
    /// :group: Debug
    pub occlusion_queries: bool,
    pub debug_depth_show: bool,
    /// Draw a cube at the light's position, so the Sun is visible.
    ///
    /// Size comes from `light_cube_scale`. `debug_light_cube_fit` is on by
    /// default, which is what makes it *visible* rather than merely drawn:
    /// the camera's far plane is fitted to the bodies, and the Sun is well
    /// outside them.
    pub debug_light_cube_show: bool,

    /// Fit the camera's frustum around the light cube too, not just the
    /// bodies.
    ///
    /// What makes `debug_light_cube_show` show something, so it defaults to
    /// **on**: without it the Sun sits past the fitted far plane and is
    /// clipped away, and asking to see the light and being shown nothing is
    /// not a useful default. Nothing happens either way unless the cube is
    /// being drawn.
    ///
    /// Turn it off to keep the frustum fitted to the geometry being studied
    /// while the cube is on -- for a figure where the cube is out of frame
    /// but the framing must not change.
    ///
    /// The **camera** only. The light's own frustum stays fitted to the
    /// bodies, because it is what the shadow map covers: stretching it to the
    /// Sun would spend the whole map on empty space and leave the bodies a
    /// few texels across.
    ///
    /// Costs the depth range: a far plane at the Sun rather than at the
    /// body's edge is a much longer near-to-far span, so depth precision
    /// drops. For looking at where the light is, not for a figure.
    pub debug_light_cube_fit: bool,

    /// The OS window title.
    pub title: String,
    /// Render size in physical pixels -- the *image*, not the window.
    ///
    /// `0` means "follow the window", which is what a terminal run wants and
    /// what every script got when there was only one pair of these. Set it to
    /// pin the render independently: a 4K export from a small window, or a
    /// fixed frame size while the editor's viewport panel is dragged about.
    ///
    /// Everything about the image follows this -- the camera's aspect ratio,
    /// where axis ticks project, where the colour bar sits, and what an
    /// exported frame measures.
    /// :label: image width
    /// :range: 0..=7680
    pub width: u32,
    /// :label: image height
    /// :range: 0..=4320
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
    /// :range: 1..=8
    pub msaa: u32,

    /// Multiplier for WASD movement speed.
    /// :range: 0.1..=5.0
    pub sensitivity_move: Float,
    /// Multiplier for mouse-look speed in WASD mode.
    /// :range: 0.1..=5.0
    pub sensitivity_look: Float,
    /// Multiplier for arcball orbit speed.
    /// :range: 0.1..=5.0
    pub sensitivity_rotate: Float,
    /// Multiplier for scroll and pinch zoom speed.
    /// :range: 0.1..=5.0
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
    /// :range: 0..=3
    pub color_mode: u32,
    /// Free integer passed through to the shader, for one-off experiments.
    /// :range: 0..=10
    pub extra: u32,

    /// 0 converts sRGB to linear before shading; 1 treats colours as already
    /// linear.
    /// :range: 0..=2
    pub srgb_mode: u32,
    /// Exponent used by the sRGB conversion when `srgb_mode` is 0.
    /// :range: 0.1..=4.0
    pub gamma: Float,

    /// Light added to every fragment regardless of shadowing.
    ///
    /// Deliberately tiny by default: a shadowed facet on an airless body receives
    /// almost nothing, and a visible ambient term would be inventing light that is
    /// not there.
    /// :range: 0.0..=1.0
    pub ambient_strength: f32,
    /// Colour of the Sun, `(r, g, b, a)`.
    pub light_color: wgpu::Color,
    /// Size of the debug light cube, in world units.
    ///
    /// Only drawn when `debug_light_cube_show` is on.
    /// :range: 0.0..=5.0
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
    /// :range: 512..=16384
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
    /// :range: 0..=16
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
    /// Draw each facet's index at its centre.
    ///
    /// For working out *which* facet a number in a data product refers to,
    /// without counting round a mesh by hand. Off by default, and capped by
    /// `facet_labels_max`: a label per facet is a text draw per facet, and a
    /// shape model has millions of them.
    ///
    /// Only facets turned towards the camera are labelled. The text has no
    /// depth test -- it is drawn over the frame -- so labelling the far side
    /// of a body would print numbers on top of the surface hiding them. On a
    /// concave shape, a facet behind another that faces the same way can
    /// still show through.
    ///
    /// :group: Selection
    pub facet_labels: bool,

    /// Most facets to label before giving up, per body.
    ///
    /// A guard rather than a preference: turning labels on with a 3.1M-facet
    /// body would queue three million text draws and stop the frame dead.
    ///
    /// :range: 0..=20000
    /// :group: Selection
    pub facet_labels_max: u32,

    /// Size of a facet label, in pixels.
    /// :range: 4.0..=48.0
    /// :group: Selection
    pub facet_label_size: f32,

    /// Colour of a facet label, `(r, g, b, a)`.
    /// :group: Selection
    pub facet_label_color: [f32; 4],

    /// Colour a facet takes when it is selected, `(r, g, b, a)`.
    ///
    /// Selecting writes this onto the facet's own vertices and marks them
    /// colour-mode 1, which the shader honours for that facet alone -- so a
    /// picked facet is unlit and this colour while the rest of the body keeps
    /// its shading. Deselecting puts back what was there.
    ///
    /// :group: Selection
    pub selection_color: wgpu::Color,

    /// the barycentrics are meaningless and the CPU side warns once.
    /// :range: 0..=2
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
    /// :range: 0.1..=10.0
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
    /// :range: 1..=512
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
    /// :skip:
    pub colormap: Vec<[f32; 3]>,

    /// Colour scale drawn over the render. Off by default.
    pub colorbar: Colorbar,

    /// Reference axes drawn around the scene.
    ///
    /// `"off"`, `"box"` (MATLAB), `"panes"` (matplotlib), `"gizmo"` (three
    /// labelled arrows at the origin), `"blender"` (ground grid, Z line and
    /// gizmo). A rendered body alone carries no scale or orientation; these
    /// supply both.
    pub axes: crate::app::axes::AxesStyle,

    /// Shade the `"blender"` style's ground grid instead of drawing it as
    /// line segments. On by default; `False` restores the segments.
    ///
    /// The segments end at the scene bounds, sit at one spacing, and are one
    /// pixel wide because WebGPU has no line width. This computes the grid
    /// per pixel instead: it has no edge, it crossfades between decades as
    /// you zoom -- which is what lets one grid serve a unit cube and a body
    /// 1e4 km away -- and its lines antialias themselves.
    pub grid: bool,
    /// Width of a grid line, in pixels.
    /// :range: 0.25..=8.0
    pub grid_width: f32,
    /// Cells between thick lines, and the factor between the levels the
    /// crossfade steps through -- the same number seen from two sides.
    /// :range: 2..=100
    pub grid_major: u32,
    /// Colour of the ordinary lines, `(r, g, b, a)`.
    pub grid_color: [f32; 4],
    /// Colour of every `grid_major`-th line.
    pub grid_major_color: [f32; 4],
    /// The X and Y axis lines, drawn over the grid so the origin reads
    /// without hunting for it.
    pub grid_axis_x_color: [f32; 4],
    pub grid_axis_y_color: [f32; 4],
    /// Fade the grid out between these grazing factors: `0.0` is looking
    /// straight down at the ground plane and `1.0` is looking along it.
    /// Without it the horizon is a hard line of aliasing.
    ///
    /// On the angle rather than the distance because the plane is infinite:
    /// what bounds it on screen is the horizon, not the far plane, so a
    /// distance fade never reaches its ramp.
    /// :range: 0.0..=1.0
    pub grid_fade_near: f32,
    /// :range: 0.0..=1.0
    pub grid_fade_far: f32,
    /// Colour of the axis lines and grid.
    pub axes_color: [f32; 3],
    /// Roughly how many ticks per axis. The step is rounded to 1, 2 or 5
    /// times a power of ten first, so the count lands near this rather than
    /// on it -- a figure with ticks at 0.0347 is unreadable.
    /// :range: 1..=20
    pub axes_ticks: usize,
    /// Appended to every tick label, e.g. `" km"`.
    ///
    /// The renderer knows the mesh is 0.437 across but not whether that is
    /// metres or kilometres, so the unit has to come from the script.
    pub axes_unit: String,
    /// Tick label size in pixels, and their colour.
    /// :range: 4.0..=64.0
    pub axes_label_size: f32,
    pub axes_label_color: [f32; 4],

    /// Which corner the navigation gizmo sits in. Any of the nine HUD
    /// anchors, so it can be moved out of the way of a colour bar or a HUD.
    ///
    /// Drawn by the `"gizmo"` and `"blender"` axes styles and by no other.
    pub gizmo_anchor: HudAnchor,
    /// Half the widget's width, in pixels: a ball centre never sits further
    /// than this from the middle.
    /// :range: 16.0..=200.0
    pub gizmo_size: f32,
    /// Gap between the widget and the edge of the image, in pixels. Ignored
    /// on the axis a centre anchor centres.
    /// :range: 0.0..=200.0
    pub gizmo_margin: f32,
    /// Size and colour of the `X`, `Y`, `Z` letters on the positive balls.
    /// The colour's alpha is scaled by how far the ball faces the viewer, so
    /// a letter never outshines the ball it is on.
    /// :range: 4.0..=64.0
    pub gizmo_label_size: f32,
    pub gizmo_label_color: [f32; 4],

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
            gpu_timing: false,
            occlusion_queries: false,
            debug_depth_show: false,
            debug_light_cube_show: false,
            debug_light_cube_fit: true,

            title: "kalast".to_string(),
            width: 0,
            height: 0,

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
            facet_labels: false,
            facet_labels_max: 2000,
            facet_label_size: 12.0,
            facet_label_color: [1.0, 1.0, 1.0, 0.9],
            selection_color: wgpu::Color {
                r: 1.0,
                g: 1.0,
                b: 0.0,
                a: 1.0,
            },
            wireframe_color: wgpu::Color::BLACK,
            wireframe_width: 1.0,

            vsync: true,
            export_sync: false,
            export_max_queued: 64,

            export_dir: "out/frames".to_string(),

            access_shadow_map: false,

            export_hud: false,

            shadow_per_body: true,

            colorbar: Colorbar::default(),

            axes: crate::app::axes::AxesStyle::Off,

            grid: true,
            grid_width: 1.0,
            grid_major: 10,
            grid_color: [0.32, 0.32, 0.35, 0.5],
            grid_major_color: [0.45, 0.45, 0.5, 0.75],
            grid_axis_x_color: [0.78, 0.24, 0.30, 0.9],
            grid_axis_y_color: [0.38, 0.66, 0.20, 0.9],
            grid_fade_near: 0.5,
            grid_fade_far: 1.0,
            axes_color: [0.45, 0.45, 0.45],
            axes_ticks: 5,
            axes_unit: String::new(),
            axes_label_size: 13.0,
            axes_label_color: [0.85, 0.85, 0.85, 1.0],
            gizmo_anchor: HudAnchor::TopRight,
            gizmo_size: 40.0,
            gizmo_margin: 18.0,
            gizmo_label_size: 9.0,
            // Dark, because it is read against the ball rather than against
            // the scene, and every ball colour is light enough to carry it.
            gizmo_label_color: [0.08, 0.08, 0.10, 1.0],

            value_min: None,
            value_max: None,
            colormap: Vec::new(),

            emulate_middle_button: cfg!(target_os = "macos"),
        }
    }
}

/// Settings for the **application**, not the simulation.
///
/// Two configs, because they answer different questions. This one is about
/// the program you are looking at: how big its window is, and -- as the
/// editor grows -- where its panels sit and what colour they are. The
/// simulation's own config, `app.simulation.config`, is about the thing being
/// simulated and the image made of it.
///
/// The split only became visible with the editor. While the scene filled the
/// window, "the window" and "the image" were one number; in a layout where
/// the viewport is a panel they are plainly two.
#[derive(Debug, Clone, PartialEq)]
pub struct AppConfig {
    /// Draw in the editor layout: viewport panel, script, config, log.
    ///
    /// A *mode*, not a different loop. `start()` and `step()` behave exactly
    /// as they always did -- the frame simply also draws the UI, and the
    /// scene lands in the viewport panel instead of filling the window. So a
    /// script that drives its own `while app.step():` gets the editor around
    /// it without changing a line, which is the whole reason `step()` was
    /// made non-blocking.
    ///
    /// `start_editor()` is this plus `start()`.
    ///
    /// :skip:
    /// No widget: a checkbox that switches the UI off from inside the UI
    /// leaves nothing to switch it back on with.
    pub editor: bool,

    /// Give the whole window to the renderer: panels out of the way, each
    /// coming back when the pointer reaches its edge.
    ///
    /// Focus on the scene, in other words. Not on the window -- that is yours
    /// to size, by double clicking its title bar or dragging a corner -- but
    /// on what is inside it. A focused renderer looks like a plain render
    /// window, with the panels a pointer-flick away: top for the toolbar,
    /// left for the script, right for the config, bottom for the log.
    ///
    /// Independent of `simulation.config.fullscreen`, which is the OS window
    /// and nothing else. Set both to be rid of everything at once; set this
    /// alone and the window stays where it is.
    pub focus: bool,

    /// Open the window without taking focus, so a run can go on beside
    /// other work.
    ///
    /// A render window normally comes up *key* and pulls the keyboard away
    /// from whatever was in front of it, which is fine for one run and not
    /// fine for a script that opens a window per case. With this set the
    /// window is ordered in behind the active application instead, and
    /// nothing is typed into it by accident.
    ///
    /// **Startup only** -- it decides how the window is first shown, and how
    /// the application announces itself, neither of which can be taken back
    /// afterwards. Set it before `start()` or the first `step()`.
    ///
    /// The window is still drawn and still interactive; it simply has to be
    /// clicked before it takes the keyboard.
    ///
    /// Named at length because two shorter names were taken and both mean
    /// something else: `focus` here is the panels *inside* the window, and
    /// `simulation.config.background` is the colour the frame is cleared to.
    ///
    /// Unsupported on X11 and Wayland, where winit cannot ask for it, and the
    /// window comes up focused as before.
    ///
    /// :skip:
    /// No widget: it is read once, while the window is being created. By the
    /// time there is a panel to tick it in, the window it would have governed
    /// is already open, and a checkbox that does nothing is worse than none.
    pub open_in_background: bool,

    /// Window size in physical pixels.
    ///
    /// The *window*, not the render. `simulation.config.width` is the image
    /// inside it, and follows this unless it is set.
    ///
    /// :label: window width
    pub width: u32,
    /// :label: window height
    pub height: u32,

    /// What the editor's toolbar says beside the transport buttons.
    ///
    /// The same template as `huds`, so every placeholder works here too --
    /// `{drawn}` for the iteration on screen, `{it}` for how many have been
    /// begun, `{its}`, `{fps}`, `{ms}`, `{bodies}`, `{paused}`, `{warn}`,
    /// `{gpu}` and its per-pass forms -- and a precision may be attached, as
    /// `{fps:.1}`.
    ///
    /// Empty for a bare toolbar.
    ///
    /// :label: toolbar text
    pub toolbar: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            editor: false,
            focus: false,
            open_in_background: false,
            width: 0,
            height: 0,
            toolbar: "iteration {drawn}    {its} it/s    {fps} fps".to_string(),
        }
    }
}
