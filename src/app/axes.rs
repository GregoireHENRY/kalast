//! Reference axes, in the styles the plotting tools people already use draw.
//!
//! A rendered body on its own says nothing about scale or orientation. These
//! put a frame around it: how big it is, which way is up, where the origin
//! sits. Four styles, because the right one depends on what the figure is
//! for -- a measurable box for a profile, a corner gizmo for a fly-through
//! where a full box would be noise.
//!
//! All of it is lines and text. There is no line width in WebGPU, so every
//! line is one pixel; a thicker axis would have to be built from triangles.

use crate::{Float, Vec3};

/// Which frame to draw around the scene.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AxesStyle {
    /// Nothing.
    Off,
    /// A closed box around the scene, ticked on the near edges. What MATLAB's
    /// `box on` draws, and what a profile figure wants: every edge is a ruler.
    Box,
    /// The three far panes, gridded, with ticks on their outer edges --
    /// matplotlib's `Axes3D`. Reads as a room the body sits in, so the grid
    /// gives depth cues a bare box does not.
    Panes,
    /// Three labelled arrows at the origin and nothing else. For fly-throughs
    /// and movies, where a box would occlude the subject every time the camera
    /// swings.
    Gizmo,
    /// Blender's viewport: a ground grid on the XY plane, the Z axis picked
    /// out as a vertical line, plus the corner gizmo. Best for judging
    /// orientation while moving around a scene.
    Blender,
}

impl AxesStyle {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().replace(['_', ' '], "-").as_str() {
            "off" | "none" => Some(Self::Off),
            "box" | "matlab" => Some(Self::Box),
            "panes" | "matplotlib" | "mpl" => Some(Self::Panes),
            "gizmo" | "corner" | "xyz" => Some(Self::Gizmo),
            "blender" | "grid" => Some(Self::Blender),
            _ => None,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Box => "box",
            Self::Panes => "panes",
            Self::Gizmo => "gizmo",
            Self::Blender => "blender",
        }
    }
}

/// One line segment endpoint.
#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LineVertex {
    pub pos: [f32; 3],
    pub color: [f32; 3],
}

/// A tick value to be drawn at a world position, formatted by the caller so
/// the unit string is applied once rather than per label.
pub struct Label {
    pub world: Vec3,
    pub text: String,
    /// Whether `Config::axes_unit` applies. True for tick values, false for
    /// the gizmo's axis names -- "X m" is nonsense.
    pub unit: bool,
}

pub struct Axes {
    pub lines: Vec<LineVertex>,
    pub labels: Vec<Label>,
}

/// Tick step: 1, 2 or 5 times a power of ten, whichever gives closest to
/// `target` intervals across `span`.
///
/// The alternative -- `span / target` unrounded -- gives ticks at 0.0347 and
/// nobody reads a figure that way.
fn nice_step(span: Float, target: usize) -> Float {
    if span <= 0.0 || target == 0 {
        return 1.0;
    }
    let raw = span / target as Float;
    let mag = (10.0 as Float).powf(raw.log10().floor());
    let norm = raw / mag;
    let step = if norm < 1.5 {
        1.0
    } else if norm < 3.0 {
        2.0
    } else if norm < 7.0 {
        5.0
    } else {
        10.0
    };
    step * mag
}

/// Tick positions covering `lo..hi` on a nice step, inclusive of any that
/// land exactly on the ends.
fn ticks(lo: Float, hi: Float, target: usize) -> Vec<Float> {
    let step = nice_step(hi - lo, target);
    if !step.is_finite() || step <= 0.0 {
        return vec![];
    }
    let first = (lo / step).ceil() * step;
    let mut out = Vec::new();
    let mut t = first;
    // Guard rather than trust the float loop: a pathological span would
    // otherwise spin here.
    while t <= hi + step * 1e-6 && out.len() < 64 {
        out.push(t);
        t += step;
    }
    out
}

/// Enough decimals to tell adjacent ticks apart, and no more.
fn format_tick(v: Float, step: Float) -> String {
    let decimals = if step >= 1.0 {
        0
    } else {
        (-step.log10().floor()) as usize
    };
    let s = format!("{v:.decimals$}", decimals = decimals.min(6));
    // "-0" reads as an error in a figure.
    if s.trim_start_matches('-').chars().all(|c| c == '0' || c == '.') {
        s.trim_start_matches('-').to_string()
    } else {
        s
    }
}

fn seg(out: &mut Vec<LineVertex>, a: Vec3, b: Vec3, c: [f32; 3]) {
    out.push(LineVertex {
        pos: [a.x as f32, a.y as f32, a.z as f32],
        color: c,
    });
    out.push(LineVertex {
        pos: [b.x as f32, b.y as f32, b.z as f32],
        color: c,
    });
}

const AXIS_X: [f32; 3] = [0.90, 0.25, 0.25];
const AXIS_Y: [f32; 3] = [0.35, 0.80, 0.35];
const AXIS_Z: [f32; 3] = [0.35, 0.50, 0.95];

/// Builds the geometry for `style` around `bounds`.
///
/// `tick_target` is a wish, not a count: the step is rounded to a readable
/// number first, so the result is usually within one or two of it.
pub fn build(
    style: AxesStyle,
    bounds: &crate::mesh::Aabb,
    grid: [f32; 3],
    tick_target: usize,
) -> Axes {
    let mut lines = Vec::new();
    let mut labels = Vec::new();

    if style == AxesStyle::Off || bounds.is_empty() {
        return Axes { lines, labels };
    }

    let (lo, hi) = (bounds.min, bounds.max);
    let tx = ticks(lo.x, hi.x, tick_target);
    let ty = ticks(lo.y, hi.y, tick_target);
    let tz = ticks(lo.z, hi.z, tick_target);
    let (sx, sy, sz) = (
        nice_step(hi.x - lo.x, tick_target),
        nice_step(hi.y - lo.y, tick_target),
        nice_step(hi.z - lo.z, tick_target),
    );
    // Labels are pushed outward from the box centre rather than to a fixed
    // corner: at a fixed corner they land on top of the body from half the
    // viewing angles, which is what the first attempt did.
    let pad = bounds.radius() * 0.10;
    let c = bounds.center();
    let out = |p: Vec3| -> Vec3 {
        let d = p - c;
        // Only the axes this label is *not* measuring get pushed, so a tick
        // stays at its own value along its own axis.
        p + Vec3::new(
            if d.x.abs() > 1e-12 { d.x.signum() * pad } else { 0.0 },
            if d.y.abs() > 1e-12 { d.y.signum() * pad } else { 0.0 },
            if d.z.abs() > 1e-12 { d.z.signum() * pad } else { 0.0 },
        )
    };

    match style {
        AxesStyle::Off => {}

        AxesStyle::Box => {
            // Twelve edges.
            for &(a, b) in &[
                ((lo.x, lo.y, lo.z), (hi.x, lo.y, lo.z)),
                ((lo.x, hi.y, lo.z), (hi.x, hi.y, lo.z)),
                ((lo.x, lo.y, hi.z), (hi.x, lo.y, hi.z)),
                ((lo.x, hi.y, hi.z), (hi.x, hi.y, hi.z)),
                ((lo.x, lo.y, lo.z), (lo.x, hi.y, lo.z)),
                ((hi.x, lo.y, lo.z), (hi.x, hi.y, lo.z)),
                ((lo.x, lo.y, hi.z), (lo.x, hi.y, hi.z)),
                ((hi.x, lo.y, hi.z), (hi.x, hi.y, hi.z)),
                ((lo.x, lo.y, lo.z), (lo.x, lo.y, hi.z)),
                ((hi.x, lo.y, lo.z), (hi.x, lo.y, hi.z)),
                ((lo.x, hi.y, lo.z), (lo.x, hi.y, hi.z)),
                ((hi.x, hi.y, lo.z), (hi.x, hi.y, hi.z)),
            ] {
                seg(
                    &mut lines,
                    Vec3::new(a.0, a.1, a.2),
                    Vec3::new(b.0, b.1, b.2),
                    grid,
                );
            }
            for &v in &tx {
                labels.push(Label {
                    world: out(Vec3::new(v, lo.y, lo.z)),
                    text: format_tick(v, sx),
                    unit: true,
                });
            }
            for &v in &ty {
                labels.push(Label {
                    world: out(Vec3::new(lo.x, v, lo.z)),
                    text: format_tick(v, sy),
                    unit: true,
                });
            }
            for &v in &tz {
                labels.push(Label {
                    world: out(Vec3::new(lo.x, lo.y, v)),
                    text: format_tick(v, sz),
                    unit: true,
                });
            }
        }

        AxesStyle::Panes => {
            // Three far panes, gridded on the tick lines so the grid and the
            // labels agree.
            for &v in &tx {
                seg(
                    &mut lines,
                    Vec3::new(v, lo.y, lo.z),
                    Vec3::new(v, hi.y, lo.z),
                    grid,
                );
                seg(
                    &mut lines,
                    Vec3::new(v, lo.y, lo.z),
                    Vec3::new(v, lo.y, hi.z),
                    grid,
                );
                labels.push(Label {
                    world: out(Vec3::new(v, lo.y, lo.z)),
                    text: format_tick(v, sx),
                    unit: true,
                });
            }
            for &v in &ty {
                seg(
                    &mut lines,
                    Vec3::new(lo.x, v, lo.z),
                    Vec3::new(hi.x, v, lo.z),
                    grid,
                );
                seg(
                    &mut lines,
                    Vec3::new(lo.x, v, lo.z),
                    Vec3::new(lo.x, v, hi.z),
                    grid,
                );
                labels.push(Label {
                    world: out(Vec3::new(lo.x, v, lo.z)),
                    text: format_tick(v, sy),
                    unit: true,
                });
            }
            for &v in &tz {
                seg(
                    &mut lines,
                    Vec3::new(lo.x, lo.y, v),
                    Vec3::new(hi.x, lo.y, v),
                    grid,
                );
                seg(
                    &mut lines,
                    Vec3::new(lo.x, lo.y, v),
                    Vec3::new(lo.x, hi.y, v),
                    grid,
                );
                labels.push(Label {
                    world: out(Vec3::new(lo.x, lo.y, v)),
                    text: format_tick(v, sz),
                    unit: true,
                });
            }
        }

        AxesStyle::Gizmo | AxesStyle::Blender => {
            if style == AxesStyle::Blender {
                // Ground grid on XY, at the tick spacing so it means
                // something rather than being decoration.
                let dim = [grid[0] * 0.45, grid[1] * 0.45, grid[2] * 0.45];
                for &v in &tx {
                    seg(
                        &mut lines,
                        Vec3::new(v, lo.y, 0.0),
                        Vec3::new(v, hi.y, 0.0),
                        dim,
                    );
                }
                for &v in &ty {
                    seg(
                        &mut lines,
                        Vec3::new(lo.x, v, 0.0),
                        Vec3::new(hi.x, v, 0.0),
                        dim,
                    );
                }
                // Z picked out, since a flat grid alone gives no sense of up.
                seg(
                    &mut lines,
                    Vec3::new(0.0, 0.0, lo.z),
                    Vec3::new(0.0, 0.0, hi.z),
                    AXIS_Z,
                );
            }

            // The gizmo itself: three arrows from the origin, long enough to
            // read against the body without swamping it.
            let len = bounds.radius() * 0.35;
            let o = Vec3::ZERO;
            for (dir, color, name) in [
                (Vec3::X, AXIS_X, "X"),
                (Vec3::Y, AXIS_Y, "Y"),
                (Vec3::Z, AXIS_Z, "Z"),
            ] {
                let tip = o + dir * len;
                seg(&mut lines, o, tip, color);
                // Two barbs, in the plane that faces the reader most of the
                // time. A cone would need triangles.
                let side = if dir == Vec3::Z { Vec3::X } else { Vec3::Z };
                let barb = len * 0.15;
                seg(&mut lines, tip, tip - dir * barb + side * barb * 0.5, color);
                seg(&mut lines, tip, tip - dir * barb - side * barb * 0.5, color);
                labels.push(Label {
                    world: tip + dir * (len * 0.12),
                    text: name.to_string(),
                    unit: false,
                });
            }
        }
    }

    Axes { lines, labels }
}
