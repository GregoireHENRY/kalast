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
    /// :step: 1.0
    pub x: f32,
    /// :step: 1.0
    pub y: f32,
    /// Long and short axis of the bar, pixels. Its edges drag in the
    /// viewport too.
    /// :step: 1.0
    pub length: f32,
    /// :step: 1.0
    pub thickness: f32,
    /// Length of a tick mark, pixels, across the bar's edge -- half inside,
    /// half out -- the numbers standing past it.
    /// :step: 0.5
    pub tick_size: f32,
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
    /// Outline drawn around the strip, in the text's colour, so it reads as a
    /// scale rather than as part of the scene when it sits over a dark body.
    pub border: bool,
    /// Mark the lowest and highest value the bodies carry where they fall on
    /// the scale -- at its end, when a pinned range leaves them outside it --
    /// and name them, on the side away from the tick numbers: above a
    /// horizontal bar, left of a vertical one. The data map's only
    /// (`shading.color_mode` "data").
    pub min_max: bool,
    /// How `min_max` writes the two values, as a Python format spec: `.0f`
    /// whole numbers, `.3f` three decimals, `.2e` in powers of ten, `d` an
    /// integer. Anything else reads as `.0f`.
    pub min_max_format: String,
}

impl Default for Colorbar {
    fn default() -> Self {
        Self {
            enabled: false,
            anchor: HudAnchor::BottomCenter,
            x: 0.0,
            y: 48.0,
            length: 800.0,
            thickness: 36.0,
            tick_size: 5.0,
            vertical: None,
            label: String::new(),
            ticks: 5,
            text_size: 13.0,
            text_color: [0.92, 0.92, 0.92, 1.0],
            border: true,
            min_max: false,
            min_max_format: ".0f".into(),
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

/// The colour the data map draws `v` in: the shader's `colormap_lookup` on
/// the CPU, so that a selected facet's colour can be shown beside its value.
/// `table` as `data.colormap` holds it -- empty is the renderer's greyscale
/// -- resampled to the renderer's entries and read between two of them, `v`
/// clamped to `range`.
pub fn colormap_color(table: &[[f32; 3]], range: (f32, f32), v: f32) -> [f32; 3] {
    let t = ((v - range.0) / (range.1 - range.0).max(1e-20)).clamp(0.0, 1.0);
    if table.is_empty() {
        return [t, t, t];
    }
    let n = crate::app::uniform::COLORMAP_SIZE;
    let lut = resample_colormap(table, n);
    let x = t * (n - 1) as f32;
    let i = x.floor() as usize;
    let (a, b, f) = (lut[i], lut[(i + 1).min(n - 1)], x - i as f32);
    [a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f, a[2] + (b[2] - a[2]) * f]
}

/// A colour's red, green and blue, `0..1` as kalast takes colours -- what a
/// script can give `selection.color` or a colormap back.
pub fn rgb(c: [f32; 3]) -> String {
    format!("{:.3} {:.3} {:.3}", c[0], c[1], c[2])
}

/// Resample a colour table to `n` entries, interpolating between them.
///
/// Interpolated, not nearest: nearest turned a table of a few anchors into
/// as many visible bands -- tolerable on a shaded body where lighting hides
/// it, obvious on a colour scale, which is a flat ramp with nothing to hide
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

/// matplotlib's colormaps, every one, sampled at the renderer's 256 entries
/// by `tools/gen_colormaps.py` -- the format is in its docstring, the notices
/// they come with in `res/LICENSE-colormaps`. Compiled in, so the panel lists
/// them and a script names them without matplotlib, in Rust alone too.
static COLORMAPS: &[u8] = include_bytes!("../../res/colormaps.bin");

/// One of `COLORMAPS`: its name, whether it is another's table under a
/// second name -- `grey` of `gray` -- and its 256 RGB bytes.
struct Colormap {
    name: &'static str,
    alias: bool,
    rgb: &'static [u8],
}

impl Colormap {
    fn rows(&self) -> impl DoubleEndedIterator<Item = [f32; 3]> + '_ {
        self.rgb.chunks_exact(3).map(|c| [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0])
    }

    /// Whether `table` is this one, in its order or reversed.
    fn is(&self, table: &[[f32; 3]], reversed: bool) -> bool {
        table.len() * 3 == self.rgb.len()
            && if reversed {
                self.rows().rev().eq(table.iter().copied())
            } else {
                self.rows().eq(table.iter().copied())
            }
    }
}

fn colormaps() -> &'static [Colormap] {
    static PARSED: std::sync::OnceLock<Vec<Colormap>> = std::sync::OnceLock::new();
    PARSED.get_or_init(|| {
        let b = COLORMAPS;
        assert!(b.starts_with(b"KALASTCM"), "res/colormaps.bin is not tools/gen_colormaps.py's");
        let count = u16::from_le_bytes([b[8], b[9]]) as usize;
        let mut at = 10;
        (0..count)
            .map(|_| {
                let (alias, len) = (b[at] == 1, b[at + 1] as usize);
                let name = std::str::from_utf8(&b[at + 2..at + 2 + len]).expect("a name in UTF-8");
                let rgb = &b[at + 2 + len..at + 2 + len + 3 * crate::app::uniform::COLORMAP_SIZE];
                at += 2 + len + rgb.len();
                Colormap { name, alias, rgb }
            })
            .collect()
    })
}

/// The colormaps the panel lists: matplotlib's, each table once, in
/// matplotlib's order -- its perceptually uniform first. Their other names,
/// `grey` of `gray`, are taken too, and each reversed with `_r` after it, as
/// matplotlib names them. Python's `colormap_names` gives these.
pub fn colormap_names() -> Vec<&'static str> {
    colormaps().iter().filter(|c| !c.alias).map(|c| c.name).collect()
}

/// A built-in colour table by its matplotlib name, `_r` after it for its
/// reverse, as its 256 rows; its case need not match, `Inferno` is
/// `inferno`. `None` for a name matplotlib does not have.
pub fn builtin_colormap(name: &str) -> Option<Vec<[f32; 3]>> {
    if let Some(forward) = name.strip_suffix("_r").or_else(|| name.strip_suffix("_R")) {
        return builtin_colormap(forward).map(|mut table| {
            table.reverse();
            table
        });
    }
    let all = colormaps();
    all.iter()
        .find(|c| c.name == name)
        .or_else(|| all.iter().find(|c| c.name.eq_ignore_ascii_case(name)))
        .map(|c| c.rows().collect())
}

/// The name a colour table was made from: a listed built-in, `_r` after it
/// if reversed -- or `None` for one given as an array or read from a file.
/// Empty, the default, is the renderer's own greyscale, which is `gray`.
pub fn colormap_name(table: &[[f32; 3]]) -> Option<String> {
    if table.is_empty() {
        return Some("gray".into());
    }
    // A name as it is before any reversed: gray's table is binary's the
    // other way round, and is gray.
    let listed = || colormaps().iter().filter(|c| !c.alias);
    listed()
        .find(|c| c.is(table, false))
        .map(|c| c.name.to_string())
        .or_else(|| listed().find(|c| c.is(table, true)).map(|c| format!("{}_r", c.name)))
}

/// A colour table read from a text file: a colour a line, its red, green
/// and blue -- a fourth value, alpha, dropped -- apart by commas, spaces,
/// tabs or semicolons. In 0..1, or 0..255 where any value is above 1.
/// Blank lines, `#` comments and a header before the first colour are
/// skipped, so a table saved from matplotlib, ParaView or a spreadsheet
/// reads as it is.
pub fn colormap_from_file(path: &std::path::Path) -> Result<Vec<[f32; 3]>, String> {
    let shown = path.display();
    let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read {shown}: {e}"))?;
    let mut rows: Vec<[f32; 3]> = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.split('#').next().unwrap_or_default().trim();
        if line.is_empty() {
            continue;
        }
        let values: Result<Vec<f32>, _> = line
            .split(|c: char| c == ',' || c == ';' || c.is_whitespace())
            .filter(|v| !v.is_empty())
            .map(str::parse::<f32>)
            .collect();
        match values {
            Ok(v) if v.len() == 3 || v.len() == 4 => rows.push([v[0], v[1], v[2]]),
            Ok(v) => {
                return Err(format!(
                    "{shown}, line {}: {} values, where a colour is 3 -- or 4, with alpha",
                    i + 1,
                    v.len()
                ));
            }
            Err(_) if rows.is_empty() => {} // a header
            Err(_) => return Err(format!("{shown}, line {}: not numbers: {line}", i + 1)),
        }
    }
    if rows.is_empty() {
        return Err(format!("{shown}: no colours in it, a line of red, green and blue each"));
    }
    if let Some(x) = rows.iter().flatten().find(|x| !x.is_finite()) {
        return Err(format!("{shown}: {x} is not a colour value"));
    }
    let (low, high) = rows.iter().flatten().fold((f32::MAX, f32::MIN), |(l, h), &x| (l.min(x), h.max(x)));
    if !(low >= 0.0 && high <= 255.0) {
        return Err(format!("{shown}: colours run from {low} to {high}, where 0..1 or 0..255 is read"));
    }
    if high > 1.0 {
        rows.iter_mut().flatten().for_each(|x| *x /= 255.0);
    }
    Ok(rows)
}

/// What the surface shows (`shading.color_mode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    /// Lit by the Sun through the body's scattering law, with the shadows
    /// `shadows.method` draws. The default.
    Shaded,
    /// The data, unlit: a mesh's `values` through the colormap, else its own
    /// colours as they are. What a quantitative figure wants: shading a data
    /// map makes one value read as two colours.
    Data,
}

impl ColorMode {
    pub const ALL: [ColorMode; 2] = [Self::Shaded, Self::Data];

    /// Parsed from Python, where these are plain strings.
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "shaded" | "shader" => Some(Self::Shaded),
            "data" => Some(Self::Data),
            _ => None,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Shaded => "shaded",
            Self::Data => "data",
        }
    }

    /// The number the shaders take (`Globals.color_mode`), which is also
    /// what scripts wrote before the names: 0 and 1.
    pub fn code(&self) -> u32 {
        match self {
            Self::Shaded => 0,
            Self::Data => 1,
        }
    }

    pub fn from_code(code: u32) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.code() == code)
    }
}

/// How the surface is coloured and the image encoded.
#[derive(Clone, Debug)]
pub struct Shading {
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
    /// Multisample anti-aliasing for the main render pass: 1 (off), 2, 4 or 8
    /// samples a pixel.
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
    /// MSAA takes powers of two, and which of them a GPU has is its own: an
    /// Apple M1 Pro has 1, 2 and 4, no 8. A count the GPU lacks falls back to
    /// the largest it has below, and the console says so, naming the GPU and
    /// its counts. Note that `debug_depth_show` only mirrors the main pass's
    /// depth at 1: above that the pass writes its own multisampled depth
    /// buffer instead.
    /// :choices: 1 = off, 2, 4, 8
    pub msaa: u32,
    // See app/uniform.rs Globals struct for shader
    /// What the surface shows: "shaded", lit by the Sun with the shadows
    /// `shadows.method` draws (the default), or "data", unlit, a mesh's
    /// values through the colormap, else its own colours.
    ///
    /// The numbers scripts set before 12 October still work for a release,
    /// with a deprecation warning: 0 is "shaded", 1 "data", and 3, shaded
    /// without shadows, is `shadows.method = "none"`. 2, every body one flat
    /// colour, is gone.
    /// :py_custom:
    pub color_mode: ColorMode,
    /// How the lit image is stored: "screen", through the sRGB curve as a
    /// screen shows light (the default), or "linear", the lit value itself.
    ///
    /// "screen": a screen showing the image gives off light in proportion
    /// to the I/F, the natural look -- a lit 0.078 is stored as 0.31.
    /// "linear": an exported pixel reads `exposure * I/F * 255`, as a linear
    /// camera records it, darker on a screen; with MSAA a pixel stores the
    /// mean of its samples' values, the mean I/F of what it covers. Colours
    /// -- a colormap's, a picked facet's, the data mode's -- are stored as
    /// given in both: the data mode has no use for it.
    ///
    /// The numbers scripts set before 14 October still work for a release,
    /// with a deprecation warning: 0 is "screen", 1 "linear". In the settings
    /// in the shaded mode only: the data mode stores its colours as given
    /// whatever it is.
    /// :when: color_mode == ColorMode::Shaded
    /// :py_custom:
    pub srgb_mode: SrgbMode,
    /// The power law the conversion uses instead of sRGB's own curve, unset
    /// by default.
    ///
    /// 2.2 is how it was done before 7 October, and parted from the curve in
    /// the dark: with `srgb_mode` "linear" a lit 0.078 was stored as 0.047,
    /// and a colormap's darkest colours came out darker than they are. Set
    /// it only to reproduce such an image. In the settings with `srgb_mode`
    /// "linear" only; it applies to the colours too, set from a script.
    /// :when: color_mode == ColorMode::Shaded && srgb_mode == SrgbMode::Linear
    /// :some: 2.2
    /// :step: 0.01
    pub gamma: Option<Float>,
    /// Draw a large mesh by the parts the camera can see, each only as fine
    /// as the image can show it.
    ///
    /// A flat mesh of 16,384 facets or more is split, on a thread of its own
    /// once loaded, into a tree of patches, each simplified a level at a
    /// time; every frame draws the patches in view and facing the camera, at
    /// the level whose triangles are about `lod_pixels` across. Each patch
    /// hangs a skirt below its edge, so patches drawn at different levels
    /// leave no gap between them, and every triangle drawn shows the colour,
    /// mode and value of an original facet. The shadow maps hold the camera's
    /// own triangles where the camera looks, coarser ones elsewhere
    /// (`shadows.lod_pixels`), and fit to what the camera sees. The facet-id
    /// pass and the per-facet shadow query always see the full mesh. Off,
    /// or until the tree is built, every mesh is drawn whole, every frame.
    pub lod: bool,
    /// The size, in pixels, a large mesh's triangles are drawn at when `lod`
    /// is on: smaller is finer and slower.
    ///
    /// A patch is drawn at the coarsest level whose triangles' edges come out
    /// at most this many pixels; its full facets when even those are larger.
    /// At 2, about a triangle per pixel: on Mars at Hera's closest, 0.35 %
    /// from every facet drawn, after a pixel's blur; 0.25 % at 1.5, 0.76 %
    /// at 3.
    /// :range: 0.25..=16.0
    pub lod_pixels: f32,
}

impl Default for Shading {
    fn default() -> Self {
        Self {
            background: wgpu::Color::BLACK,
            render_back_face: false,
            msaa: 4,
            color_mode: ColorMode::Shaded,
            srgb_mode: SrgbMode::Screen,
            gamma: None,
            lod: true,
            lod_pixels: 2.0,
        }
    }
}

/// The Sun as a light: its colour, the ambient floor, the debug cube.
#[derive(Clone, Debug)]
pub struct Light {
    /// Draw a cube at the light's position, so the Sun is visible.
    ///
    /// Size comes from `light_cube_scale`. `debug_light_cube_fit` is on by
    /// default, which is what makes it *visible* rather than merely drawn:
    /// the camera's far plane is fitted to the bodies, and the Sun is well
    /// outside them.
    pub cube_show: bool,
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
    pub cube_fit: bool,
    /// Light added to every fragment regardless of shadowing.
    ///
    /// **Zero by default.** A shadowed facet on an airless body receives
    /// essentially nothing, so any ambient term is light the scene does not
    /// have -- and a shadow that is not black is a shadow whose depth cannot
    /// be read off the image. It was 0.002, small enough to look like nothing
    /// and large enough to be a floor under every dark pixel.
    ///
    /// Raise it to see into shadows while navigating; it is the wrong thing
    /// to have on for anything quantitative.
    /// :range: 0.0..=1.0
    pub ambient: f32,
    /// Colour of the Sun, `(r, g, b, a)`.
    pub color: wgpu::Color,
    /// Scales the light by one number, `color`'s red, green and blue alike:
    /// a camera's exposure.
    ///
    /// A lit pixel is `exposure * color * albedo * cos(i)`, the ambient term
    /// scaled with it, and the debug cube is drawn as bright. With
    /// `shading.srgb_mode = "linear"` an exported pixel reads `exposure * I/F * 255`:
    /// 4.5 shows I/F 0 to 0.22 from black to white, and anything brighter
    /// clips at white -- without a `stretch`, which shows the values its own
    /// way. The data mode's colours -- a colormap's -- are data, and keep
    /// theirs.
    /// :range: 0.1..=1000.0
    /// :log:
    pub exposure: f32,
    /// The Sun as a point, every shadow hard; on by default.
    ///
    /// Off, the Sun is a disc of `sun_radius` as each point sees it, and a
    /// shadow has the penumbra that disc gives it: as soft as its occluder
    /// is far from what it falls on, a body's own shadows and another's
    /// alike, and where they overlap, what both leave of the disc. A moon's
    /// shadow on a planet is a soft spot, an eclipse comes on gradually --
    /// from Mars, Phobos's shadow is at most 22 % deep and 50 km across --
    /// and a boulder's shadow blurs toward its tip. The disc is
    /// limb-darkened. A penumbra narrower than a shadow-map texel is drawn
    /// as with a point.
    pub sun_as_point: bool,
    /// The Sun's radius in the scene's units, with `sun_as_point` off:
    /// 695,700, the Sun's in km. A scene in metres wants 6.957e8.
    /// :when: sun_as_point == false
    /// :step: 1000.0
    pub sun_radius: f32,
    /// Size of the debug light cube, in world units.
    ///
    /// Only drawn when `debug_light_cube_show` is on.
    /// :range: 0.0..=5.0
    pub cube_scale: Float,
}

impl Light {
    /// The Sun's radius as the shadows see it: 0 for a point.
    pub fn disc_radius(&self) -> f32 {
        if self.sun_as_point {
            0.0
        } else {
            self.sun_radius.max(0.0)
        }
    }
}

impl Default for Light {
    fn default() -> Self {
        Self {
            cube_show: false,
            cube_fit: true,
            ambient: 0.0,
            color: wgpu::Color::WHITE,
            exposure: 1.0,
            sun_as_point: true,
            sun_radius: 695_700.0,
            // light_target: Vec3::new(0.0, 0.0, 0.0),
            // light_up: Vec3::new(0.0, 0.0, 1.0),
            // light_side: 10.0,
            // light_znear: 0.1,
            // light_zfar: 100.0,
            cube_scale: 0.25,
        }
    }
}

/// How the lit image is stored (`shading.srgb_mode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SrgbMode {
    /// Through the sRGB curve, as a screen shows light: on a screen, light in
    /// proportion to the I/F. A lit 0.078 stored as 0.31. The default.
    Screen,
    /// The lit value itself: an exported pixel reads exposure x I/F x 255, as
    /// a linear camera records it. Darker on a screen.
    Linear,
}

impl SrgbMode {
    pub const ALL: [SrgbMode; 2] = [Self::Screen, Self::Linear];

    /// Parsed from Python, where these are plain strings.
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.name() == s.to_ascii_lowercase())
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Screen => "screen",
            Self::Linear => "linear",
        }
    }

    /// The number the shaders take (`Globals.srgb_mode`), which is also what
    /// scripts wrote before the names: 0 and 1.
    pub fn code(&self) -> u32 {
        match self {
            Self::Screen => 0,
            Self::Linear => 1,
        }
    }

    pub fn from_code(code: u32) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.code() == code)
    }
}

/// How the Sun's shadows are drawn (`shadows.method`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShadowMethod {
    /// Shadow maps: a layer per body, fitted to what the camera sees of it,
    /// drawn from the Sun every frame. The default.
    Maps,
    /// Traced with rays against the full meshes, the Sun's disc sampled by
    /// `ray_samples` rays a point: no texel, filter or bias. On a GPU with ray
    /// queries; without, the shadow maps answer.
    Rays,
    /// A reference image: a still drawn as exactly as kalast can, for a figure
    /// or to check a faster method against -- every pixel summed over its
    /// area and the Sun's disc with rays, a sample a frame, until its error is
    /// under `reference.error`, then held. For a still scene: anything that
    /// moves starts the sum again.
    Reference,
    /// No shadows: every point facing the Sun sees all of it, and nothing is
    /// drawn for them.
    None,
}

impl ShadowMethod {
    pub const ALL: [ShadowMethod; 4] = [Self::Maps, Self::Rays, Self::Reference, Self::None];

    /// Parsed from Python, where these are plain strings.
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.name() == s.to_ascii_lowercase())
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Maps => "maps",
            Self::Rays => "rays",
            Self::Reference => "reference",
            Self::None => "none",
        }
    }

    /// Traced with rays, where the GPU can: the rays and a reference image.
    pub fn traces(&self) -> bool {
        matches!(self, Self::Rays | Self::Reference)
    }
}

/// The shadow map and its readback.
#[derive(Clone, Debug)]
pub struct Shadows {
    /// How the Sun's shadows are drawn: "maps", a shadow map per body (the
    /// default); "rays", traced against the full meshes, on a GPU with ray
    /// queries; "reference", a still image summed with rays to a stated error
    /// (`reference`); "none", no shadows at all. The image's and those the
    /// thermophysical model reads (`sim.facet_shadow`) alike -- a reference
    /// image's, the rays'.
    pub method: ShadowMethod,
    /// Side length of each square shadow map, in texels.
    ///
    /// One layer per body (one in all with `per_body` off), each
    /// `resolution^2 x 4 bytes`: 67 MB a layer at the default 4096, 268 MB at
    /// 8192, 17 MB at 2048. The default was 8192 until 22 September; every
    /// layer is stored every frame and the main pass waits for it, and at
    /// 4096 the Didymos pair went from 93 to 106 it/s with the per-facet
    /// shadow query unchanged (texel 21 cm on Didymos, 4 cm on Dimorphos,
    /// with a layer per body). See
    /// `notes/2026-09-22_indexed_main_pass_primitive_index/`.
    ///
    /// It also feeds the automatic bias, which is expressed relative to one texel,
    /// so changing it changes the shadow bias with it.
    /// :when: method == ShadowMethod::Maps
    /// :range: 512..=16384
    pub resolution: u32,
    /// Smoothing of the shadows' edges, the percentage-closer-filtering
    /// kernel's *radius* in shadow-map texels: 0 is the hardware's single 2x2
    /// comparison, N a `(2N+1)^2` grid of them averaged.
    ///
    /// The grid lies on the ground around each point, a texel apart along it,
    /// and blurs a shadow's edge as wide whatever the Sun's height, without
    /// moving it. It darkened lit ground as if by ambient occlusion before
    /// 9 October -- on Dimorphos from 68 m, 45 % of the lit pixels by more than
    /// 5 % at 16, 8 % at 4 -- its taps then a square in the map's view, which
    /// at a low Sun reached far along the ground. See
    /// `notes/2026-10-09_pcf_on_the_ground/`.
    ///
    /// It costs where a shadow's edge is in reach -- elsewhere five taps agree
    /// and the rest are skipped -- and grows with the image's pixels: on
    /// Dimorphos at 3234 x 1774, 1.4 ms a frame at 2 and 4.3 at 4, on an M1
    /// Pro. With the map's texels finer than the image's pixels (16384 up
    /// close) there are no stairs to smooth.
    /// :when: method == ShadowMethod::Maps
    /// :range: 0..=16
    pub pcf: u32,
    // None means "derive from the fitted light frustum and shadow_resolution"
    // (the default). These are scale-dependent -- values tuned for a 780 m
    // body seen from 25 km are wrong for any other scene -- so deriving them
    // per frame is both more correct and less work than hand-tuning. Set one
    // to pin it and leave the rest automatic; see app/frame.rs::fit_shadow.
    /// Push the sample along the surface normal before the shadow lookup, in
    /// world units. `None` fits it per frame from the layer's own texel size.
    ///
    /// One texel diagonal whatever the PCF radius. The far taps of a kernel
    /// are the shader's per-tap receiver-plane term's business, not this
    /// offset's: lifting the lookup further off the surface moved the shadow
    /// instead of blurring it.
    ///
    /// Pinning it is now worse than leaving it automatic: with per-body shadow
    /// layers a pinned value replaces the fitted one on *every* layer, and
    /// those differ by the ratio of the bodies' sizes -- 403x between Mars and
    /// Deimos in the same scene.
    /// :when: method == ShadowMethod::Maps
    pub normal_offset_scale: Option<f32>,
    /// Slope-dependent term of the depth-comparison bias. `None` fits it per
    /// frame. Combined in the shader as
    /// `max(shadow_bias_scale * k, shadow_bias_minimum)`.
    ///
    /// See `shadow_normal_offset_scale` for why pinning is discouraged.
    /// :when: method == ShadowMethod::Maps
    pub bias_scale: Option<f32>,
    /// Floor on the depth-comparison bias, for surfaces facing the light
    /// head-on. `None` fits it per frame.
    ///
    /// Measured to be the *ineffective* knob for the crater-floor PCF leak --
    /// auto, 1e-4 and 1e-3 all gave identical results, while the normal offset
    /// moved it 9x. Reach for that one first.
    /// :when: method == ShadowMethod::Maps
    pub bias_minimum: Option<f32>,
    /// Compute every body's per-facet shadows every frame, as `method` draws
    /// them, read back with `sim.facet_shadow(body)` from `after_render`.
    ///
    /// Off by default because it is not free: the query costs ~1.6 ms per
    /// body at 100k facets and ~7.3 ms at 3.1M, dominated by the blocking
    /// readback. Turn it on for thermophysical or radiance work; leave it off
    /// when you only want images.
    pub access_shadow_map: bool,
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
    /// :when: method == ShadowMethod::Maps
    pub per_body: bool,
    /// A finer shadow layer over where the camera looks closest, for a body
    /// seen up close.
    ///
    /// A body's layer is fitted to all the camera sees of it, so a close view
    /// reaching far across the body coarsens its texels: Dimorphos from 37 m
    /// had them at 3 cm, its boulders' penumbrae 4 cm wide -- hard-edged
    /// stairs, and too narrow for the Sun's disc to soften
    /// (`light.sun_as_point`). The near layer covers the patches nearest the
    /// camera, as far out as keeps its texels half the image's pixels there,
    /// and a point inside it is looked up there. Only while the body's own
    /// layer is twice coarser than that, with a perspective camera, layers
    /// per body, the body drawn by its cut (`shading.lod`) and without a
    /// horizon map, and up to eight layers in all; then it costs a layer's
    /// memory and a shadow pass.
    /// :when: method == ShadowMethod::Maps
    pub near_layer: bool,
    /// With the Sun a disc, a second depth layer under each shadow layer: the
    /// nearest surface behind what the Sun sees first. Nothing with the Sun a
    /// point, whatever it is set to.
    ///
    /// A map holds what the Sun sees first, and a rock in the shadow of a
    /// bigger one is not in it: where a penumbra is walked through such
    /// relief, the disc comes through. Near Dimorphos's terminator, with
    /// texels fine enough to walk its penumbrae (16384), it lit slivers in
    /// the dark, 0.05-0.17 of the disc where rays reach none, which this
    /// takes to 0; on Dimorphos from 68 m, it takes the thin gaps of light
    /// left from 163 pixels to 101. It costs a second shadow pass and a slice
    /// per layer, the pass slower than the first -- 2 to 7 ms a frame on
    /// Dimorphos up close -- for little where penumbrae are narrower than six
    /// texels and drawn hard, as at the default resolution.
    /// :when: method == ShadowMethod::Maps && light.sun_as_point == false
    pub second_depth: bool,
    /// Keep each body's shadow layer from frame to frame, drawn again only
    /// when the Sun has moved past `cache_degrees` in the body's own frame,
    /// another body has moved about it, or the scene or the shadows' settings
    /// changed.
    ///
    /// A layer kept turns with its body, so its own shadows stay on it; a
    /// camera that moves, or a simulation paused, then costs no shadow pass at
    /// all. Each layer is fitted to its whole body rather than to what the
    /// camera sees of it, and there is no near layer (`near_layer`), so a view
    /// up close has coarser texels than without. The per-facet shadows the
    /// thermophysical model reads are the kept layers too. Off by default:
    /// the layers are drawn every frame, as fine as the view allows.
    /// :when: method == ShadowMethod::Maps
    pub cache: bool,
    /// How far, in degrees, the Sun may move in a body's own frame, or another
    /// body about it, before a kept layer is drawn again (`cache`). 0: only
    /// when nothing has moved at all, the shadows as exact as without the
    /// cache. Larger keeps layers through a running simulation -- Didymos
    /// turns 0.044 deg a second of simulated time -- each shadow up to that
    /// far behind the Sun.
    /// :when: method == ShadowMethod::Maps && cache == true
    /// :step: 0.01
    pub cache_degrees: f32,
    /// The size, in shadow-map texels, a large mesh's triangles are drawn at
    /// in the shadow maps when `shading.lod` is on, outside the camera's
    /// view.
    ///
    /// A shadow cast into the view needs its occluder's outline, not its
    /// facets: 2 keeps the outline to a texel. With `access_shadow_map`, or a
    /// per-facet shadow query pending, the maps take the full facets and the
    /// whole body.
    /// :when: method == ShadowMethod::Maps
    /// :range: 0.5..=16.0
    pub lod_texels: f32,
    /// The size, in the camera's pixels, a large mesh's triangles are drawn
    /// at in the shadow maps when `shading.lod` is on, outside the camera's
    /// view.
    ///
    /// Where the camera looks, the maps hold the very triangles it draws: a
    /// coarser caster over a finer receiver shadows it wherever the coarse
    /// surface passes above the fine one, across every crater floor. Outside
    /// it, a caster only casts into the view, and coarser does: whichever of
    /// this and `lod_texels` is coarser is used.
    /// :when: method == ShadowMethod::Maps
    /// :range: 0.5..=32.0
    pub lod_pixels: f32,
    /// The rays each point -- a pixel, a facet's corner or centre -- traces
    /// across the Sun's disc, with `method` "rays" and `light.sun_as_point`
    /// off, and with "reference" each facet's: each carries the same share of
    /// the limb-darkened disc's light, so a penumbra is resolved to about
    /// 1/`ray_samples`. A point Sun takes one.
    ///
    /// No texel, filter or bias: a shadow is where the mesh is, a penumbra
    /// what the rays see of the disc. With a level of detail, a pixel's rays
    /// start from the full mesh, not the coarser surface drawn. Vulkan on
    /// Windows and Linux, Metal on macOS 15 and later -- on an Apple M1 or M2
    /// the traversal is the driver's, slower than on hardware that has it.
    /// :when: method.traces()
    /// :range: 1..=1024
    pub ray_samples: u32,
    /// The rays a point tries first across the disc, with `method` "rays": where
    /// they all agree the point is wholly lit or wholly hidden and the rest
    /// of `ray_samples` are not traced. Nearly every point is, so a step
    /// costs about this many rays a point whatever `ray_samples` is.
    ///
    /// A part of the disc smaller than about 1/`ray_probe` of it, hidden or
    /// seen with all of these rays missing it, is missed: the edge of a
    /// penumbra by a sliver. 0 traces all of `ray_samples` everywhere,
    /// exact to them.
    /// :when: method.traces()
    /// :range: 0..=64
    pub ray_probe: u32,
    /// Trace the disc's rays only where the shadow maps see a penumbra can
    /// be, one ray to the Sun's centre elsewhere: much quicker, and exact
    /// wherever the maps hold what casts the penumbra.
    ///
    /// With `method` "rays" and the Sun a disc, the maps are drawn as well,
    /// and each point -- a pixel, a facet -- first asks their depth pyramid
    /// whether anything lies within the disc's reach of it. Where nothing
    /// does, the point sees all of the disc or, in the umbra, none of it, and
    /// one ray says which; where something does, `ray_samples` rays are
    /// traced across the disc. Relief finer than the maps' texels is not in
    /// them, and its penumbra is drawn hard. A body with a horizon map is
    /// traced throughout. Off by default.
    /// :when: method == ShadowMethod::Rays
    pub ray_guide: bool,
    /// The points of a pixel its rays start from, with `method` "rays": 1,
    /// the surface under its centre; more, the surface under that many
    /// points spread over it -- MSAA's own patterns -- their shares of the
    /// Sun averaged.
    ///
    /// A shadow narrower than a pixel, a groove's under a low Sun, is then
    /// the share of the pixel it covers, as the reference draws it: from the
    /// centre alone it came out as dots along the groove, each pixel's
    /// centre in the shadow or out of it -- on Phobos's grooves near the
    /// limb, 1,970 pixels of h8963 against the reference's thin grey lines.
    /// Not a filter: the shadows are traced there, and a shadow wider than a
    /// pixel is as sharp. Each point costs a pixel's rays again; 8 and 16
    /// trace four first, and all of theirs only where those four disagree --
    /// a shadow's edge in the pixel -- so they cost little more than 4. A
    /// count between the patterns takes the next one up.
    /// :when: method == ShadowMethod::Rays
    /// :choices: 1 = the centre, 4, 8, 16
    pub ray_points: u32,
}

/// The shadow settings for one use, from the quickest look to the most exact
/// (`Config::set_quality`): each sets the fields `settings` lists, and leaves
/// everything else.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quality {
    /// A layer per body at 2048 texels, hard-edged (PCF 0), a point Sun: the
    /// quickest shadows, the Didymos pair from 1 km at 868 frames a second
    /// against the defaults' 685, Dimorphos from 68 m at 331 against 201, on
    /// an M1 Pro. One layer over the whole scene, which it was until 12
    /// October, was slower up close (179).
    Quick,
    /// A layer per body, kept from frame to frame, a point Sun.
    Fast,
    /// A layer per body drawn every frame, a point Sun: the defaults.
    Point,
    /// The Sun's disc with its second depth layer, a layer per body and a
    /// near layer, drawn every frame.
    Accurate,
    /// The shadows traced with rays (`shadows.method` "rays"), the Sun's disc
    /// sampled by 64 a point, 16 tried first: on a GPU with ray queries.
    Rays,
}

/// One field a `Quality` sets, and its value, as a script writes them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Preset {
    Bool(&'static str, bool),
    U32(&'static str, u32),
    F32(&'static str, f32),
    Method(&'static str, ShadowMethod),
}

impl std::fmt::Display for Preset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Preset::Bool(name, v) => write!(f, "{name} = {}", if v { "True" } else { "False" }),
            Preset::U32(name, v) => write!(f, "{name} = {v}"),
            Preset::F32(name, v) => write!(f, "{name} = {v:?}"),
            Preset::Method(name, v) => write!(f, "{name} = \"{}\"", v.name()),
        }
    }
}

impl Quality {
    pub const ALL: [Quality; 5] = [Quality::Quick, Quality::Fast, Quality::Point, Quality::Accurate, Quality::Rays];

    pub fn name(self) -> &'static str {
        match self {
            Quality::Quick => "quick",
            Quality::Fast => "fast",
            Quality::Point => "point",
            Quality::Accurate => "accurate",
            Quality::Rays => "rays",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|q| q.name() == name)
    }

    /// What it is for, in a line.
    pub fn describe(self) -> &'static str {
        match self {
            Quality::Quick => "The quickest shadows: a layer per body at 2048 texels, hard-edged, the Sun a point",
            Quality::Fast => "Fast: a shadow layer per body, kept from frame to frame",
            Quality::Point => "The defaults: a shadow layer per body, drawn every frame, the Sun a point",
            Quality::Accurate => "The Sun's disc and its penumbrae, from the shadow maps",
            Quality::Rays => "The shadows traced with rays, the Sun's disc and its penumbrae: on a GPU with ray queries",
        }
    }

    /// What it sets, each field to its value: one table, so the hover
    /// (`hover`), `apply` and `Config::quality` cannot disagree. The maps'
    /// presets pick the maps; the rays' leaves the maps' settings, which
    /// nothing reads while they answer.
    pub fn settings(self) -> Vec<Preset> {
        use Preset::*;
        let maps = |point, per_body, cache, pcf, resolution| {
            vec![
                Method("shadows.method", ShadowMethod::Maps),
                Bool("light.sun_as_point", point),
                Bool("shadows.per_body", per_body),
                Bool("shadows.cache", cache),
                U32("shadows.pcf", pcf),
                U32("shadows.resolution", resolution),
            ]
        };
        match self {
            Quality::Quick => maps(true, true, false, 0, 2048),
            Quality::Fast => [maps(true, true, true, 2, 4096), vec![F32("shadows.cache_degrees", 0.0)]].concat(),
            Quality::Point => maps(true, true, false, 2, 4096),
            Quality::Accurate => [
                maps(false, true, false, 2, 4096),
                vec![Bool("shadows.second_depth", true), Bool("shadows.near_layer", true)],
            ]
            .concat(),
            Quality::Rays => vec![
                Method("shadows.method", ShadowMethod::Rays),
                Bool("light.sun_as_point", false),
                U32("shadows.ray_samples", 64),
                U32("shadows.ray_probe", 16),
            ],
        }
    }

    /// For the settings' button: what it is for, then each field it sets to
    /// which value, a line each.
    pub fn hover(self) -> String {
        let lines: Vec<String> = self.settings().iter().map(Preset::to_string).collect();
        format!("{}\n\n{}", self.describe(), lines.join("\n"))
    }

    fn apply(self, c: &mut Config) {
        for s in self.settings() {
            c.preset_set(s);
        }
    }
}

impl Config {
    /// Whether anything casts a shadow: every `shadows.method` but "none".
    pub fn shadows_on(&self) -> bool {
        self.shadows.method != ShadowMethod::None
    }

    /// Whether the image's floats are the values it stores, decoded for the
    /// sRGB target -- the light stored `"linear"`, or the data mode's colours,
    /// stored as given -- rather than light the target encodes as a screen
    /// shows it: how the passes after the main one read them, the reference's
    /// sum, the MSAA resolve by hand, the camera's response and the stretch.
    pub fn stores_values(&self) -> bool {
        self.shading.color_mode == ColorMode::Data || self.shading.srgb_mode == SrgbMode::Linear
    }

    /// Whether the image is shown through a stretch: one set, in the shaded
    /// colour mode.
    pub fn stretch_on(&self) -> bool {
        self.shading.color_mode == ColorMode::Shaded && !self.stretch.is_identity()
    }

    /// Set the shadow settings for one use (`Quality`).
    pub fn set_quality(&mut self, quality: Quality) {
        quality.apply(self);
    }

    /// Which `Quality` the settings are now, if they are one's: every field
    /// it sets at its value.
    pub fn quality(&self) -> Option<Quality> {
        Quality::ALL.into_iter().find(|q| q.settings().into_iter().all(|s| self.preset_holds(s)))
    }

    /// A `Quality`'s field set, by the name a script writes it with.
    fn preset_set(&mut self, s: Preset) {
        use Preset::*;
        match s {
            Method("shadows.method", v) => self.shadows.method = v,
            Bool("light.sun_as_point", v) => self.light.sun_as_point = v,
            Bool("shadows.per_body", v) => self.shadows.per_body = v,
            Bool("shadows.cache", v) => self.shadows.cache = v,
            Bool("shadows.second_depth", v) => self.shadows.second_depth = v,
            Bool("shadows.near_layer", v) => self.shadows.near_layer = v,
            U32("shadows.pcf", v) => self.shadows.pcf = v,
            U32("shadows.resolution", v) => self.shadows.resolution = v,
            U32("shadows.ray_samples", v) => self.shadows.ray_samples = v,
            U32("shadows.ray_probe", v) => self.shadows.ray_probe = v,
            F32("shadows.cache_degrees", v) => self.shadows.cache_degrees = v,
            _ => unreachable!("a preset sets a field `preset_set` does not know: {s}"),
        }
    }

    /// Whether a `Quality`'s field holds its value: set on a copy, the copy
    /// no different there.
    fn preset_holds(&self, s: Preset) -> bool {
        let mut c = self.clone();
        c.preset_set(s);
        use Preset::*;
        match s {
            Method("shadows.method", _) => c.shadows.method == self.shadows.method,
            Bool("light.sun_as_point", _) => c.light.sun_as_point == self.light.sun_as_point,
            Bool("shadows.per_body", _) => c.shadows.per_body == self.shadows.per_body,
            Bool("shadows.cache", _) => c.shadows.cache == self.shadows.cache,
            Bool("shadows.second_depth", _) => c.shadows.second_depth == self.shadows.second_depth,
            Bool("shadows.near_layer", _) => c.shadows.near_layer == self.shadows.near_layer,
            U32("shadows.pcf", _) => c.shadows.pcf == self.shadows.pcf,
            U32("shadows.resolution", _) => c.shadows.resolution == self.shadows.resolution,
            U32("shadows.ray_samples", _) => c.shadows.ray_samples == self.shadows.ray_samples,
            U32("shadows.ray_probe", _) => c.shadows.ray_probe == self.shadows.ray_probe,
            F32("shadows.cache_degrees", _) => c.shadows.cache_degrees == self.shadows.cache_degrees,
            _ => unreachable!("a preset sets a field `preset_holds` does not know: {s}"),
        }
    }
}

#[cfg(test)]
mod quality_tests {
    use super::{Config, Quality, ShadowMethod};

    /// Each preset reads back as itself, from the defaults and from settings
    /// another left, and no two are the same.
    #[test]
    fn each_preset_reads_back_as_itself() {
        for start in Quality::ALL {
            for q in Quality::ALL {
                let mut c = Config::default();
                c.set_quality(start);
                c.set_quality(q);
                assert_eq!(c.quality(), Some(q), "{} after {}", q.name(), start.name());
            }
        }
    }

    /// The hover names every field a preset sets, and its value, a line each.
    #[test]
    fn the_hover_lists_every_field_and_value() {
        for q in Quality::ALL {
            let hover = q.hover();
            let lines: Vec<&str> = hover.lines().collect();
            for s in q.settings() {
                assert!(lines.contains(&s.to_string().as_str()), "{}: {s} not a line of {hover:?}", q.name());
            }
        }
        assert!(Quality::Accurate.hover().contains("\nlight.sun_as_point = False\n"));
        assert!(Quality::Point.hover().contains("\nshadows.method = \"maps\"\n"));
    }

    /// The maps' presets pick the maps, so picking one after "rays", or with
    /// none, gives what it says.
    #[test]
    fn a_maps_preset_picks_the_maps() {
        let mut c = Config::default();
        c.set_quality(Quality::Rays);
        assert!(c.shadows.method == ShadowMethod::Rays && !c.light.sun_as_point);
        c.set_quality(Quality::Point);
        assert!(c.shadows.method == ShadowMethod::Maps && c.light.sun_as_point);
        c.shadows.method = ShadowMethod::None;
        c.set_quality(Quality::Fast);
        assert_eq!(c.shadows.method, ShadowMethod::Maps);
    }
}

impl Default for Shadows {
    fn default() -> Self {
        Self {
            method: ShadowMethod::Maps,
            resolution: 4096,
            pcf: 2,
            normal_offset_scale: None,
            bias_scale: None,
            bias_minimum: None,
            access_shadow_map: false,
            per_body: true,
            near_layer: true,
            second_depth: true,
            cache: false,
            cache_degrees: 0.0,
            lod_texels: 2.0,
            lod_pixels: 3.0,
            ray_samples: 64,
            ray_probe: 16,
            ray_guide: false,
            ray_points: 1,
        }
    }
}

/// Facet edges drawn over or instead of the surface.
#[derive(Clone, Debug)]
pub struct Wireframe {
    /// the barycentrics are meaningless and the CPU side warns once.
    /// :range: 0..=2
    pub mode: u32,
    /// Wireframe colour, `(r, g, b, a)`; alpha is dropped. Very dark grey
    /// rather than black by default.
    ///
    /// Mode 2 blends by edge coverage and is antialiased; mode 1 thresholds instead,
    /// because the pipeline blend state is REPLACE and a fractional alpha would be
    /// ignored.
    pub color: wgpu::Color,
    // Line half-width in pixels. Screen-space, so thickness stays constant
    // regardless of distance or zoom.
    /// Wireframe half-width in screen pixels.
    /// :range: 0.1..=10.0
    pub width: f32,
    /// Fade the wireframe out as a body recedes far enough that its facets
    /// stop being resolvable. **Off by default.**
    ///
    /// Past about a pixel per facet the three edges cover the whole triangle,
    /// so the mesh reads as a sheet of wireframe colour -- a shadowed body at
    /// distance comes out grey rather than black, which is the wireframe
    /// overwriting the shading rather than drawing the mesh. This fades it out
    /// between 4 px and 1 px facets instead.
    ///
    /// Distance only: the measure is the facet's *largest* screen height, so
    /// tilt does not trigger it and the limb of a sphere keeps its wireframe.
    ///
    /// Only applies to `wireframe_mode = 2`, where there is a shaded surface
    /// underneath to fade into. Mode 1 is wireframe alone and would simply
    /// vanish.
    pub fade: bool,
    /// Smooth the wireframe's edges, or draw them hard.
    ///
    /// Its own antialiasing, whatever `shading.msaa` is: the mesh's shader
    /// draws the lines and blends each edge in over a pixel. Off, a pixel is
    /// wire or surface. Mode 1 is always hard, having nothing to blend into.
    pub antialias: bool,
}

impl Default for Wireframe {
    fn default() -> Self {
        Self {
            mode: 2,
            color: wgpu::Color { r: 0.01, g: 0.01, b: 0.01, a: 1.0 },
            width: 1.0,
            fade: false,
            antialias: true,
        }
    }
}

/// The picked facet and the facet labels.
#[derive(Clone, Debug)]
pub struct Selection {
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
    pub labels: bool,
    /// Most facets to label before giving up, per body.
    ///
    /// A guard rather than a preference: turning labels on with a 3.1M-facet
    /// body would queue three million text draws and stop the frame dead.
    ///
    /// :range: 0..=20000
    pub labels_max: u32,
    /// Size of a facet label, in pixels.
    /// :range: 4.0..=48.0
    pub label_size: f32,
    /// Colour of a facet label, `(r, g, b, a)`.
    pub label_color: [f32; 4],
    /// Colour a facet takes when it is selected, `(r, g, b, a)`.
    ///
    /// Selecting writes this onto the facet's own vertices and marks them
    /// colour-mode 1, which the shader honours for that facet alone -- so a
    /// picked facet is unlit and this colour while the rest of the body keeps
    /// its shading. Deselecting puts back what was there.
    ///
    pub color: wgpu::Color,
}

impl Default for Selection {
    fn default() -> Self {
        Self {
            labels: false,
            labels_max: 2000,
            label_size: 12.0,
            label_color: [1.0, 1.0, 1.0, 1.0],
            color: wgpu::Color {
                r: 1.0,
                g: 1.0,
                b: 0.0,
                a: 1.0,
            },
        }
    }
}

/// Colouring facets from per-facet values.
#[derive(Clone, Debug)]
pub struct Data {
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
    /// Set by name -- any of matplotlib's, `"inferno"`, each reversed with
    /// `_r` after it (`colormap_names`) -- from any Nx3 array, so a matplotlib
    /// colormap can be handed over unchanged, or from a text file of rows of
    /// RGB (`colormap_from_file`). Defaults to greyscale. Chosen, reversed and
    /// loaded in the panel by `colormap_ui`, by hand.
    /// :skip:
    /// :py_custom:
    pub colormap: Vec<[f32; 3]>,
}

impl Default for Data {
    fn default() -> Self {
        Self {
            value_min: None,
            value_max: None,
            colormap: Vec::new(),
        }
    }
}

/// Reference axes, tick labels and the navigation gizmo.
#[derive(Clone, Debug)]
pub struct AxesConfig {
    /// Reference axes drawn around the scene.
    ///
    /// `"off"`, `"box"` (MATLAB), `"panes"` (matplotlib), `"gizmo"` (three
    /// labelled arrows at the origin), `"blender"` (ground grid, Z line and
    /// gizmo). A rendered body alone carries no scale or orientation; these
    /// supply both.
    pub style: crate::app::axes::AxesStyle,
    /// Colour of the axis lines and grid.
    pub color: [f32; 3],
    /// Roughly how many ticks per axis. The step is rounded to 1, 2 or 5
    /// times a power of ten first, so the count lands near this rather than
    /// on it -- a figure with ticks at 0.0347 is unreadable.
    /// :range: 1..=20
    pub ticks: usize,
    /// Appended to every tick label, e.g. `" km"`.
    ///
    /// The renderer knows the mesh is 0.437 across but not whether that is
    /// metres or kilometres, so the unit has to come from the script.
    pub unit: String,
    /// Tick label size in pixels, and their colour.
    /// :range: 4.0..=64.0
    pub label_size: f32,
    pub label_color: [f32; 4],
    /// Which corner the navigation gizmo sits in. Any of the nine HUD
    /// anchors, so it can be moved out of the way of a colour bar or a HUD.
    ///
    /// Drawn by the `"gizmo"` and `"blender"` axes styles and by no other.
    pub gizmo_anchor: HudAnchor,
    /// Half the widget's width, in pixels: a ball centre never sits further
    /// than this from the middle.
    ///
    /// The gizmo's only size knob. Letter height follows it
    /// ([`crate::app::gizmo::label_size`]), the margin to the image edge is
    /// fixed at 20 px, and the letters are black -- each of those was settable
    /// once and none of them was worth setting: a letter is sized and coloured
    /// by the ball it sits on, so the pair could only ever agree or disagree.
    /// :range: 16.0..=200.0
    pub gizmo_size: f32,
    /// Smooth the axes' lines, or draw them hard.
    ///
    /// Their own antialiasing, whatever `shading.msaa` is: each segment is a
    /// thin strip whose shader blends its edges in over a pixel, or cuts them.
    /// Before, the lines were one pixel wide and smoothed by the mesh's MSAA
    /// alone. The ground grid has its own smoothing, and the gizmo's.
    pub antialias: bool,
}

impl Default for AxesConfig {
    fn default() -> Self {
        Self {
            // The gizmo by default: it says which way the view looks and
            // takes the clicks that turn it, and puts nothing in the scene.
            // A script that exports a data product sets `"off"` (or
            // `export.axes = False`), as the Hera examples do.
            style: crate::app::axes::AxesStyle::Gizmo,
            color: [0.45, 0.45, 0.45],
            ticks: 5,
            unit: String::new(),
            label_size: 13.0,
            label_color: [0.85, 0.85, 0.85, 1.0],
            gizmo_anchor: HudAnchor::TopRight,
            gizmo_size: 30.0,
            antialias: true,
        }
    }
}

/// A reference image (`shadows.method` "reference", `app::reference`): a
/// still drawn as exactly as kalast can, each pixel integrated over its area
/// and the Sun's disc, progressively, a sample a frame, to a stated error.
///
/// A sample jitters the camera within the pixel and traces one ray to a point
/// of the Sun's limb-darkened disc -- with `bounces`, the light bounced off the
/// surfaces too -- into the mean of all the samples so far. For a still scene:
/// anything that changes it starts the sum again, but `light.exposure`, which
/// scales the mean and not the sum. The shadows are traced with
/// rays where the GPU can, against the full meshes: the level of detail and
/// MSAA are off, the frame being integrated over its pixels instead. No
/// denoiser, no history, no clamp: a plain mean of fixed sequences, the same
/// image every run. `sim.reference_samples`, `sim.reference_error` and
/// `sim.reference_done` follow it; `sim.reference_image()` gives the mean as
/// floats once done.
#[derive(Clone, Debug)]
pub struct Reference {
    /// The standard error the sum stops at, as a share of the image's
    /// brightest value: the 99.9th percentile of the pixels', from the spread
    /// of four independent estimates of each. 1/510 by default, half an 8-bit
    /// step of an image whose brightest is white.
    ///
    /// The image's own measure: the same sum stops at the same sample
    /// whatever its `light.exposure`, `srgb_mode` or stretch. A stretch that
    /// lifts the shadows shows their noise, which may want it smaller.
    /// :when: shadows.method == ShadowMethod::Reference
    /// :step: 0.0001
    pub error: f32,
    /// The most samples a pixel takes, the error reached or not.
    /// :when: shadows.method == ShadowMethod::Reference
    /// :range: 64..=65536
    pub max_samples: u32,
    /// Sunlight bounced off the surfaces this many times on its way to the
    /// camera, up to 4: what lights a crater's floor in its own shadow.
    ///
    /// Each sample follows a ray from the point seen in a direction drawn
    /// as a Lambert surface scatters (cosine-weighted), to whatever surface
    /// it meets, adds that surface's own sunlight -- one ray to the disc --
    /// as it reflects it, and goes on from there. Every surface on the way
    /// reflects by its body's law (`body.scattering`, Lambert without one)
    /// at the angles the path makes there: the point seen with its colour,
    /// a surface met with the mean of its body's colours. The light that
    /// leaves for space is gone. 0, the default: the Sun's direct light
    /// alone.
    /// :when: shadows.method == ShadowMethod::Reference
    /// :range: 0..=4
    pub bounces: u32,
}

impl Default for Reference {
    fn default() -> Self {
        Self { error: 1.0 / 510.0, max_samples: 4096, bounces: 0 }
    }
}

/// The shaded ground grid of the `blender` axes style.
#[derive(Clone, Debug)]
pub struct Grid {
    /// Shade the `"blender"` style's ground grid instead of drawing it as
    /// line segments. On by default; `False` restores the segments.
    ///
    /// The segments end at the scene bounds, sit at one spacing, and are one
    /// pixel wide because WebGPU has no line width. This computes the grid
    /// per pixel instead: it has no edge, it crossfades between decades as
    /// you zoom -- which is what lets one grid serve a unit cube and a body
    /// 1e4 km away -- and its lines antialias themselves.
    pub enabled: bool,
    /// Width of a grid line, in pixels.
    /// :range: 0.25..=8.0
    pub width: f32,
    /// Cells between thick lines, and the factor between the levels the
    /// crossfade steps through -- the same number seen from two sides.
    /// :range: 2..=100
    pub major: u32,
    /// Colour of the ordinary lines, `(r, g, b, a)`.
    pub color: [f32; 4],
    /// Colour of every `grid_major`-th line.
    pub major_color: [f32; 4],
    /// The axis lines, drawn over the grid so the origin reads without
    /// hunting for it. Two of the three are in the grid's plane and get
    /// drawn; which two depends on which plane that is.
    pub axis_x_color: [f32; 4],
    pub axis_y_color: [f32; 4],
    pub axis_z_color: [f32; 4],
    /// Fade the grid out between these grazing factors: `0.0` is looking
    /// straight down at the ground plane and `1.0` is looking along it.
    /// Without it the horizon is a hard line of aliasing.
    ///
    /// On the angle rather than the distance because the plane is infinite:
    /// what bounds it on screen is the horizon, not the far plane, so a
    /// distance fade never reaches its ramp.
    /// :range: 0.0..=1.0
    pub fade_near: f32,
    /// :range: 0.0..=1.0
    pub fade_far: f32,
}

impl Default for Grid {
    fn default() -> Self {
        Self {
            enabled: true,
            width: 1.0,
            major: 10,
            // **These are linear, and the surface is sRGB.** What reaches
            // the eye is the sRGB encoding of `rgb * alpha`, which is far
            // brighter than the product looks: 0.16 at alpha 0.30 is 0.048
            // linear and **0.24 on screen**. Three rounds of halving these
            // numbers barely changed the picture for exactly that reason --
            // sRGB compresses a 2x linear cut into about 15 % perceived.
            //
            // So they are chosen the other way round now: pick what the line
            // should look like, convert, and divide by the alpha. Blender
            // sits its grid about 0.10 sRGB above its own background, which
            // on black is the entire budget -- 0.055 for a subdivision and
            // 0.19 for a major line.
            //
            // The **ratio** matters as much as the levels. 0.10 against 0.15
            // is 1.5x, and at these brightnesses that is not a difference the
            // eye separates: every tenth line looked like every other line.
            // 3.5x reads as two kinds of line.
            //
            // Measured, not asserted: `abs(axes_on - axes_off)` over a
            // rendered frame, gizmo masked and the coloured axis lines
            // separated out by saturation.
            color: [0.0147, 0.0147, 0.0166, 0.30],
            major_color: [0.0578, 0.0578, 0.0642, 0.52],
            // The same hues, scaled so the dominant channel lands at 0.45
            // on screen rather than 0.86. At full strength they were about
            // ten times the grid they sit in and read as the subject of the
            // picture; they are a reference, not the content.
            axis_x_color: [0.1896, 0.0583, 0.0729, 0.9],
            axis_y_color: [0.1092, 0.1896, 0.0575, 0.9],
            axis_z_color: [0.0699, 0.0998, 0.1896, 0.9],
            // 0.5 starts fading 60 degrees off the normal, which is
            // barely past a three-quarter view and took the ground away
            // while there was still plenty of it to see. The horizon is at
            // 1.0, so this holds the grid to within ~25 degrees of edge-on.
            fade_near: 0.9,
            fade_far: 1.0,
        }
    }
}

/// The text overlays.
#[derive(Clone, Debug)]
pub struct HudConfig {
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
    pub font: String,
    /// Smooth the text drawn over the image, or draw it hard: the HUDs and
    /// every label -- the axes', the facets', the colour bar's, the gizmo's.
    ///
    /// Text has its own antialiasing, from the font, which `shading.msaa`
    /// never touches. Off, a pixel is text or not: the text is drawn on a
    /// layer of its own and copied over wherever it covers half the pixel,
    /// opaque.
    pub antialias: bool,
}

impl Default for HudConfig {
    fn default() -> Self {
        Self {
            font: String::new(),
            antialias: true,
        }
    }
}

/// Frame export.
#[derive(Clone, Debug)]
pub struct Export {
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
    pub sync: bool,
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
    pub max_queued: u32,
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
    pub dir: String,
    /// Burn the HUD text into exported frames as well as drawing it on screen.
    ///
    /// On by default: an exported frame is what the window shows, the run
    /// state a HUD writes -- a date, an iteration -- included. On, it costs
    /// one extra text pass, on exported frames only. Off for a data product
    /// with a HUD on screen: the HUD is drawn onto the swapchain after the
    /// scene has been copied out, so the export carries the render alone.
    pub hud: bool,
    /// Keep the axes -- grid, box or panes, gizmo, and their labels -- in
    /// exported frames as well as on screen.
    ///
    /// On by default, as `hud` is: an exported frame is what the window shows.
    /// Off, frames that are exported draw the axes in a second pass after the
    /// copy, so the window keeps them and the export has the scene alone; other
    /// frames are drawn as before.
    pub axes: bool,
}

impl Default for Export {
    fn default() -> Self {
        Self {
            sync: false,
            max_queued: 64,
            dir: "out/frames".to_string(),
            hud: true,
            axes: true,
        }
    }
}

/// Mouse and keyboard sensitivities.
#[derive(Clone, Debug)]
pub struct Controls {
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
    /// Two-finger swipe on a trackpad orbits, `shift` pans and `ctrl` zooms; off, it zooms like a wheel.
    ///
    /// Blender's default trackpad map. A trackpad is told from a wheel by
    /// what it reports -- pixels against notches -- which is how Blender
    /// tells them apart too, so a Magic Mouse counts as a trackpad here as
    /// it does there; this is the switch for it. A pinch zooms either way.
    pub trackpad_orbit: bool,
}

impl Default for Controls {
    fn default() -> Self {
        Self {
            sensitivity_move: 1.0,
            sensitivity_look: 1.0,
            sensitivity_rotate: 1.0,
            sensitivity_zoom: 1.0,
            emulate_middle_button: cfg!(target_os = "macos"),
            trackpad_orbit: true,
        }
    }
}

/// The size of the image being rendered, as distinct from the window.
#[derive(Clone, Debug)]
pub struct Image {
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
    /// Mirror the image left to right: the camera's right drawn on the left.
    ///
    /// For an instrument whose images are stored mirrored, so a frame
    /// compares pixel for pixel: pixel `(0, 0)` of an image is its top-left
    /// corner, which a camera fills with the top left of what it sees, and
    /// these put another corner of the view there -- `flip_y` the bottom
    /// left, both the bottom right. The scene is mirrored, on screen and in
    /// an exported frame; the HUD and the colour bar are not. Picking, the
    /// navigation gizmo and the mouse follow the mirror.
    /// :label: mirror left to right
    pub flip_x: bool,
    /// Mirror the image top to bottom: the camera's up drawn at the bottom.
    /// :label: mirror top to bottom
    pub flip_y: bool,
}

impl Default for Image {
    fn default() -> Self {
        Self {
            width: 0,
            height: 0,
            flip_x: false,
            flip_y: false,
        }
    }
}

/// What the camera does to the image it records (`app::response`): its
/// point spread -- a Gaussian core and up to two exponential halos -- and a
/// smear along a line, the image moved across the sensor during the
/// exposure. The image drawn, exported and read (`sim.reference_image()`) is
/// the scene through it; not the HUD, the axes or the colour bar.
///
/// Applied to the image's floats by an FFT on the GPU, the image padded to
/// twice its size: each part of the point spread normalised over the
/// offsets of that padded image, the light it spreads further put back
/// within it, as `examples/phobos_shadows_fit/hrsc.py`'s response has it --
/// whose fit of the HRSC SRC's response these parameters are made for
/// (`notes/2026-10-12_phobos_hrsc_shadows`). Without the reference, the
/// image is then drawn single-sampled into floats, as a reference's sample
/// is, the response smoothing its edges instead of MSAA: a 1018-pixel image
/// is a 2048 x 2048 transform a frame.
#[derive(Clone, Debug)]
pub struct Response {
    /// Apply it. Off by default.
    pub enabled: bool,
    /// The core's standard deviation, pixels: a Gaussian holding the light
    /// the halos do not. 0: a point.
    /// :when: enabled == true
    /// :range: 0.0..=16.0
    pub core: f32,
    /// The first halo's share of the light, spread as `exp(-r / halo_scale)`.
    /// The halos' shares over 1 together are scaled down to 1.
    /// :when: enabled == true
    /// :range: 0.0..=1.0
    pub halo_share: f32,
    /// The first halo's scale, pixels.
    /// :when: enabled == true
    /// :range: 0.0..=2000.0
    pub halo_scale: f32,
    /// The second halo's share of the light, spread as
    /// `exp(-r / halo2_scale)`.
    /// :when: enabled == true
    /// :range: 0.0..=1.0
    pub halo2_share: f32,
    /// The second halo's scale, pixels.
    /// :when: enabled == true
    /// :range: 0.0..=2000.0
    pub halo2_scale: f32,
    /// The smear's length, pixels: each point drawn out to a segment, the
    /// image's motion across the sensor over the exposure. 0: none.
    /// :when: enabled == true
    /// :range: 0.0..=64.0
    pub smear: f32,
    /// The smear's direction, degrees from the image's x axis toward its y
    /// axis, down the image: 0 along the rows.
    /// :when: enabled == true
    /// :range: -180.0..=180.0
    pub smear_angle: f32,
}

impl Default for Response {
    fn default() -> Self {
        Self {
            enabled: false,
            core: 0.0,
            halo_share: 0.0,
            halo_scale: 0.0,
            halo2_share: 0.0,
            halo2_scale: 0.0,
            smear: 0.0,
            smear_angle: 0.0,
        }
    }
}

impl Response {
    /// The parameters as bits, to know when the kernel must be laid out
    /// again (`app::response`).
    pub fn bits(&self) -> [u32; 7] {
        [
            self.core.to_bits(),
            self.halo_share.to_bits(),
            self.halo_scale.to_bits(),
            self.halo2_share.to_bits(),
            self.halo2_scale.to_bits(),
            self.smear.to_bits(),
            self.smear_angle.to_bits(),
        ]
    }
}

/// How the image's values are shown (`app::stretch`), as SAO DS9 scales an
/// image: each value taken from between two limits, the low one black and
/// the high one white, through a function -- a logarithm, a square root, ...
/// -- for faint and bright to show together. The window and exported frames
/// show it; `sim.reference_image()` still reads the values themselves, and
/// the HUD, the axes and the colour bar are drawn over it as they are. In the
/// shaded colour mode only: the data mode's colours are data.
///
/// The values are the image's -- `exposure * I/F` -- before `srgb_mode`
/// stores them, the largest of a pixel's three channels; the background
/// counts. The limits come from all of them, each frame, on the GPU. Off --
/// `"linear"` between `"user"` limits of 0 and 1, the default -- the image is
/// drawn as it was. On, it is drawn into floats first, single-sampled as a
/// camera's response has it.
#[derive(Clone, Debug)]
pub struct Stretch {
    /// The function between the limits, DS9's: `"linear"`, the default,
    /// `"log"`, `"pow"`, `"sqrt"`, `"squared"`, `"asinh"`, `"sinh"`, or
    /// `"histequ"`, histogram equalisation.
    pub scale: StretchScale,
    /// The values shown black and white: `"user"`, the default, `low` and
    /// `high`; `"minmax"`, the image's least and greatest; `"percent"`;
    /// `"zscale"`, IRAF's; or `"zmax"`, zscale's low and the greatest.
    pub limits: StretchLimits,
    /// With `"percent"` limits: the share of the values between them, the
    /// rest cut as much below as above. 99.5 by default.
    /// :when: limits == StretchLimits::Percent
    /// :range: 50.0..=100.0
    pub percent: f32,
    /// With `"user"` limits: the value shown black, in the image's values
    /// -- 1 is white unstretched.
    /// :when: limits == StretchLimits::User
    /// :step: 0.001
    pub low: f32,
    /// With `"user"` limits: the value shown white.
    /// :when: limits == StretchLimits::User
    /// :step: 0.001
    pub high: f32,
    /// The `"log"` and `"pow"` scales' exponent `a`: DS9's 1000 by default.
    /// :when: scale.has_exponent()
    /// :step: 10.0
    pub exponent: f32,
    /// zscale's contrast: IRAF's 0.25 by default; less widens the limits.
    /// :when: limits.zscale()
    /// :range: 0.01..=1.0
    pub contrast: f32,
}

impl Default for Stretch {
    fn default() -> Self {
        Self {
            scale: StretchScale::Linear,
            limits: StretchLimits::User,
            percent: 99.5,
            low: 0.0,
            high: 1.0,
            exponent: 1000.0,
            contrast: 0.25,
        }
    }
}

impl Stretch {
    /// Nothing done: linear from 0 to 1, the image as drawn.
    pub fn is_identity(&self) -> bool {
        self.scale == StretchScale::Linear && self.limits == StretchLimits::User && self.low == 0.0 && self.high == 1.0
    }
}

/// The function a stretch shows its values through (`stretch.scale`), of
/// `x` from 0 at the low limit to 1 at the high one: DS9's, each made 1 at
/// 1 exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StretchScale {
    /// `x`. The default.
    Linear,
    /// `log(a x + 1) / log(a + 1)`, `a` the exponent: the faint lifted.
    Log,
    /// `(a^x - 1) / (a - 1)`: the bright spread, the inverse of log.
    Pow,
    /// The square root of `x`.
    Sqrt,
    /// `x` squared.
    Squared,
    /// `asinh(10 x) / asinh(10)`: linear in the faint, logarithmic above.
    Asinh,
    /// `sinh(3 x) / sinh(3)`.
    Sinh,
    /// Histogram equalisation: each value shown as the share of the values
    /// between the limits under it.
    Histequ,
}

impl StretchScale {
    pub const ALL: [StretchScale; 8] =
        [Self::Linear, Self::Log, Self::Pow, Self::Sqrt, Self::Squared, Self::Asinh, Self::Sinh, Self::Histequ];

    /// Parsed from Python, where these are plain strings.
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.name() == s.to_ascii_lowercase())
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Linear => "linear",
            Self::Log => "log",
            Self::Pow => "pow",
            Self::Sqrt => "sqrt",
            Self::Squared => "squared",
            Self::Asinh => "asinh",
            Self::Sinh => "sinh",
            Self::Histequ => "histequ",
        }
    }

    /// The number `stretch.wgsl` takes.
    pub fn code(&self) -> u32 {
        Self::ALL.iter().position(|m| m == self).unwrap() as u32
    }

    /// Whether `exponent` is used: `"log"` and `"pow"`.
    pub fn has_exponent(&self) -> bool {
        matches!(self, Self::Log | Self::Pow)
    }
}

/// How a stretch's limits are found (`stretch.limits`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StretchLimits {
    /// `low` and `high`, as given. The default.
    User,
    /// The image's least value and its greatest.
    Minmax,
    /// The values `percent` of them lie between, as many cut below as above:
    /// from a histogram, to about 1 %.
    Percent,
    /// IRAF's zscale, as DS9 and astropy have it: a line fitted to 1000 of
    /// the values, sorted, the outliers cut, its slope over `contrast` about
    /// the median. For a sky and its faint objects.
    Zscale,
    /// zscale's low limit, and the greatest value.
    Zmax,
}

impl StretchLimits {
    pub const ALL: [StretchLimits; 5] = [Self::User, Self::Minmax, Self::Percent, Self::Zscale, Self::Zmax];

    /// Parsed from Python, where these are plain strings.
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.name() == s.to_ascii_lowercase())
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Minmax => "minmax",
            Self::Percent => "percent",
            Self::Zscale => "zscale",
            Self::Zmax => "zmax",
        }
    }

    /// The number `stretch.wgsl` takes.
    pub fn code(&self) -> u32 {
        Self::ALL.iter().position(|m| m == self).unwrap() as u32
    }

    /// Whether zscale's fit is made: `"zscale"` and `"zmax"`.
    pub fn zscale(&self) -> bool {
        matches!(self, Self::Zscale | Self::Zmax)
    }
}

/// Diagnostics and console output.
#[derive(Clone, Debug)]
pub struct Diagnostics {
    /// Print app lifecycle events: pause and camera-mode changes.
    pub app: bool,
    /// Print window and GPU setup: chosen surface format, adapter and device
    /// features, and **the present modes the surface supports**.
    ///
    /// Worth enabling once on any new machine -- it is how the vsync cap that
    /// invalidated a whole benchmark was identified.
    pub window: bool,
    /// Print per-mesh detail as meshes are uploaded.
    pub window_mesh: bool,
    /// **Does nothing.** The field exists and is settable from Python, but no code
    /// reads it. Left as a placeholder.
    pub simulation: bool,
    /// Time each GPU pass with timestamp queries, into `sim.gpu_timings()`.
    ///
    /// Off by default: the queries themselves are nearly free, but reading them
    /// back costs a buffer map per frame, and nothing needs it unless someone is
    /// asking where a frame goes. Silently inert where the adapter has no
    /// `TIMESTAMP_QUERY` -- `sim.gpu_timings()` returns an empty dict there.
    ///
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
    pub occlusion_queries: bool,
    pub depth_show: bool,
    /// Free integer passed through to the shader, for one-off experiments.
    /// :range: 0..=10
    pub extra: u32,
}

impl Default for Diagnostics {
    fn default() -> Self {
        Self {
            app: false,
            window: false,
            window_mesh: false,
            simulation: false,
            gpu_timing: false,
            occlusion_queries: false,
            depth_show: false,
            extra: 0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Config {
    /// How the surface is coloured and the image encoded.
    pub shading: Shading,
    /// The Sun as a light: its colour, the ambient floor, the debug cube.
    pub light: Light,
    /// The shadow map and its readback.
    pub shadows: Shadows,
    /// A reference image, integrated progressively to a stated error.
    pub reference: Reference,
    /// Facet edges drawn over or instead of the surface.
    pub wireframe: Wireframe,
    /// The picked facet and the facet labels.
    pub selection: Selection,
    /// Colouring facets from per-facet values.
    pub data: Data,
    /// The colour scale drawn for `data`.
    pub colorbar: Colorbar,
    /// Reference axes, tick labels and the navigation gizmo.
    pub axes: AxesConfig,
    /// The shaded ground grid of the `blender` axes style.
    pub grid: Grid,
    /// The text overlays.
    pub hud: HudConfig,
    /// Frame export.
    pub export: Export,
    /// Mouse and keyboard sensitivities.
    pub controls: Controls,
    /// The size of the image being rendered, as distinct from the window.
    pub image: Image,
    /// What the camera does to the image it records.
    pub response: Response,
    /// How the image's values are shown, as SAO DS9 scales an image.
    pub stretch: Stretch,
    /// Diagnostics and console output.
    pub debug: Diagnostics,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            shading: Shading::default(),
            light: Light::default(),
            shadows: Shadows::default(),
            reference: Reference::default(),
            wireframe: Wireframe::default(),
            selection: Selection::default(),
            data: Data::default(),
            colorbar: Colorbar::default(),
            axes: AxesConfig::default(),
            grid: Grid::default(),
            hud: HudConfig::default(),
            export: Export::default(),
            controls: Controls::default(),
            image: Image::default(),
            response: Response::default(),
            stretch: Stretch::default(),
            debug: Diagnostics::default(),
        }
    }
}

/// The colours the UI app's panels are drawn in -- the panels only: the
/// scene is the renderer's, shown as an image no theme tints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiTheme {
    /// Catppuccin's Mocha, its darkest flavour. The default.
    CatppuccinMocha,
    /// egui's dark theme, whatever the system's appearance.
    Dark,
}

impl UiTheme {
    /// Parsed from Python, where these are plain strings. Hyphen, underscore
    /// and space all work, since all three get typed.
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().replace(['_', ' '], "-").as_str() {
            "catppuccin-mocha" | "mocha" => Some(Self::CatppuccinMocha),
            "dark" => Some(Self::Dark),
            _ => None,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::CatppuccinMocha => "catppuccin-mocha",
            Self::Dark => "dark",
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
    /// right for the side panel, bottom for the log.
    ///
    /// Independent of `simulation.config.fullscreen`, which is the OS window
    /// and nothing else. Set both to be rid of everything at once; set this
    /// alone and the window stays where it is.
    ///
    /// :section: Panels
    pub focus: bool,
    /// Fold the editor's panels to the window edges; `N` toggles it.
    ///
    /// Toolbar, side panel and log fold to their edges and come back
    /// together; each keeps egui's thin handle, so one can be dragged or
    /// double-clicked back out on its own, and an arrow key folds or unfolds
    /// the panel on that edge. Reads `true` only while all three are folded,
    /// so bringing one out clears it. The halfway house between the full
    /// layout and `focus`, which hides everything and reveals on hover. Set
    /// before `start()` to open the editor folded.
    pub panels_folded: bool,
    /// Fold the toolbar, the top panel, or bring it back; `↑` toggles it.
    ///
    /// Live, and written back: reads `true` while the toolbar is folded,
    /// however it got there -- this field, the key, a drag on its edge, or
    /// `panels_folded`. One per panel; `panels_folded` is all three at once.
    pub toolbar_folded: bool,
    /// Fold the log, the bottom panel, or bring it back; `↓` toggles it.
    ///
    /// Live and written back, like `toolbar_folded`.
    pub log_folded: bool,
    /// Does nothing now: the script is the middle's editor tab, and the left
    /// edge has no panel to fold. Kept so a script that sets it still runs.
    ///
    /// :skip:
    /// No widget: a checkbox that does nothing is worse than none.
    pub script_folded: bool,
    /// Fold the side panel, on the right -- app, simulation and files -- or
    /// bring it back; `→` toggles it.
    ///
    /// Live and written back, like `toolbar_folded`.
    pub simulation_folded: bool,
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
    /// The colours of the UI app's panels: `"catppuccin-mocha"`, the default,
    /// or `"dark"`, egui's own.
    ///
    /// The panels only. The scene -- its background included -- is drawn by
    /// the renderer from `app.simulation.config` and shown as an image no
    /// theme tints, so a frame looks the same under either, on screen and
    /// exported.
    ///
    /// :section: Window
    pub theme: UiTheme,

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

    /// Open the UI app's window where it was left: on its screen, at its
    /// place and size, fullscreen if it was.
    ///
    /// Off, it opens where `monitor`, `window_x`, `window_y`, `width`,
    /// `height` and `start_fullscreen` say, every time; the app tab shows
    /// them then, and remembers them as they are set there.
    ///
    /// :label: remember last window
    pub remember_window: bool,
    /// Window size in physical pixels.
    ///
    /// The *window*, not the render. `simulation.config.width` is the image
    /// inside it, and follows this unless it is set.
    ///
    /// `0`, the default: the size the UI app was left at, or else most of
    /// the screen. Set, it wins over what the app remembers.
    ///
    /// :skip:
    /// In the app tab while `remember_window` is off, beside the screen and
    /// the place: written by hand there (`gui::window_start`).
    pub width: u32,
    /// :skip:
    pub height: u32,
    /// The screen the window opens on: its number in the system's list,
    /// from `"1"`, or part of its name, as `"DELL"` or `"Built-in"`.
    ///
    /// Empty, the default: the screen the UI app was last closed on, where
    /// it was on it -- or else the main screen, the window in its middle.
    /// Set, it wins over what the app remembers, as `width` and `height` do.
    /// Set while the window is open, it moves it there.
    ///
    /// :skip:
    /// A list of the screens in the app tab while `remember_window` is off,
    /// written by hand: a name typed in would move the window at each letter.
    pub monitor: String,
    /// Where on its screen the window opens: its top-left corner from the
    /// screen's, in physical pixels. `-1`, the default: in the screen's
    /// middle, or where the UI app left it. Set while the window is open,
    /// it moves it there.
    ///
    /// :skip:
    /// In the app tab while `remember_window` is off, with `width`.
    pub window_x: i32,
    /// :skip:
    pub window_y: i32,
    /// Open the UI app fullscreen, while `remember_window` is off -- `F` and
    /// the green button then change the window, not this.
    ///
    /// :skip:
    /// In the app tab while `remember_window` is off.
    pub start_fullscreen: bool,

    /// The OS window title.
    pub title: String,
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
    // Present with vsync (wgpu Fifo) instead of uncapped (Immediate).
    // Defaults off, so a measurement is never silently capped. Falls back to
    // the surface's preferred mode if the requested one isn't supported.
    /// Cap the frame rate to the display refresh.
    ///
    /// **Off by default**, because a capped loop reports the monitor rather
    /// than the scene: on a 239 Hz panel the render loop measured exactly
    /// 239.46 it/s regardless of complexity, which made a 3.1M-facet scene
    /// look identical to a 100k one. That trap cost a wrong conclusion once
    /// and was worked around by hand in nine scripts, so it is the default
    /// that is wrong rather than those scripts.
    ///
    /// The price is that an idle viewer redraws as fast as it can instead of
    /// 60 times a second. Set `True` when you are looking at a scene rather
    /// than timing one.
    pub vsync: bool,

    /// Neovim in the script editor: your own `nvim`, run the way VS Code's
    /// Neovim extension runs it -- modes, motions, operators, `:` commands,
    /// registers, macros and mappings -- with the config `neovim_config`
    /// names.
    ///
    /// Needs Neovim 0.10 or newer, as `nvim` on the PATH or named by
    /// `neovim_path`. The config is read with `vim.g.kalast` set, so a part
    /// of it that has no place here can be skipped with `if not vim.g.kalast`
    /// -- the way `vim.g.vscode` is used for VS Code. Off, the editor takes
    /// VS Code's keys.
    ///
    /// :section: Editor
    pub neovim: bool,
    /// The Neovim to run when `neovim` is on; empty for `nvim` on the PATH.
    ///
    /// :label: neovim path
    pub neovim_path: String,
    /// The config Neovim reads: `"kalast"`, the one kalast ships; `"user"`,
    /// your own, where Neovim looks for it; or a path, to a config folder
    /// holding `init.lua` or to one file.
    ///
    /// kalast's is its author's: lazy.nvim and its plugins, which it clones
    /// with git the first time it starts, into a folder of its own
    /// (`~/.local/share/kalast-nvim`), apart from your own Neovim's.
    ///
    /// :label: neovim config
    pub neovim_config: String,
    /// The column the script editor draws a vertical line at, 0 for none:
    /// `editor.rulers` in VS Code, `colorcolumn` in Vim.
    ///
    /// :range: 0..=200
    pub ruler: u32,
    /// Completion, hover, signatures and errors in the script editor, from a
    /// language server -- pyright for Python, rust-analyzer for Rust -- the
    /// servers VS Code runs for its own.
    ///
    /// Each runs as a process of its own at low priority, so a simulation
    /// never waits on it. One that is not installed is skipped, and the log
    /// says how to install it.
    ///
    /// :label: language servers
    pub language_servers: bool,
    /// The command starting the Python language server; empty for the first
    /// of `basedpyright-langserver`, `pyright-langserver`, `pylsp` and
    /// `jedi-language-server` found.
    ///
    /// :label: python server
    pub python_language_server: String,
    /// The command starting the Rust language server; empty for
    /// `rust-analyzer`.
    ///
    /// :label: rust server
    pub rust_language_server: String,

    /// Ask GitHub for a newer release when the UI app opens, and offer it in
    /// the toolbar. On a thread, so nothing waits on it; never when a script
    /// runs its own window. Off, kalast touches the network at no point.
    ///
    /// :section: Updates
    /// :label: check for updates
    pub check_updates: bool,
    /// Offer betas too: the pre-release of the next version, built from main
    /// and replaced as it moves on, beside the releases. Remembered.
    ///
    /// :label: offer betas
    pub update_betas: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            editor: false,
            focus: false,
            panels_folded: false,
            toolbar_folded: false,
            log_folded: false,
            script_folded: false,
            simulation_folded: false,
            theme: UiTheme::CatppuccinMocha,
            open_in_background: false,
            remember_window: true,
            width: 0,
            height: 0,
            monitor: String::new(),
            window_x: -1,
            window_y: -1,
            start_fullscreen: false,
            toolbar: "iteration {drawn}    {fps} fps".to_string(),
            title: "kalast".to_string(),
            fullscreen: false,
            vsync: false,
            neovim: false,
            neovim_path: String::new(),
            neovim_config: "kalast".to_string(),
            ruler: 80,
            language_servers: true,
            python_language_server: String::new(),
            rust_language_server: String::new(),
            check_updates: true,
            update_betas: false,
        }
    }
}

#[cfg(test)]
mod colormap_tests {
    use super::*;

    /// A built-in's name with `_r` after it is its reverse, as matplotlib
    /// names them, and a table is named back from what it holds.
    #[test]
    fn a_colormap_is_named_back_reversed_or_not() {
        let inferno = builtin_colormap("inferno").unwrap();
        let reversed = builtin_colormap("inferno_r").unwrap();
        assert!(inferno.iter().rev().eq(reversed.iter()));
        assert_eq!(colormap_name(&reversed).as_deref(), Some("inferno_r"));
        assert_eq!(colormap_name(&[]).as_deref(), Some("gray"), "the default greyscale");
        assert_eq!(colormap_name(&[[1.0, 0.0, 0.0], [0.0, 0.0, 1.0]]), None);
        for name in colormap_names() {
            assert_eq!(colormap_name(&builtin_colormap(name).unwrap()).as_deref(), Some(name));
        }
    }

    /// matplotlib's colormaps, every one, by its names -- its aliases, any
    /// case -- listed once each; a listed one keeps its steps. Asked for: "i
    /// only see 4 builtins colormaps in UI app, can we have all the builtins
    /// colormaps of matplotlib?"
    #[test]
    fn matplotlibs_colormaps_are_all_there() {
        let names = colormap_names();
        for name in ["magma", "viridis", "cividis", "twilight", "turbo", "berlin", "coolwarm", "RdBu", "Blues", "tab10", "gray", "jet"] {
            assert!(names.contains(&name), "{name} not listed");
        }
        assert!(!names.contains(&"grey"), "an alias listed beside its table");
        assert_eq!(builtin_colormap("grey"), builtin_colormap("gray"));
        assert_eq!(builtin_colormap("rdbu"), builtin_colormap("RdBu"));
        assert_eq!(builtin_colormap("gray").unwrap().len(), crate::app::uniform::COLORMAP_SIZE);
        let mut tab10 = builtin_colormap("tab10").unwrap();
        tab10.dedup();
        assert_eq!(tab10.len(), 10, "tab10's ten colours, in steps");
        assert!(builtin_colormap("nonsense").is_none());
    }

    /// A text file of colours: any separator, a header, comments, a fourth
    /// column dropped, 0..255 read as bytes; and what is not a table refused
    /// saying why.
    #[test]
    fn a_colormap_is_read_from_a_text_file() {
        let dir = std::env::temp_dir().join(format!("kalast-colormap-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let write = |name: &str, text: &str| {
            let p = dir.join(name);
            std::fs::write(&p, text).unwrap();
            p
        };
        let csv = write("a.csv", "r,g,b\n# ice\n0,0,0.5\n0.5, 0.5 ,1\n1;1;1;0.2\n");
        assert_eq!(colormap_from_file(&csv).unwrap(), vec![[0.0, 0.0, 0.5], [0.5, 0.5, 1.0], [1.0, 1.0, 1.0]]);
        let bytes = write("b.txt", "0 0 0\n255\t128\t0\n");
        assert_eq!(colormap_from_file(&bytes).unwrap(), vec![[0.0, 0.0, 0.0], [1.0, 128.0 / 255.0, 0.0]]);
        for (name, text, says) in [
            ("c.txt", "0 0\n", "2 values"),
            ("d.txt", "", "no colours"),
            ("e.txt", "0 0 0\nred green blue\n", "not numbers"),
            ("f.txt", "-1 0 0\n", "colours run from"),
            ("g.txt", "0 0 nan\n", "is not a colour value"),
        ] {
            let e = colormap_from_file(&write(name, text)).unwrap_err();
            assert!(e.contains(says), "{name}: {e}");
        }
        assert!(colormap_from_file(&dir.join("none.csv")).unwrap_err().contains("cannot read"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
