use std::sync::Arc;

use glam::Mat4;

use crate::{Float, Vec3};

pub fn light_view_proj(
    pos: Vec3,
    target: Vec3,
    up: Vec3,
    side: Float,
    znear: Float,
    zfar: Float,
) -> Mat4 {
    let dir = (target - pos).normalize();
    let view = Mat4::look_to_rh(pos, dir, up);
    let proj = Mat4::orthographic_rh(-side, side, -side, side, znear, zfar);

    proj * view
}

/// Light matrix for one body's shadow layer.
///
/// Aimed at that body and sized to it, which is the whole point: one map
/// fitted to the entire scene gives a small body almost no texels beside a
/// large one -- 6 km Deimos next to 3,396 km Mars is the case that forced
/// this.
///
/// The depth range is fitted to the **scene**, not to the body, so anything
/// lying between the Sun and this body is still inside the frustum and still
/// casts into its map. That is what keeps mutual shadowing working while the
/// lateral extent stays tight: self and mutual shadows both land in the same
/// layer, at the same resolution.
/// A fitted shadow layer: the matrix to draw with, and the world-space extents
/// it was built from.
///
/// The extents are returned rather than recovered from `view_proj` later. They
/// cannot be recovered: `view_proj` is `projection * view`, so `x_axis.x` is
/// `R[0][0] / side` and inverting it yields `side / |R[0][0]|`, which is right
/// only when the light happens to look down an axis. It was off by 3,788x on
/// Mars at the Hera swing-by geometry -- `R[0][0]` fell below `f32::EPSILON`
/// there, taking a `1.0` fallback, so a 3,788 km body was biased as though it
/// were 1 km across and the automatic normal offset came out at 0.35 m. The
/// symptom was self-shadow acne over the whole disc that no automatic setting
/// could clear.
pub struct LightFit {
    pub view_proj: Mat4,
    /// Half-extent of the orthographic box, world units.
    pub side: Float,
    pub near: Float,
    pub far: Float,
}

pub fn fit_light_view_proj(
    sun_pos: Vec3,
    body: &crate::mesh::Aabb,
    scene: &crate::mesh::Aabb,
    up_world: Vec3,
) -> LightFit {
    let target = body.center();
    let to_body = target - sun_pos;
    let dir = if to_body.length_squared() > 1e-20 {
        to_body.normalize()
    } else {
        Vec3::Z
    };

    // look_to_rh degenerates if up is parallel to dir.
    let up = if dir.dot(up_world).abs() > 0.999 {
        Vec3::X
    } else {
        up_world
    };
    let view = Mat4::look_to_rh(sun_pos, dir, up);

    // Lateral extent: this body alone.
    let mut half = 0.0 as Float;
    for c in body.corners() {
        let p = view.transform_point3(c);
        half = half.max(p.x.abs()).max(p.y.abs());
    }
    let side = (half * 1.05).max(Float::EPSILON);

    // Depth: the whole scene, so occluders in front of this body are kept.
    let (mut near, mut far) = (Float::INFINITY, Float::NEG_INFINITY);
    for c in scene.corners() {
        let d = -view.transform_point3(c).z;
        near = near.min(d);
        far = far.max(d);
    }
    let pad = ((far - near) * 0.01).max(Float::EPSILON);
    let (near, far) = (near - pad, far + pad);

    LightFit {
        view_proj: Mat4::orthographic_rh(-side, side, -side, side, near, far) * view,
        side,
        near,
        far,
    }
}


/// Screen position and alignment for one HUD.
///
/// Right- and bottom-anchored HUDs are placed by glyph_brush's own alignment
/// rather than by measuring the text: the text changes every frame, and
/// measuring it to subtract a width would make the block jitter horizontally
/// as digits change width.
/// Classifies each body against the camera frustum, and the light cube too.
///
/// Conservative in the standard way: a body counts as outside only when
/// *every* corner of its box fails the same plane. A box straddling a corner
/// of the frustum can therefore be reported visible when it is not, which is
/// the safe direction for a diagnostic -- it never claims something is missing
/// when it is there.
fn diagnose(
    simulation: &crate::app::simulation::Simulation,
    config: &crate::app::config::Config,
    view_proj: &glam::Mat4,
) -> crate::app::simulation::Diagnostics {
    use crate::app::simulation::Diagnostics;

    let mut d = Diagnostics {
        n_bodies: simulation.bodies.len(),
        ..Default::default()
    };

    for body in &simulation.bodies {
        let Some(mesh) = body.mesh.as_ref() else {
            d.n_bodies = d.n_bodies.saturating_sub(1);
            continue;
        };
        let bounds = mesh.borrow().bounds.transform(&body.mat);
        if bounds.is_empty() {
            continue;
        }

        let (mut l, mut r, mut b, mut t, mut n, mut f) = (true, true, true, true, true, true);
        for c in bounds.corners() {
            let p = *view_proj * glam::Vec4::new(c.x, c.y, c.z, 1.0);
            l &= p.x < -p.w;
            r &= p.x > p.w;
            b &= p.y < -p.w;
            t &= p.y > p.w;
            // wgpu clip space is z in 0..w, not -w..w.
            n &= p.z < 0.0;
            f &= p.z > p.w;
        }

        // Near and far first: a body behind the camera also fails the side
        // tests, and "behind you" is the more useful thing to be told.
        if n {
            d.out_near += 1;
        } else if f {
            d.out_far += 1;
        } else if l || r || b || t {
            d.out_side += 1;
        } else {
            d.n_visible += 1;
        }
    }

    if config.debug_light_cube_show {
        let p = *view_proj
            * glam::Vec4::new(
                simulation.sun.pos.x,
                simulation.sun.pos.y,
                simulation.sun.pos.z,
                1.0,
            );
        // Only the far plane: the cube being off to one side is the user
        // looking elsewhere, which is not a fault worth warning about.
        d.light_cube_clipped = p.z > p.w;
    }

    d
}

/// Tick labels and caption for the colour scale, already in screen pixels.
///
/// Ticks come from the same rounding the axes use, so a scale from 87 to 349
/// is labelled 100, 200, 300 rather than at the raw ends.
fn colorbar_labels(
    cb: &crate::app::config::Colorbar,
    rect: (f32, f32, f32, f32),
    lo: f32,
    hi: f32,
) -> Vec<(String, (f32, f32), wgpu_text::glyph_brush::HorizontalAlign)> {
    use wgpu_text::glyph_brush::HorizontalAlign;

    let (left, top, w, h) = rect;
    let vertical = cb.is_vertical();
    let mut out = Vec::new();

    // The lighting source is a fraction by construction, so its ends are 0
    // and 1 whatever the data happens to span.
    let (lo, hi) = match cb.source {
        crate::app::config::ColorbarSource::Lighting => (0.0, 1.0),
        crate::app::config::ColorbarSource::Values => (lo, hi),
    };

    let gap = 6.0;
    for v in crate::app::axes::tick_values(lo as crate::Float, hi as crate::Float, cb.ticks) {
        let t = if (hi - lo).abs() > f32::EPSILON {
            (v as f32 - lo) / (hi - lo)
        } else {
            0.0
        };
        let text = crate::app::axes::format_value(
            v,
            crate::app::axes::step_for(lo as crate::Float, hi as crate::Float, cb.ticks),
        );
        if vertical {
            // Up the right-hand edge, since a vertical bar is usually parked
            // against a side of the frame with room on its inside.
            out.push((
                text,
                (left + w + gap, top + h * (1.0 - t)),
                HorizontalAlign::Left,
            ));
        } else {
            out.push((text, (left + w * t, top + h + gap), HorizontalAlign::Center));
        }
    }

    if !cb.label.is_empty() {
        let pos = if vertical {
            (left + w * 0.5, top - gap * 2.0)
        } else {
            (left + w * 0.5, top - gap * 2.0)
        };
        out.push((cb.label.clone(), pos, HorizontalAlign::Center));
    }

    out
}

/// Screen positions for the axis tick labels.
///
/// Computed up front rather than inside the draw, because the text brush is
/// borrowed mutably there and these read `self`. Labels behind the camera are
/// dropped -- a point with `w <= 0` projects to a mirrored position in front,
/// which would scatter tick numbers across the wrong side of the frame.
fn axis_label_screen(
    labels: &[crate::app::axes::Label],
    view_proj: &glam::Mat4,
    unit: &str,
    width: f32,
    height: f32,
) -> Vec<(String, (f32, f32))> {
    labels
        .iter()
        .filter_map(|l| {
            let clip = *view_proj * glam::Vec4::new(l.world.x, l.world.y, l.world.z, 1.0);
            if clip.w <= 0.0 {
                return None;
            }
            let ndc = clip.truncate() / clip.w;
            let x = (ndc.x * 0.5 + 0.5) * width;
            // NDC y is up, screen y is down.
            let y = (0.5 - ndc.y * 0.5) * height;
            if !x.is_finite() || !y.is_finite() {
                return None;
            }
            let text = if l.unit {
                format!("{}{}", l.text, unit)
            } else {
                l.text.clone()
            };
            Some((text, (x as f32, y as f32)))
        })
        .collect()
}

fn hud_placement(
    hud: &crate::app::config::Hud,
    width: f32,
    height: f32,
) -> ((f32, f32), wgpu_text::glyph_brush::Layout<wgpu_text::glyph_brush::BuiltInLineBreaker>) {
    use crate::app::config::{HAlign, HudAnchor::*, VAlign};
    use wgpu_text::glyph_brush::{HorizontalAlign, Layout, VerticalAlign};

    // `x`/`y` are an inset *from* the anchor, so the same offset means "8 px
    // in from my corner" whichever corner that is. Centre anchors take the
    // inset as a signed nudge instead: there is no edge to come in from.
    let x = match hud.anchor {
        TopLeft | MiddleLeft | BottomLeft => hud.x,
        TopCenter | MiddleCenter | BottomCenter => width * 0.5 + hud.x,
        TopRight | MiddleRight | BottomRight => width - hud.x,
    };
    let y = match hud.anchor {
        TopLeft | TopCenter | TopRight => hud.y,
        MiddleLeft | MiddleCenter | MiddleRight => height * 0.5 + hud.y,
        BottomLeft | BottomCenter | BottomRight => height - hud.y,
    };

    // Alignment is glyph_brush's, not ours: measuring the text to subtract a
    // width would make the block jitter horizontally as digits change width.
    let (dh, dv) = hud.anchor.default_align();
    let h = match hud.align_h.unwrap_or(dh) {
        HAlign::Left => HorizontalAlign::Left,
        HAlign::Center => HorizontalAlign::Center,
        HAlign::Right => HorizontalAlign::Right,
    };
    let v = match hud.align_v.unwrap_or(dv) {
        VAlign::Top => VerticalAlign::Top,
        VAlign::Center => VerticalAlign::Center,
        VAlign::Bottom => VerticalAlign::Bottom,
    };

    ((x, y), Layout::default_wrap().h_align(h).v_align(v))
}


/// Standard font directories for this platform, most specific first, so a
/// user-installed font beats a system one of the same name.
fn font_dirs() -> Vec<std::path::PathBuf> {
    let home = std::env::var("HOME").ok().map(std::path::PathBuf::from);
    let mut dirs: Vec<std::path::PathBuf> = Vec::new();

    if cfg!(target_os = "macos") {
        if let Some(h) = &home {
            dirs.push(h.join("Library/Fonts"));
        }
        dirs.push("/Library/Fonts".into());
        dirs.push("/System/Library/Fonts".into());
        // Where macOS keeps Arial, Times New Roman and the rest of the
        // names people actually type.
        dirs.push("/System/Library/Fonts/Supplemental".into());
    } else if cfg!(target_os = "windows") {
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            dirs.push(std::path::PathBuf::from(local).join("Microsoft/Windows/Fonts"));
        }
        dirs.push("C:/Windows/Fonts".into());
    } else {
        if let Some(h) = &home {
            dirs.push(h.join(".local/share/fonts"));
            dirs.push(h.join(".fonts"));
        }
        dirs.push("/usr/local/share/fonts".into());
        dirs.push("/usr/share/fonts".into());
    }
    dirs
}

/// `Arial`, `arial`, `Arial Black` and `arialblack` all match `Arial.ttf`.
///
/// Matching is on the *filename*, not the family name recorded inside the
/// font. Reading the real family would need a font database dependency; for
/// an overlay, filenames cover the names anyone types and cost nothing.
fn font_key(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Depth-limited search for a font file whose stem matches `name`.
fn find_font_file(name: &str) -> Option<std::path::PathBuf> {
    let want = font_key(name);
    let exts = ["ttf", "otf", "ttc", "otc"];

    fn walk(dir: &std::path::Path, depth: usize, out: &mut Vec<std::path::PathBuf>) {
        if depth == 0 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, depth - 1, out);
            } else {
                out.push(p);
            }
        }
    }

    for dir in font_dirs() {
        let mut files = Vec::new();
        // 3 is enough for Linux's /usr/share/fonts/truetype/dejavu nesting
        // without walking an entire home directory by accident.
        walk(&dir, 3, &mut files);
        for p in files {
            let is_font = p
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| exts.contains(&e.to_ascii_lowercase().as_str()))
                .unwrap_or(false);
            if !is_font {
                continue;
            }
            if p.file_stem().and_then(|s| s.to_str()).map(font_key).as_deref() == Some(&want) {
                return Some(p);
            }
        }
    }
    None
}

/// The HUD font: `config.hud_font` if it resolves, otherwise the built-in one.
///
/// Accepts either a path or a font name -- `"Arial"` and
/// `"/Library/Fonts/Arial.ttf"` both work. A path is anything that exists on
/// disk; everything else is looked up by name in the platform's font
/// directories.
///
/// A font that will not resolve warns and falls back rather than returning
/// `None`. Losing the whole HUD because a name was mistyped would hide the
/// counters a long run is being watched by, which is worse than the wrong
/// typeface.
fn hud_font(spec: &str) -> Option<wgpu_text::glyph_brush::ab_glyph::FontArc> {
    use wgpu_text::glyph_brush::ab_glyph::{FontArc, FontVec};

    let builtin =
        || FontArc::try_from_slice(include_bytes!("../../res/DejaVuSans.ttf")).ok();

    if spec.is_empty() {
        return builtin();
    }

    let path = if std::path::Path::new(spec).exists() {
        Some(std::path::PathBuf::from(spec))
    } else {
        find_font_file(spec)
    };

    let Some(path) = path else {
        eprintln!(
            "hud_font: no font named {spec:?} in {:?}, using built-in",
            font_dirs()
        );
        return builtin();
    };

    match std::fs::read(&path) {
        Ok(bytes) => {
            // `.ttc`/`.otc` are collections; take the first face.
            match FontVec::try_from_vec(bytes.clone())
                .or_else(|_| FontVec::try_from_vec_and_index(bytes, 0))
                .map(FontArc::from)
            {
                Ok(font) => Some(font),
                Err(e) => {
                    eprintln!("hud_font: {path:?} is not a usable font ({e}), using built-in");
                    builtin()
                }
            }
        }
        Err(e) => {
            eprintln!("hud_font: cannot read {path:?} ({e}), using built-in");
            builtin()
        }
    }
}

type HudBrush = wgpu_text::TextBrush<wgpu_text::glyph_brush::ab_glyph::FontArc>;

pub struct Window {
    /// Draws `Simulation::hud` over the swapchain. `None` if the font would
    /// not load, which is not worth failing a run over.
    pub hud: Option<HudBrush>,
    pub window: Arc<winit::window::Window>,
    pub instance: wgpu::Instance,
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub surface_config: wgpu::SurfaceConfiguration,
    pub is_surface_configured: bool,

    /// Tick labels for the axes, in world space. Projected to screen at draw
    /// time rather than stored as pixels, since the camera moves.
    pub axes_labels: Vec<super::axes::Label>,

    /// Where the bar landed, in pixels `(left, top, width, height)`, so the
    /// tick labels can be placed against it. `None` when it is off.
    pub colorbar_px: Option<(f32, f32, f32, f32)>,

    // 0: white cube
    // 1..: loaded by user in app.simulation.bodies
    pub meshes: Vec<super::gpu::MeshBuffer>,

    // Parallel to `meshes`: `Some` where a body supplied a lower-resolution
    // shadow stand-in, `None` to fall back to the entry in `meshes`. Same
    // indexing (element 0 is the light cube), so the shadow pass can pick
    // per body without a second lookup.
    pub shadow_meshes: Vec<Option<super::gpu::MeshBuffer>>,

    pub uniforms: super::uniform::Uniforms,
    pub passes: super::pass::Passes,

    pub export_frame: bool,
    pub frame_exporter: super::gpu::FrameExporter,

    // Built lazily: most runs never ask for a per-facet shadow query, and
    // compiling the compute pipeline is not free.
    pub facet_shadow: Option<super::facet_shadow::FacetShadowQuery>,

    // Same lazy treatment: the ID pass allocates two full-resolution
    // textures, which is wasted on any run that never asks for one.
    pub facet_id: Option<super::facet_id::FacetIdPass>,

    // Built on first use, like the other query passes: it allocates an atlas
    // and a weight texture that most runs never need.
    pub hemicube: Option<super::hemicube::Hemicube>,

    // Body model matrices as of the last `update`. The facet shadow query
    // needs the same transform the shadow map was built with, and it is
    // called from outside the borrow of `Simulation`.
    pub last_body_mats: Vec<Mat4>,
}

impl Window {
    pub async fn new(
        display: winit::event_loop::OwnedDisplayHandle,
        window: Arc<winit::window::Window>,
        config: &crate::app::config::Config,
        simulation: &crate::app::simulation::Simulation,
    ) -> Self {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle(
            Box::new(display),
        ));
        let surface = instance.create_surface(window.clone()).unwrap();

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptionsBase {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
            })
            .await
            .unwrap();

        // The adapted above isn't guaranteed to work on all devices.
        // In such case, use the adapter auto selection below.
        // let adapter = instance
        //     .enumerate_adapters(wgpu::Backends::all())
        //     .await.iter()
        //     .filter(|adapter| {
        //         adapter.is_surface_supported(&surface)
        //     })
        //     .next()
        //     .unwrap();

        let features_wgpu = wgpu::FeaturesWGPU::empty();
        // features_wgpu.insert(wgpu::FeaturesWGPU::POLYGON_MODE_LINE);

        let features_webgpu = wgpu::FeaturesWebGPU::empty();
        // features_webgpu.insert(wgpu::FeaturesWebGPU::DEPTH32FLOAT_STENCIL8);

        // Features::NON_FILL_POLYGON_MODE
        // Features::POLYGON_MODE_LINE
        // Features::POLYGON_MODE_POINT
        // Features::DEPTH_CLIP_CONTROL
        // Requires Features::CONSERVATIVE_RASTERIZATION

        // Default::default() for required_limits is wgpu::Limits::default(),
        // the conservative cross-backend-safe limits (e.g. 256 MiB max
        // buffer size) -- too small for full-resolution shape models
        // (unflattened Didymos alone needs a ~717 MB vertex buffer), and
        // needlessly so, since native backends (Metal here) typically
        // support far larger buffers. Request the adapter's actual limits
        // instead.
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                required_features: wgpu::Features {
                    features_wgpu,
                    features_webgpu,
                },
                required_limits: adapter.limits(),
                ..Default::default()
            })
            .await
            .unwrap();

        let caps = surface.get_capabilities(&adapter);

        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);

        if config.debug_window {
            println!("{:?}", format);
        }

        let size = window.inner_size();

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            format: format,
            width: size.width,
            height: size.height,
            present_mode: pick_present_mode(&caps, config.vsync),
            desired_maximum_frame_latency: 2,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
        };

        // List of supported configurations by the adapter, device, surface.
        if config.debug_window {
            println!("[WINDOW] adapter features: {}", adapter.features());
            println!("[WINDOW] device features: {}", device.features());
            println!(
                "[WINDOW] surface capabilities present modes: {:?}",
                caps.present_modes
            );
        }

        let mut meshes = vec![];

        // TODO: ADD COLOR PER MESH?
        // InstanceInput is full in location (16)
        // need to move that to uniform or something else

        meshes.push(super::gpu::MeshBuffer::new(
            &device,
            &crate::meshes::cube::VERTICES,
            &crate::meshes::cube::INDICES,
            &super::gpu::InstanceInput::default(),
            false,
            &[],
        ));

        // Element 0 pairs with the light cube, which the shadow pass skips.
        let mut shadow_meshes: Vec<Option<super::gpu::MeshBuffer>> = vec![None];

        let mut warned_wireframe = false;

        for body in &simulation.bodies {
            if let Some(mesh) = body.mesh.as_ref() {
                let mesh = mesh.borrow();

                // The wireframe recovers barycentrics from vertex_index,
                // which only holds for flat (non-indexed) meshes. Say so
                // once rather than silently dropping the overlay.
                if config.wireframe_mode != 0 && !mesh.is_flat() && !warned_wireframe {
                    warned_wireframe = true;
                    println!(
                        "[WINDOW] wireframe needs flat meshes (load with flatten=True); \
                         smooth meshes render shaded only"
                    );
                }

                if config.debug_window_mesh {
                    for v in &mesh.vertices {
                        println!("v: {}", v.pos);
                    }
                    println!("indices: {:?}", &mesh.indices);
                    println!("mat: {:?}", body.mat);
                }

                let instance = super::gpu::InstanceInput::new(body.mat);

                meshes.push(super::gpu::MeshBuffer::new(
                    &device,
                    &mesh.vertices,
                    &mesh.indices,
                    &instance,
                    mesh.is_flat(),
                    &mesh.values,
                ));

                shadow_meshes.push(body.shadow_mesh.as_ref().map(|shadow| {
                    let shadow = shadow.borrow();
                    super::gpu::MeshBuffer::new(
                        &device,
                        &shadow.vertices,
                        &shadow.indices,
                        &instance,
                        shadow.is_flat(),
                        // The shadow stand-in only ever writes depth, so its
                        // colours and values are never read.
                        &[],
                    )
                }));
            }
        }

        /*
        let texture = super::gpu::Texture::new_image_from_bytes(
            &device,
            &queue,
            include_bytes!("../../res/happy-tree.png"),
        );
        let textures = vec![texture];
        */

        let globals = super::gpu::UniformBuffer::new(&device, build_globals(config, None, (0.0, 1.0)));

        let camera = super::uniform::Camera {
            view_proj: simulation
                .camera
                .view_proj(size.width as Float / size.height as Float)
                .unwrap(),
        };

        // Light needsmto be optimized in pos/proj znear/far/side
        // to have optimized shadow mapping resolution and reduce bias effects.

        let light = super::uniform::Light {
            view_proj: simulation
                .sun
                // .view_proj(size.width as Float / size.height as Float)
                .view_proj(1.0)
                .unwrap(),

            pos: simulation.sun.pos,
            color: super::gpu::color_vec3(&config.light_color),
            ..Default::default()
        };

        let view = super::gpu::UniformBuffer::new(&device, super::uniform::View { camera, light });

        // One layer per body, so each gets a shadow map aimed at it and sized
        // to it. Allocated at the cap rather than at the body count, because
        // bodies can be loaded after the window exists.
        let shadow = super::gpu::Texture::create_depth_texture_shadow_pass(
            &device,
            config.shadow_resolution,
            config.shadow_resolution,
            super::uniform::MAX_SHADOW_LAYERS as u32,
        );

        let colormap = super::gpu::UniformBuffer::new(
            &device,
            super::uniform::Colormap::default(),
        );

        let bar = super::gpu::UniformBuffer::new(&device, super::uniform::Bar::default());

        let uniforms = super::uniform::Uniforms {
            globals,
            view,
            shadow,
            colormap,
            bar,
        };

        let passes = super::pass::Passes::new(&device, surface_config.format, &config, &uniforms);

        // The font is embedded rather than read from `res/`, so the overlay
        // works from any working directory. A font that will not load leaves
        // the overlay off rather than failing the run.
        let hud = hud_font(&config.hud_font).map(|font| {
            wgpu_text::BrushBuilder::using_font(font)
            .build(
                &device,
                surface_config.width,
                surface_config.height,
                surface_config.format,
            )
        });

        Self {
            window,
            instance,
            surface,
            device,
            queue,
            surface_config,
            is_surface_configured: false,
            axes_labels: Vec::new(),
            colorbar_px: None,

            meshes,
            shadow_meshes,
            uniforms,
            passes,

            export_frame: false,
            frame_exporter: super::gpu::FrameExporter::new(
                config.export_dir.clone(),
                config.export_sync,
                config.export_max_queued as usize,
            ),

            facet_shadow: None,
            facet_id: None,
            hemicube: None,
            last_body_mats: vec![],
            hud,
        }
    }

    /// Occluded fraction per facet for `body` (index into
    /// `simulation.bodies`), read back from the current shadow map.
    ///
    /// Reads the shadow map as it stands after the last rendered frame, so
    /// call it after at least one frame has been drawn for the epoch you
    /// care about. Blocking -- see `FacetShadowQuery::query`.
    ///
    /// Always queries the full-resolution render mesh, never the coarser
    /// `shadow_path` proxy: the proxy decides what the *map* contains, but
    /// the answer is wanted per real facet.
    pub fn facet_shadow_fractions(&mut self, body: usize) -> Vec<f32> {
        if self.facet_shadow.is_none() {
            self.facet_shadow = Some(super::facet_shadow::FacetShadowQuery::new(&self.device));
        }

        let Some(mesh) = self.meshes.get(1 + body) else {
            return vec![];
        };

        self.facet_shadow.as_ref().unwrap().query(
            &self.device,
            &self.queue,
            &self.uniforms.shadow,
            body.min(super::uniform::MAX_SHADOW_LAYERS - 1),
            mesh,
            self.last_body_mats.get(body).copied().unwrap_or(Mat4::IDENTITY),
            // This body's own layer, not the shared scratch. Each layer is
            // fitted to its body, so querying with the wrong matrix reads a
            // map covering a different volume -- silently wrong occlusion
            // rather than an error, and it feeds the TPM.
            self.uniforms.view.uniform.light.view_proj_layers
                [body.min(super::uniform::MAX_SHADOW_LAYERS - 1)],
            self.uniforms.view.uniform.light.pos,
            self.uniforms.view.uniform.light.layer_bias
                [body.min(super::uniform::MAX_SHADOW_LAYERS - 1)],
        )
    }

    /// Facet index per pixel from the camera's point of view, plus the index
    /// offset applied to each body.
    ///
    /// Renders the scene again through the same view matrix into an integer
    /// target and reads it back. Costs a second geometry pass and a blocking
    /// readback, so it is meant for the frames a product is wanted from, not
    /// for every frame of a long run.
    pub fn facet_id_map(&mut self) -> (Vec<u32>, Vec<u32>, u32, u32) {
        let (w, h) = (self.surface_config.width, self.surface_config.height);
        if self.facet_id.is_none() {
            self.facet_id = Some(super::facet_id::FacetIdPass::new(
                &self.device,
                &self.uniforms.view.layout,
                w,
                h,
            ));
        }
        let pass = self.facet_id.as_mut().unwrap();
        pass.resize(&self.device, w, h);

        let camera_bind_group = self.uniforms.view.bind_group(&self.device);
        let (pixels, offsets) = pass.render_and_read(
            &self.device,
            &self.queue,
            &camera_bind_group,
            &self.meshes[1..],
        );
        (pixels, offsets, w, h)
    }

    /// View-factor rows for the given facets of `body`, by hemicube.
    ///
    /// Every loaded body is rendered into a shared index space, so one row
    /// carries the self view factors alongside the mutual ones. Returns
    /// `(rows, offsets, n_total)`; `rows` is row-major with entry
    /// `[i * n_total + j]` the fraction of energy leaving `facets[i]` that
    /// reaches global facet `j`, and `offsets[b]` is where body `b` starts.
    /// Occlusion is resolved by the depth test -- including by the *other*
    /// body, which is what makes a mutual eclipse block mutual heating.
    ///
    /// The CPU-side `mesh` is passed in rather than looked up: the window
    /// holds GPU buffers, and the facet centroids and normals the hemicubes
    /// are built from live on the simulation side. They are in that body's
    /// own frame, so the body's model matrix is applied here to place the
    /// hemicubes in the same world the meshes are drawn in.
    ///
    /// `near` matters more than it looks. It is tied to the smallest facet,
    /// because a near plane larger than a facet clips away that facet's
    /// immediate neighbours -- which are exactly the ones that dominate
    /// self-heating.
    pub fn hemicube_rows(
        &mut self,
        body: usize,
        mesh: &crate::mesh::Mesh,
        scene: Option<crate::mesh::Aabb>,
        facets: &[u32],
        resolution: u32,
        batch: u32,
    ) -> (Vec<f32>, Vec<u32>, u32) {
        if self.meshes.len() <= 1 + body {
            return (vec![], vec![], 0);
        }
        let model = self
            .last_body_mats
            .get(body)
            .copied()
            .unwrap_or(Mat4::IDENTITY);
        let normal_mat = crate::Mat3::from_mat4(model);

        let radius = mesh.bounds.radius().max(Float::EPSILON);
        let smallest = mesh
            .facets
            .iter()
            .map(|f| f.area)
            .fold(Float::INFINITY, Float::min)
            .max(Float::EPSILON)
            .sqrt();
        let near = (smallest * 1.0e-3).max(radius * 1.0e-7);

        // The far plane has to reach the whole scene, not just this body. A
        // companion sits at a distance set by the orbit, which for a small
        // secondary is many times its own radius: sizing `far` from the
        // requesting body alone put Didymos beyond Dimorphos's far plane at
        // the real 1.15 km separation, leaving a clipped remnant that read as
        // a mutual view factor 20x too small, and exactly zero past 1.5 km.
        // Measured against `(R/d)^2` over a separation sweep -- the falloff
        // now follows it instead of collapsing.
        //
        // Origins are spread over the body's surface rather than sitting at
        // its centre, so the reach is measured from the centre and the body's
        // own radius added back.
        let far = scene
            .map(|s| {
                let own = mesh.bounds.transform(&model);
                let c = own.center();
                let reach = s
                    .corners()
                    .iter()
                    .map(|p| (*p - c).length())
                    .fold(0.0 as Float, Float::max);
                (reach + own.radius()) * 1.01
            })
            .unwrap_or(0.0)
            .max(radius * 4.0);
        let proj = Mat4::perspective_rh(std::f64::consts::FRAC_PI_2 as Float, 1.0, near, far);

        let mut views = Vec::with_capacity(facets.len() * super::hemicube::FACES as usize);
        for &i in facets {
            let Some(facet) = mesh.facets.get(i as usize) else {
                continue;
            };
            let n = facet.normal.normalize_or_zero();
            if n.length_squared() < 0.5 {
                continue;
            }
            // Any tangent will do: the delta form factors are symmetric under
            // rotation about the normal, so the choice changes which side
            // face a given facet lands in, not the total.
            let helper = if n.x.abs() < 0.9 {
                crate::Vec3::X
            } else {
                crate::Vec3::Y
            };
            let t = n.cross(helper).normalize();
            let b = n.cross(t);

            // Lift off the surface so the facet does not fill its own
            // hemicube through depth fighting.
            let o = facet.pos + n * near * 2.0;
            let (o, n, t, b) = (
                model.transform_point3(o),
                (normal_mat * n).normalize_or_zero(),
                (normal_mat * t).normalize_or_zero(),
                (normal_mat * b).normalize_or_zero(),
            );
            for (dir, up) in [(n, t), (t, n), (-t, n), (b, n), (-b, n)] {
                views.push(proj * Mat4::look_to_rh(o, dir, up));
            }
        }

        if self.hemicube.is_none() {
            self.hemicube = Some(super::hemicube::Hemicube::new(
                &self.device,
                &self.queue,
                resolution,
                batch,
            ));
        }
        // Columns come from the shadow proxy where a body has one, rows from
        // the real mesh. The same asymmetry the shadow map exploits: what
        // fills a hemicube is a far-field quantity, so a decimated occluder is
        // as good, while the rows *are* the resolution being asked for.
        //
        // It is not a nicety at scale. The accumulator is
        // `batch * n_columns`, so putting Didymos on the 100k mesh took it
        // from 20 MB to 113 MB and a rebuild from 6 s to 110 s; with a 10k
        // proxy the columns stay at 10k however fine the rows get. Without
        // this the 3.1M mesh is unreachable for mutual heating -- each
        // Dimorphos facet would carry ~150,000 nonzeros.
        //
        // **The body's own columns stay at full resolution.** A proxy is only
        // valid for the companion. The hemicube origin sits on the real
        // surface, while the proxy deviates from it by about a facet, so a
        // fine facet ends up buried inside the coarse hull and sees it in
        // every direction: measured, Didymos's self view-factor row sum went
        // from 0.0011 to 0.3156 and its peak temperature from 352 K to 405 K.
        // That is the same near-field/far-field split that makes the shadow
        // proxy safe -- self view factors are dominated by immediate
        // neighbours, which is exactly what decimation removes.
        let columns: Vec<&super::gpu::MeshBuffer> = (1..self.meshes.len())
            .map(|i| {
                if i == 1 + body {
                    &self.meshes[i]
                } else {
                    self.shadow_meshes
                        .get(i)
                        .and_then(|s| s.as_ref())
                        .unwrap_or(&self.meshes[i])
                }
            })
            .collect();
        let hc = self.hemicube.as_mut().unwrap();
        hc.rows(&self.device, &self.queue, &columns, &views)
    }

    pub fn get_window(&self) -> &winit::window::Window {
        &self.window
    }

    pub fn configure_surface(&self) {
        // todo
    }

    pub fn center_cursor(&self) {
        let width = self.surface_config.width;
        let height = self.surface_config.height;
        let mid = (width / 2, height / 2);
        self.window
            .set_cursor_position(winit::dpi::PhysicalPosition::new(mid.0, mid.1))
            .unwrap();
    }

    pub fn reset_cursor(&self) {
        self.center_cursor();
        self.window.set_cursor_visible(true);
        self.window
            .set_cursor_grab(winit::window::CursorGrabMode::None)
            .unwrap();
    }

    pub fn toggle_export_frame(&mut self) {
        self.export_frame = !self.export_frame;
    }

    pub fn resize(&mut self, width: u32, height: u32, config: &crate::app::config::Config) {
        // Windows reports Resized(0, 0) when the window is minimised, and
        // wgpu rejects a zero-area surface outright ("Both `Surface` width
        // and height must be non-zero"), which is a panic rather than an
        // error we could recover from. macOS does not do this, so a run that
        // is fine on the laptop dies here the moment anything minimises the
        // window -- it killed a phase 2 segment at step 105 of 1,309.
        //
        // Keep the last good configuration and wait: the window sends
        // another Resized with a real size when it is restored. Marking the
        // surface unconfigured makes the redraw path skip frames until then,
        // rather than drawing into a surface that no longer matches.
        if width == 0 || height == 0 {
            self.is_surface_configured = false;

            if config.debug_window {
                println!("[WINDOW] zero-area resize ({width}x{height}), likely minimised -- skipping reconfigure");
            }

            return;
        }

        self.surface_config.width = width;
        self.surface_config.height = height;
        self.surface.configure(&self.device, &self.surface_config);

        if let Some(hud) = &self.hud {
            hud.resize_view(width as f32, height as f32, &self.queue);
        }
        self.passes
            .render
            .resize(&self.device, self.surface_config.format, width, height);
        self.passes.depth.resize(&self.device, width, height);

        let is_surface_configured = self.is_surface_configured;
        self.is_surface_configured = true;
        if !is_surface_configured && self.is_surface_configured {
            if config.debug_window {
                println!("[WINDOW] surface is now configured")
            }
        }
    }

    pub fn update(
        &mut self,
        simulation: &mut crate::app::simulation::Simulation,
        config: &crate::app::config::Config,
    ) {
        let width = self.surface_config.width;
        let height = self.surface_config.height;

        // Resolve body-tracking anchors before anything reads them, so an
        // orbiting camera follows a moving body instead of the place it was
        // when the anchor was last assigned.
        for eye in [&mut simulation.camera, &mut simulation.sun] {
            if let Some(i) = eye.anchor_body {
                if let Some(b) = simulation.bodies.get(i) {
                    eye.anchor = b.mat.transform_point3(crate::Vec3::ZERO);
                }
            }
        }

        // Fit the frustums to wherever the bodies are now, then derive the
        // shadow constants from the light's fitted frustum. Both run every
        // frame because bodies and the sun move; user-pinned values survive
        // this untouched (see Projection::resolve_with).
        let shadow_fit = if let Some(bounds) = simulation.scene_bounds() {
            simulation.camera.fit_projection(&bounds, None);
            simulation
                .sun
                .fit_projection(&bounds, Some(config.shadow_resolution));

            Some(super::frame::fit_shadow(
                &simulation.sun.projection.resolved(),
                config.shadow_resolution,
            ))
        } else {
            simulation.camera.projection.resolve_manual();
            simulation.sun.projection.resolve_manual();
            None
        };

        // Globals used to be uploaded once at startup, which silently froze
        // every shading option after start(). The automatic shadow constants
        // change as the scene moves, so it now goes up every frame -- 80
        // bytes, and it makes the other options live as a side effect.
        // Auto range fits the loaded values; a pinned min or max wins over it
        // independently, so half the scale can be fixed and the other fitted.
        let mut lo = f32::INFINITY;
        let mut hi = f32::NEG_INFINITY;
        if config.value_mode && (config.value_min.is_none() || config.value_max.is_none()) {
            for body in &simulation.bodies {
                let Some(mesh) = body.mesh.as_ref() else { continue };
                for v in &mesh.borrow().values {
                    let v = *v as f32;
                    if v.is_finite() {
                        lo = lo.min(v);
                        hi = hi.max(v);
                    }
                }
            }
        }
        if !lo.is_finite() || !hi.is_finite() {
            lo = 0.0;
            hi = 1.0;
        }
        let value_range = (
            config.value_min.unwrap_or(lo),
            config.value_max.unwrap_or(hi),
        );

        self.uniforms.globals.uniform = build_globals(config, shadow_fit, value_range);

        // Resampled to the uniform's fixed 256 entries, so any length of table
        // works -- matplotlib's 256 passes through untouched.
        if !config.colormap.is_empty() {
            let n = config.colormap.len();
            for i in 0..super::uniform::COLORMAP_SIZE {
                let t = i as f32 / (super::uniform::COLORMAP_SIZE - 1) as f32;
                // Interpolated, not nearest. Nearest turned the 8-anchor
                // built-ins into 8 visible bands -- fine for a shaded body
                // where lighting hides it, obvious on a colour scale, which is
                // a flat ramp with nothing to hide behind.
                let x = t * (n - 1) as f32;
                let lo = x.floor() as usize;
                let hi = (lo + 1).min(n - 1);
                let f = x - lo as f32;
                let (a, b) = (config.colormap[lo], config.colormap[hi]);
                self.uniforms.colormap.uniform.lut[i] = crate::Vec4::new(
                    a[0] + (b[0] - a[0]) * f,
                    a[1] + (b[1] - a[1]) * f,
                    a[2] + (b[2] - a[2]) * f,
                    1.0,
                );
            }
            self.queue.write_buffer(
                &self.uniforms.colormap.buffer,
                0,
                bytemuck::bytes_of(&self.uniforms.colormap.uniform),
            );
        }
        self.queue.write_buffer(
            &self.uniforms.globals.buffer,
            0,
            bytemuck::bytes_of(&self.uniforms.globals.uniform),
        );

        self.uniforms.view.uniform.camera.view_proj = simulation
            .camera
            .view_proj(width as Float / height as Float)
            .unwrap();

        // One shadow layer per body: aimed at it, sized to it, depth spanning
        // the scene so occluders still cast. `light.view_proj` is only the
        // scratch the shadow pass draws with, rewritten per layer at render
        // time; what the main pass samples is `view_proj_layers`.
        let n_layers = if config.shadow_per_body {
            simulation
                .bodies
                .len()
                .min(super::uniform::MAX_SHADOW_LAYERS)
        } else {
            1
        };
        self.uniforms.view.uniform.light.n_layers = n_layers as u32;

        if let Some(scene) = simulation.scene_bounds() {
            for i in 0..n_layers {
                // With per-body off, the single layer is fitted to the scene,
                // which is the pre-layer behaviour.
                let body = if config.shadow_per_body {
                    match simulation.body_bounds(i) {
                        Some(b) => b,
                        None => continue,
                    }
                } else {
                    scene
                };
                let layer = fit_light_view_proj(
                    simulation.sun.pos,
                    &body,
                    &scene,
                    simulation.sun.up_world,
                );
                self.uniforms.view.uniform.light.view_proj_layers[i] = layer.view_proj;

                // Bias from this layer's own extent, so each body gets the
                // bias its texel size actually needs. Taken from the fit
                // itself -- see `LightFit` for why it cannot be read back out
                // of `view_proj`.
                let fit = super::frame::fit_shadow(
                    &super::frame::Resolved {
                        near: layer.near,
                        far: layer.far,
                        side: layer.side,
                        offset: [0.0, 0.0],
                    },
                    config.shadow_resolution,
                );
                self.uniforms.view.uniform.light.layer_bias[i] = crate::Vec4::new(
                    config
                        .shadow_normal_offset_scale
                        .unwrap_or(fit.normal_offset_scale) as Float,
                    config.shadow_bias_scale.unwrap_or(fit.bias_scale) as Float,
                    config.shadow_bias_minimum.unwrap_or(fit.bias_minimum) as Float,
                    0.0,
                );
            }
        }

        // The Sun no longer needs aiming. It is a light source, not a camera:
        // it radiates in every direction, so a single `dir` for it was never
        // physical, and making the user call `look_anchor()` to invent one was
        // both a chore and a trap -- forgetting it left the Sun pointed at
        // whatever `anchor` happened to hold, usually the origin.
        //
        // Each layer above derives its own direction from `sun.pos` and the
        // body it is aimed at, so `sun.dir` and `sun.anchor` are ignored for
        // shadowing, and `sun.pos` alone determines the lighting. Seeding the
        // scratch from layer 0 rather than from `sun.view_proj()` also drops
        // an unwrap that panicked if a script assigned a non-normalised dir.
        self.uniforms.view.uniform.light.view_proj =
            self.uniforms.view.uniform.light.view_proj_layers[0];

        self.uniforms.view.uniform.light.pos = simulation.sun.pos;

        self.queue.write_buffer(
            &self.uniforms.view.buffer,
            0,
            bytemuck::bytes_of(&self.uniforms.view.uniform),
        );

        // Rebuilt every frame: the bounds move with the bodies, and the tick
        // step has to follow or the labels stop matching the grid. Cheap --
        // a few hundred line vertices.
        if config.axes != super::axes::AxesStyle::Off {
            let bounds = simulation
                .scene_bounds()
                .unwrap_or(crate::mesh::Aabb { min: crate::Vec3::ZERO, max: crate::Vec3::ZERO });
            let built = super::axes::build(
                config.axes,
                &bounds,
                config.axes_color,
                config.axes_ticks,
            );
            self.passes
                .axes
                .upload(&self.device, &self.queue, &built.lines);
            self.axes_labels = built.labels;
        } else {
            self.axes_labels.clear();
        }

        if config.colorbar.enabled {
            let (w, h) = (
                self.surface_config.width.max(1) as f32,
                self.surface_config.height.max(1) as f32,
            );
            let cb = &config.colorbar;
            let vertical = cb.is_vertical();
            let (bw, bh) = if vertical {
                (cb.thickness, cb.length)
            } else {
                (cb.length, cb.thickness)
            };

            // Placed in pixels against the same nine anchors the HUD uses,
            // then converted to NDC. Pixels because a bar specified as a
            // fraction of the window changes thickness when the window is
            // resized, and a scale bar should not.
            use crate::app::config::HudAnchor::*;
            let left = match cb.anchor {
                TopLeft | MiddleLeft | BottomLeft => cb.x,
                TopCenter | MiddleCenter | BottomCenter => (w - bw) * 0.5 + cb.x,
                TopRight | MiddleRight | BottomRight => w - cb.x - bw,
            };
            let top = match cb.anchor {
                TopLeft | TopCenter | TopRight => cb.y,
                MiddleLeft | MiddleCenter | MiddleRight => (h - bh) * 0.5 + cb.y,
                BottomLeft | BottomCenter | BottomRight => h - cb.y - bh,
            };

            let to_ndc_x = |px: f32| px / w * 2.0 - 1.0;
            let to_ndc_y = |px: f32| 1.0 - px / h * 2.0;
            self.uniforms.bar.uniform = super::uniform::Bar {
                // NDC y runs up, so the rectangle's origin is its bottom edge.
                rect: [
                    to_ndc_x(left),
                    to_ndc_y(top + bh),
                    bw / w * 2.0,
                    bh / h * 2.0,
                ],
                vertical: vertical as u32,
                source: match cb.source {
                    crate::app::config::ColorbarSource::Values => 0,
                    crate::app::config::ColorbarSource::Lighting => 1,
                },
                _pad: [0; 2],
            };
            self.queue.write_buffer(
                &self.uniforms.bar.buffer,
                0,
                bytemuck::bytes_of(&self.uniforms.bar.uniform),
            );
            self.colorbar_px = Some((left, top, bw, bh));
        } else {
            self.colorbar_px = None;
        }

        simulation.diagnostics = diagnose(
            simulation,
            config,
            &self.uniforms.view.uniform.camera.view_proj,
        );

        self.last_body_mats.clear();
        self.last_body_mats
            .extend(simulation.bodies.iter().map(|b| b.mat));

        // skip light cube
        for ii in 0..simulation.bodies.len() {
            let flags = if self.meshes[1 + ii].is_flat {
                super::gpu::INSTANCE_FLAG_FLAT
            } else {
                0
            };

            // Body ii shades from shadow layer ii. Bodies past the layer cap
            // share the last one: degraded, not wrong.
            let layer = if config.shadow_per_body {
                ii.min(super::uniform::MAX_SHADOW_LAYERS - 1) as u32
            } else {
                0
            };
            let instance = super::gpu::InstanceInput::new_with_layer(
                simulation.bodies[ii].mat,
                flags,
                layer,
            );
            self.meshes[1 + ii].update_instance_buffer(&self.queue, &instance);

            // The shadow stand-in has to follow the same transform, or its
            // occluder would sit somewhere the visible body is not. Its own
            // flat flag applies, since it may be flattened differently.
            if let Some(shadow_buffer) = self.shadow_meshes[1 + ii].as_mut() {
                let shadow_flags = if shadow_buffer.is_flat {
                    super::gpu::INSTANCE_FLAG_FLAT
                } else {
                    0
                };
                let shadow_instance = super::gpu::InstanceInput::new_with_flags(
                    simulation.bodies[ii].mat,
                    shadow_flags,
                );
                shadow_buffer.update_instance_buffer(&self.queue, &shadow_instance);
            }

            let mesh = simulation.bodies[ii].mesh.as_ref().unwrap();
            if mesh.borrow().colors_dirty {
                // `values` rides the same buffer and the same dirty flag:
                // both are per-vertex attributes uploaded together, so a
                // script that changes either marks colours dirty.
                let m = mesh.borrow();
                self.meshes[1 + ii].update_attrib_buffer(&self.queue, &m.vertices, &m.values);
                drop(m);
                mesh.borrow_mut().colors_dirty = false;
            }
        }

        if simulation.export_once {
            self.export_frame = true;
        } else {
            self.export_frame = simulation.export;
        }
    }

    pub fn get_surface_texture(
        &mut self,
        config: &crate::app::config::Config,
    ) -> Option<wgpu::SurfaceTexture> {
        match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture) => Some(texture),
            wgpu::CurrentSurfaceTexture::Occluded | wgpu::CurrentSurfaceTexture::Timeout => None,
            wgpu::CurrentSurfaceTexture::Suboptimal(_) | wgpu::CurrentSurfaceTexture::Outdated => {
                if config.debug_window {
                    println!(
                        "[WINDOW] surface texture is suboptimal or outdated, need to reconfigure"
                    )
                }
                self.configure_surface();
                None
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                unreachable!("No error scope registered, so validation errors will panic")
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                if config.debug_window {
                    println!("[WINDOW] surface texture has been lost, need to recreate")
                }
                self.surface = self.instance.create_surface(self.window.clone()).unwrap();
                self.configure_surface();
                None
            }
        }
    }

    /// Draw a frame. `surface_texture` is `None` when there is nowhere to
    /// present -- an occluded window, most often.
    ///
    /// Everything the simulation depends on is offscreen: the shadow map the
    /// TPM reads, and the scene itself, which is drawn into `render_texture`
    /// and only blitted to the swapchain at the end. The surface view reaches
    /// nothing but the optional depth overlay. So a frame without a surface
    /// is a complete frame minus the blit and the present.
    ///
    /// This matters more than it sounds: macOS stops handing out drawables
    /// for an occluded window, and skipping the whole frame on that basis
    /// stopped the simulation dead -- a multi-hour run behind another window
    /// made no progress at all rather than merely rendering less.
    pub fn render(
        &mut self,
        surface_texture: Option<wgpu::SurfaceTexture>,
        config: &crate::app::config::Config,
        huds: &[crate::app::config::Hud],
    ) {
        let surface_view = surface_texture
            .as_ref()
            .map(|t| t.texture.create_view(&wgpu::TextureViewDescriptor::default()));
        let offscreen_view;
        let surface_view = match &surface_view {
            Some(v) => v,
            None => {
                offscreen_view = self
                    .passes
                    .render
                    .render_texture
                    .create_view(&wgpu::TextureViewDescriptor::default());
                &offscreen_view
            }
        };

        // Shadow layers first, one submit each. A uniform write is ordered
        // against submits, not against command recording, so recording every
        // layer into one encoder would leave them all drawing with whichever
        // matrix was written last.
        let n_layers = (self.uniforms.view.uniform.light.n_layers as usize)
            .min(self.uniforms.shadow.layer_views.len());
        for i in 0..n_layers {
            self.uniforms.view.uniform.light.view_proj =
                self.uniforms.view.uniform.light.view_proj_layers[i];
            self.queue.write_buffer(
                &self.uniforms.view.buffer,
                0,
                bytemuck::bytes_of(&self.uniforms.view.uniform),
            );

            let mut enc = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            self.passes.render_shadow_layer(
                &mut enc,
                &self.uniforms.shadow.layer_views[i],
                &self.meshes,
                &self.shadow_meshes,
            );
            self.queue.submit([enc.finish()]);
        }

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

        self.passes.render(
            &mut encoder,
            &surface_view,
            &self.uniforms.shadow,
            &self.meshes,
            &self.shadow_meshes,
            config,
        );

        if let Some(texture) = &surface_texture {
            encoder.copy_texture_to_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.passes.render.render_texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyTextureInfo {
                    texture: &texture.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d {
                    width: self.surface_config.width,
                    height: self.surface_config.height,
                    depth_or_array_layers: 1,
                },
            );
        }

        self.queue.submit([encoder.finish()]);

        // Non-blocking: drains any exports whose GPU->CPU copy finished
        // since the last frame. Runs every frame (not just when exporting)
        // so in-flight exports keep progressing even after export_frame
        // turns back off.
        self.frame_exporter.poll(&self.device);

        // Burn the HUD into the frame about to be exported, when asked for.
        // This has to happen here, before the copy: the on-screen pass below
        // targets the swapchain, which the exporter never reads. Drawing it
        // twice is deliberate -- the window keeps its HUD either way, and the
        // cost is one text pass on exported frames only.
        let bar_labels: Vec<_> = match (config.colorbar.enabled, self.colorbar_px) {
            (true, Some(rect)) => colorbar_labels(
                &config.colorbar,
                rect,
                self.uniforms.globals.uniform.value_min,
                self.uniforms.globals.uniform.value_max,
            ),
            _ => Vec::new(),
        };

        let axis_labels = axis_label_screen(
            &self.axes_labels,
            &self.uniforms.view.uniform.camera.view_proj,
            &config.axes_unit,
            self.surface_config.width as f32,
            self.surface_config.height as f32,
        );

        if self.export_frame
            && config.export_hud
            && (huds.iter().any(|h| !h.text.is_empty())
                || !axis_labels.is_empty()
                || !bar_labels.is_empty())
        {
            if let Some(brush) = self.hud.as_mut() {
                let view = self
                    .passes
                    .render
                    .render_texture
                    .create_view(&wgpu::TextureViewDescriptor::default());
                let (w, h) = (
                    self.surface_config.width as f32,
                    self.surface_config.height as f32,
                );
                let mut sections: Vec<_> = huds
                    .iter()
                    .filter(|hud| !hud.text.is_empty())
                    .map(|hud| {
                        let (pos, layout) = hud_placement(hud, w, h);
                        wgpu_text::glyph_brush::Section::default()
                            .add_text(
                                wgpu_text::glyph_brush::Text::new(&hud.text)
                                    .with_scale(hud.size)
                                    .with_color(hud.color),
                            )
                            .with_screen_position(pos)
                            .with_layout(layout)
                    })
                    .collect();
                sections.extend(bar_labels.iter().map(|(text, pos, align)| {
                    wgpu_text::glyph_brush::Section::default()
                        .add_text(
                            wgpu_text::glyph_brush::Text::new(text)
                                .with_scale(config.colorbar.text_size)
                                .with_color(config.colorbar.text_color),
                        )
                        .with_screen_position(*pos)
                        .with_layout(
                            wgpu_text::glyph_brush::Layout::default_single_line()
                                .h_align(*align)
                                .v_align(wgpu_text::glyph_brush::VerticalAlign::Center),
                        )
                }));
                sections.extend(bar_labels.iter().map(|(text, pos, align)| {
                wgpu_text::glyph_brush::Section::default()
                    .add_text(
                        wgpu_text::glyph_brush::Text::new(text)
                        .with_scale(config.colorbar.text_size)
                        .with_color(config.colorbar.text_color),
                    )
                    .with_screen_position(*pos)
                    .with_layout(
                        wgpu_text::glyph_brush::Layout::default_single_line()
                        .h_align(*align)
                        .v_align(wgpu_text::glyph_brush::VerticalAlign::Center),
                    )
            }));
            sections.extend(axis_labels.iter().map(|(text, pos)| {
                    wgpu_text::glyph_brush::Section::default()
                        .add_text(
                            wgpu_text::glyph_brush::Text::new(text)
                                .with_scale(config.axes_label_size)
                                .with_color(config.axes_label_color),
                        )
                        .with_screen_position(*pos)
                        .with_layout(
                            wgpu_text::glyph_brush::Layout::default_single_line()
                                .h_align(wgpu_text::glyph_brush::HorizontalAlign::Center)
                                .v_align(wgpu_text::glyph_brush::VerticalAlign::Center),
                        )
                }));
                if brush.queue(&self.device, &self.queue, sections).is_ok() {
                    let mut encoder = self
                        .device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
                    {
                        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("hud export"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: &view,
                                depth_slice: None,
                                resolve_target: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Load,
                                    store: wgpu::StoreOp::Store,
                                },
                            })],
                            depth_stencil_attachment: None,
                            timestamp_writes: None,
                            occlusion_query_set: None,
                            multiview_mask: None,
                        });
                        brush.draw(&mut pass);
                    }
                    self.queue.submit([encoder.finish()]);
                }
            }
        }

        if self.export_frame {
            self.frame_exporter.export_frame(
                &self.device,
                &self.queue,
                &self.passes.render.render_texture,
                self.surface_config.width,
                self.surface_config.height,
            );
        }

        // The on-screen HUD: drawn after the blit and straight onto the
        // swapchain, so by itself it stays out of `render_texture` and
        // therefore out of exported frames. `Config::export_hud` adds the
        // separate pass above when it should appear in them too.
        let any_hud = huds.iter().any(|h| !h.text.is_empty());
        if let (Some(texture), Some(brush), true) =
            // Axis labels alone are reason enough to run the text pass:
            // a scene with axes but no HUD still has ticks to draw.
            (
                &surface_texture,
                self.hud.as_mut(),
                any_hud || !axis_labels.is_empty() || !bar_labels.is_empty(),
            )
        {
            let view = texture
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            let (w, h) = (
                self.surface_config.width as f32,
                self.surface_config.height as f32,
            );
            let mut sections: Vec<_> = huds
                .iter()
                .filter(|hud| !hud.text.is_empty())
                .map(|hud| {
                    let (pos, layout) = hud_placement(hud, w, h);
                    wgpu_text::glyph_brush::Section::default()
                        .add_text(
                            wgpu_text::glyph_brush::Text::new(&hud.text)
                                .with_scale(hud.size)
                                .with_color(hud.color),
                        )
                        .with_screen_position(pos)
                        .with_layout(layout)
                })
                .collect();
            sections.extend(axis_labels.iter().map(|(text, pos)| {
                wgpu_text::glyph_brush::Section::default()
                    .add_text(
                        wgpu_text::glyph_brush::Text::new(text)
                            .with_scale(config.axes_label_size)
                            .with_color(config.axes_label_color),
                    )
                    .with_screen_position(*pos)
                    .with_layout(
                        wgpu_text::glyph_brush::Layout::default_single_line()
                            .h_align(wgpu_text::glyph_brush::HorizontalAlign::Center)
                            .v_align(wgpu_text::glyph_brush::VerticalAlign::Center),
                    )
            }));
            if brush.queue(&self.device, &self.queue, sections).is_ok() {
                let mut encoder = self
                    .device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
                {
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("hud"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            depth_slice: None,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Load,
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                        multiview_mask: None,
                    });
                    brush.draw(&mut pass);
                }
                self.queue.submit([encoder.finish()]);
            }
        }

        if let Some(texture) = surface_texture {
            texture.present();
        }
    }
}

/// Builds the globals uniform from config, filling in any shadow constant the
/// user left automatic from `shadow`.
///
/// `shadow` is `None` before the first fit (or when there is no geometry to
/// fit against), in which case an automatic parameter falls back to 0.0 --
/// i.e. no bias, which shows acne rather than silently hiding a failed fit.
fn build_globals(
    config: &crate::app::config::Config,
    shadow: Option<super::frame::ShadowFit>,
    value_range: (f32, f32),
) -> super::uniform::Globals {
    super::uniform::Globals {
        value_mode: config.value_mode as u32,
        value_min: value_range.0,
        value_max: value_range.1,

        color: super::gpu::color_vec3(&config.color),
        color_mode: config.color_mode,

        srgb_mode: config.srgb_mode,
        gamma: config.gamma,

        ambient_strength: config.ambient_strength,
        light_cube_scale: config.light_cube_scale,

        shadow_resolution: config.shadow_resolution,
        shadow_bias_scale: config
            .shadow_bias_scale
            .or(shadow.map(|s| s.bias_scale))
            .unwrap_or(0.0),
        shadow_bias_minimum: config
            .shadow_bias_minimum
            .or(shadow.map(|s| s.bias_minimum))
            .unwrap_or(0.0),
        shadow_normal_offset_scale: config
            .shadow_normal_offset_scale
            .or(shadow.map(|s| s.normal_offset_scale))
            .unwrap_or(0.0),
        shadow_pcf: config.shadow_pcf,

        extra: config.extra,

        wireframe_mode: config.wireframe_mode,
        wireframe_width: config.wireframe_width,
        wireframe_color: super::gpu::color_vec3(&config.wireframe_color),

        ..Default::default()
    }
}

/// Picks a present mode honouring `vsync`, falling back to the surface's
/// preferred mode (`present_modes[0]`, always supported) when the one we'd
/// want isn't available.
///
/// Note `present_modes[0]` is typically `Fifo` -- it was the unconditional
/// choice before this was configurable, which meant a GPU fast enough to
/// beat the display refresh rate was silently capped by it.
fn pick_present_mode(caps: &wgpu::SurfaceCapabilities, vsync: bool) -> wgpu::PresentMode {
    let wanted = if vsync {
        wgpu::PresentMode::Fifo
    } else {
        wgpu::PresentMode::Immediate
    };

    if caps.present_modes.contains(&wanted) {
        wanted
    } else {
        caps.present_modes[0]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn aabb(center: Vec3, half: Float) -> crate::mesh::Aabb {
        crate::mesh::Aabb {
            min: center - Vec3::splat(half),
            max: center + Vec3::splat(half),
        }
    }

    /// The fitted half-extent must describe the body, whatever direction the
    /// light comes from.
    ///
    /// It used to be recovered afterwards as `1.0 / view_proj.x_axis.x`, which
    /// is `side / |R[0][0]|` -- correct only when the light looks down an
    /// axis. At the Hera Mars swing-by geometry `R[0][0]` fell below
    /// `f32::EPSILON` and a `1.0` fallback took over, so Mars was biased as a
    /// 1 km body instead of a 3,788 km one and the whole disc rendered with
    /// self-shadow acne no automatic setting could clear.
    #[test]
    fn light_fit_reports_its_own_extent_from_every_direction() {
        let r = 3396.0;
        let body = aabb(Vec3::new(1.0e5, -2.0e4, 3.0e4), r);
        let scene = body.union(&aabb(Vec3::new(9.0e4, -1.0e4, 2.0e4), 10.0));

        // Includes directions that put a near-zero in the view rotation, which
        // is exactly the case that used to collapse to the fallback.
        let dirs = [
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(1.0, 1.0, 1.0).normalize(),
            Vec3::new(-0.3, 0.87, 0.39).normalize(),
        ];

        for d in dirs {
            let sun = body.center() + d * 2.5e8;
            let fit = fit_light_view_proj(sun, &body, &scene, Vec3::Y);

            // A sphere of radius r inside its AABB: the box half-diagonal is
            // r*sqrt(3), and the fit pads by 5%.
            let lo = r * 1.05;
            let hi = r * 3.0f64.sqrt() as Float * 1.05 * 1.001;
            assert!(
                fit.side >= lo && fit.side <= hi,
                "side {} outside [{}, {}] for light dir {:?}",
                fit.side,
                lo,
                hi,
                d
            );
            assert!(fit.far > fit.near, "degenerate depth range for {:?}", d);

            // The whole scene has to sit inside the depth slab, or occluders
            // in front of the body stop casting into its layer.
            for c in scene.corners() {
                let z = -Mat4::look_to_rh(sun, (body.center() - sun).normalize(), Vec3::Y)
                    .transform_point3(c)
                    .z;
                assert!(
                    z >= fit.near && z <= fit.far,
                    "scene corner at depth {} outside [{}, {}] for {:?}",
                    z,
                    fit.near,
                    fit.far,
                    d
                );
            }
        }
    }
}
