//! The navigation gizmo: the axis widget in a corner of the viewport.
//!
//! Blender's, and for Blender's reason. The three arrows this replaces stood
//! at the world origin, so they were only readable when the origin was in
//! shot and not behind the body, they grew and shrank with the zoom, and they
//! could not be clicked. A widget pinned to a corner is always visible, always
//! the same size, and is a control as much as a readout: click a ball to look
//! down that axis, drag it to orbit.
//!
//! Six balls -- `+X +Y +Z` filled and lettered, `-X -Y -Z` as rings -- drawn
//! back to front so the ordering itself says which way the scene is turned,
//! with the ones pointing away dimmed. That is the whole of the 3D: the
//! widget is a flat drawing of a rotated basis, so there is no mesh, no
//! camera and no depth buffer involved.
//!
//! Everything here is pure arithmetic on the camera basis and the image size.
//! The same `Gizmo` is used to draw the widget and to hit-test a click, which
//! is what stops the picture and the click target from drifting apart.

use crate::Vec3;
use crate::app::config::HudAnchor;
use crate::app::frame::{Axis, Eye};

/// One of the six axis balls.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ball {
    pub axis: Axis,
    /// The `+` end, which is filled and lettered; the other is a ring.
    pub positive: bool,
    /// Centre in image pixels, y down.
    pub center: [f32; 2],
    /// How far the axis points toward the viewer, `-1` to `1`. Orders the
    /// balls and dims the ones at the back; nothing else uses it.
    pub facing: f32,
    pub color: [f32; 3],
}

impl Ball {
    /// The letter drawn on it, or nothing for the negative end.
    pub fn label(&self) -> Option<&'static str> {
        self.positive.then(|| match self.axis {
            Axis::X => "X",
            Axis::Y => "Y",
            Axis::Z => "Z",
        })
    }

    /// Darkened with depth, so which end of an axis is nearer reads without
    /// having to compare positions.
    ///
    /// A shade rather than an alpha, and the balls stay opaque. Dimming by
    /// transparency let the stems show through the balls that covered them,
    /// and would have let a bright body behind the widget show through as
    /// well -- a control has to stay legible over whatever it is drawn on.
    /// The floor is high enough that a letter still reads on the darkest
    /// ball.
    fn shade(&self) -> f32 {
        0.5 + 0.5 * (self.facing * 0.5 + 0.5)
    }
}

pub struct Gizmo {
    /// Widget centre in image pixels.
    pub center: [f32; 2],
    /// Half the widget's extent, so a ball centre is at most this from the
    /// middle.
    pub radius: f32,
    pub ball_radius: f32,
    /// Back to front, which is the order to draw them in.
    pub balls: Vec<Ball>,
    /// Index into `balls` of whichever one the pointer is over.
    pub hovered: Option<usize>,
}

/// A screen-space vertex of the widget.
///
/// Two triangles per shape, with the shape itself cut out in the fragment
/// shader from `uv`: a disc and a ring antialias themselves that way at any
/// size, where a triangle fan would show its facets and would need a vertex
/// count chosen against the radius.
#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    /// Normalised device coordinates.
    pub pos: [f32; 2],
    /// `-1` to `1` across the shape.
    pub uv: [f32; 2],
    pub color: [f32; 4],
    /// `1` cuts a disc out of the quad, `0` leaves a bar.
    pub disc: f32,
    /// Inner radius of a ring, `0` to `1`; `0` fills the disc.
    pub inner: f32,
}

pub const AXIS_COLORS: [[f32; 3]; 3] = [
    crate::app::axes::AXIS_X,
    crate::app::axes::AXIS_Y,
    crate::app::axes::AXIS_Z,
];

/// Where the widget sits, from the anchor and the image size.
fn anchor_center(anchor: HudAnchor, size: (f32, f32), radius: f32, margin: f32) -> [f32; 2] {
    use HudAnchor::*;
    let inset = radius + margin;
    let x = match anchor {
        TopLeft | MiddleLeft | BottomLeft => inset,
        TopCenter | MiddleCenter | BottomCenter => size.0 * 0.5,
        TopRight | MiddleRight | BottomRight => size.0 - inset,
    };
    let y = match anchor {
        TopLeft | TopCenter | TopRight => inset,
        MiddleLeft | MiddleCenter | MiddleRight => size.1 * 0.5,
        BottomLeft | BottomCenter | BottomRight => size.1 - inset,
    };
    [x, y]
}

/// Lays the widget out for the camera as it stands.
///
/// `pointer` is where the cursor is in the same image pixels, if it is over
/// the image at all; it only decides which ball reads as hovered.
pub fn build(
    camera: &Eye,
    size: (f32, f32),
    anchor: HudAnchor,
    radius: f32,
    margin: f32,
    pointer: Option<(f32, f32)>,
) -> Gizmo {
    let radius = radius.max(8.0);
    let ball_radius = radius * 0.22;
    let center = anchor_center(anchor, size, radius, margin);

    // The basis the scene is being seen through. `right` and `up` give the
    // two screen axes; the dot with `dir` gives depth, negated because the
    // camera looks *along* `dir` and a ball coming toward the viewer points
    // against it.
    let right = camera.right();
    let up = camera.up();
    let fwd = camera.dir;

    // Ball centres reach to the edge of the widget, not past it, or a corner
    // anchor would put half a ball off the image.
    let reach = radius - ball_radius;

    let mut balls = Vec::with_capacity(6);
    for (i, axis) in [Axis::X, Axis::Y, Axis::Z].into_iter().enumerate() {
        let dir = match axis {
            Axis::X => Vec3::X,
            Axis::Y => Vec3::Y,
            Axis::Z => Vec3::Z,
        };
        for positive in [true, false] {
            let d = if positive { dir } else { -dir };
            let sx = d.dot(right) as f32;
            let sy = d.dot(up) as f32;
            balls.push(Ball {
                axis,
                positive,
                // Screen y grows downward and the basis' does not.
                center: [center[0] + sx * reach, center[1] - sy * reach],
                facing: -d.dot(fwd) as f32,
                color: AXIS_COLORS[i],
            });
        }
    }

    balls.sort_by(|a, b| a.facing.total_cmp(&b.facing));

    let mut gizmo = Gizmo {
        center,
        radius,
        ball_radius,
        balls,
        hovered: None,
    };
    gizmo.hovered = pointer.and_then(|p| gizmo.index_at(p));
    gizmo
}

impl Gizmo {
    /// Which ball is under `p`, nearest first.
    ///
    /// Nearest first because the balls overlap: at most orientations two of
    /// them are stacked, and the one drawn on top is the one being pointed
    /// at. The target is a little wider than the ball is drawn, since these
    /// are small.
    pub fn index_at(&self, p: (f32, f32)) -> Option<usize> {
        let reach = self.ball_radius + 3.0;
        self.balls
            .iter()
            .enumerate()
            .rev()
            .find(|(_, b)| (b.center[0] - p.0).hypot(b.center[1] - p.1) <= reach)
            .map(|(i, _)| i)
    }

    pub fn ball_at(&self, p: (f32, f32)) -> Option<Ball> {
        self.index_at(p).map(|i| self.balls[i])
    }

    /// Whether `p` is on the widget at all, which is what starts an orbit.
    pub fn contains(&self, p: (f32, f32)) -> bool {
        (self.center[0] - p.0).hypot(self.center[1] - p.1) <= self.radius
    }

    /// The letters, ready for the text overlay: text, centre, colour.
    ///
    /// One colour for all three: they are read against their own ball, which
    /// is already shaded by depth, so shading the letter as well would take
    /// the contrast away twice over.
    pub fn labels(&self, color: [f32; 4]) -> Vec<(String, (f32, f32), [f32; 4])> {
        self.balls
            .iter()
            .filter_map(|b| {
                let text = b.label()?;
                Some((text.to_string(), (b.center[0], b.center[1]), color))
            })
            .collect()
    }

    /// The widget as screen-space triangles, back to front.
    pub fn vertices(&self, size: (f32, f32)) -> Vec<Vertex> {
        let mut out = Vec::with_capacity(6 * 6 + 3 * 6);

        // Stems first, so a ball always covers the line running into it. They
        // go to the positive ends only, as Blender's do: six spokes read as a
        // star rather than as three axes.
        let thickness = (self.radius * 0.055).max(1.5);
        for b in self.balls.iter().filter(|b| b.positive) {
            let s = b.shade();
            let c = [b.color[0] * s, b.color[1] * s, b.color[2] * s, 1.0];
            // Stopped at the ball's edge rather than run to its centre. The
            // ball is drawn after and covers the difference either way, but
            // not on the frame where the two are the same colour and the join
            // shows as a seam.
            let (dx, dy) = (b.center[0] - self.center[0], b.center[1] - self.center[1]);
            let len = dx.hypot(dy);
            let end = if len > self.ball_radius {
                let k = (len - self.ball_radius * 0.9) / len;
                [self.center[0] + dx * k, self.center[1] + dy * k]
            } else {
                b.center
            };
            bar(&mut out, size, self.center, end, thickness, c);
        }

        for (i, b) in self.balls.iter().enumerate() {
            let hovered = self.hovered == Some(i);
            // Hover lightens rather than recolours, so which axis is under
            // the pointer is still the thing the colour says.
            let s = b.shade();
            let mix = if hovered { 0.45 } else { 0.0 };
            let color = [
                b.color[0] * s + (1.0 - b.color[0] * s) * mix,
                b.color[1] * s + (1.0 - b.color[1] * s) * mix,
                b.color[2] * s + (1.0 - b.color[2] * s) * mix,
                1.0,
            ];
            // A negative ball is a ring until it is pointed at, which is the
            // cheapest way to show that it can be clicked.
            let inner = if b.positive || hovered { 0.0 } else { 0.62 };
            disc(&mut out, size, b.center, self.ball_radius, color, inner);
        }

        out
    }
}

/// Pixel position to normalised device coordinates.
fn ndc(size: (f32, f32), p: [f32; 2]) -> [f32; 2] {
    [
        2.0 * p[0] / size.0.max(1.0) - 1.0,
        1.0 - 2.0 * p[1] / size.1.max(1.0),
    ]
}

fn push_quad(
    out: &mut Vec<Vertex>,
    corners: [[f32; 2]; 4],
    color: [f32; 4],
    disc: f32,
    inner: f32,
) {
    // uv runs -1..1 across the quad in both directions, matching the corner
    // order below.
    const UV: [[f32; 2]; 4] = [[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]];
    for i in [0usize, 1, 2, 0, 2, 3] {
        out.push(Vertex {
            pos: corners[i],
            uv: UV[i],
            color,
            disc,
            inner,
        });
    }
}

/// A rectangle of `width` pixels running from `a` to `b`.
fn bar(
    out: &mut Vec<Vertex>,
    size: (f32, f32),
    a: [f32; 2],
    b: [f32; 2],
    width: f32,
    color: [f32; 4],
) {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len = dx.hypot(dy);
    if len < 1e-4 {
        return;
    }
    // Across the line, half a width each side.
    let (nx, ny) = (-dy / len * width * 0.5, dx / len * width * 0.5);
    let corners = [
        ndc(size, [a[0] - nx, a[1] - ny]),
        ndc(size, [b[0] - nx, b[1] - ny]),
        ndc(size, [b[0] + nx, b[1] + ny]),
        ndc(size, [a[0] + nx, a[1] + ny]),
    ];
    push_quad(out, corners, color, 0.0, 0.0);
}

fn disc(
    out: &mut Vec<Vertex>,
    size: (f32, f32),
    center: [f32; 2],
    radius: f32,
    color: [f32; 4],
    inner: f32,
) {
    // A pixel of slack so the antialiased edge has somewhere to fall off.
    let r = radius + 1.0;
    let corners = [
        ndc(size, [center[0] - r, center[1] - r]),
        ndc(size, [center[0] + r, center[1] - r]),
        ndc(size, [center[0] + r, center[1] + r]),
        ndc(size, [center[0] - r, center[1] + r]),
    ];
    // The quad is a pixel larger than the ball, so the unit circle in `uv`
    // has to shrink by the same amount or the ball is drawn a pixel too big.
    let scale = radius / r;
    let mut quad = Vec::with_capacity(6);
    push_quad(&mut quad, corners, color, 1.0, inner / scale.max(1e-4));
    for v in &mut quad {
        v.uv = [v.uv[0] / scale, v.uv[1] / scale];
    }
    out.extend(quad);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Vec3;

    fn eye_looking_down_neg_z() -> Eye {
        let mut eye = Eye::new();
        eye.pos = Vec3::new(0.0, 0.0, 10.0);
        eye.dir = Vec3::NEG_Z;
        eye.up = Vec3::Y;
        eye.fix_up();
        eye
    }

    #[test]
    fn the_axis_pointing_at_the_camera_is_drawn_last() {
        let g = build(
            &eye_looking_down_neg_z(),
            (800.0, 600.0),
            HudAnchor::TopLeft,
            50.0,
            16.0,
            None,
        );
        let last = g.balls.last().unwrap();
        assert_eq!(last.axis, Axis::Z);
        assert!(last.positive, "+Z faces a camera on the +Z side");
    }

    #[test]
    fn an_axis_along_the_view_lands_on_the_widget_centre() {
        let g = build(
            &eye_looking_down_neg_z(),
            (800.0, 600.0),
            HudAnchor::TopLeft,
            50.0,
            16.0,
            None,
        );
        let z = g
            .balls
            .iter()
            .find(|b| b.axis == Axis::Z && b.positive)
            .unwrap();
        assert!((z.center[0] - g.center[0]).abs() < 1e-3);
        assert!((z.center[1] - g.center[1]).abs() < 1e-3);
    }

    #[test]
    fn screen_y_is_flipped_so_up_is_up() {
        let g = build(
            &eye_looking_down_neg_z(),
            (800.0, 600.0),
            HudAnchor::TopLeft,
            50.0,
            16.0,
            None,
        );
        let y = g
            .balls
            .iter()
            .find(|b| b.axis == Axis::Y && b.positive)
            .unwrap();
        assert!(
            y.center[1] < g.center[1],
            "+Y is the camera's up, so it belongs above the centre"
        );
    }

    #[test]
    fn a_corner_anchor_keeps_every_ball_on_the_image() {
        for anchor in [
            HudAnchor::TopLeft,
            HudAnchor::TopRight,
            HudAnchor::BottomLeft,
            HudAnchor::BottomRight,
        ] {
            let g = build(
                &eye_looking_down_neg_z(),
                (800.0, 600.0),
                anchor,
                50.0,
                16.0,
                None,
            );
            for b in &g.balls {
                assert!(b.center[0] - g.ball_radius >= 0.0);
                assert!(b.center[1] - g.ball_radius >= 0.0);
                assert!(b.center[0] + g.ball_radius <= 800.0);
                assert!(b.center[1] + g.ball_radius <= 600.0);
            }
        }
    }

    #[test]
    fn the_nearer_of_two_stacked_balls_is_the_one_picked() {
        // Looking down -Z stacks +Z and -Z on the centre.
        let g = build(
            &eye_looking_down_neg_z(),
            (800.0, 600.0),
            HudAnchor::TopLeft,
            50.0,
            16.0,
            None,
        );
        let hit = g.ball_at((g.center[0], g.center[1])).unwrap();
        assert_eq!(hit.axis, Axis::Z);
        assert!(hit.positive);
    }

    #[test]
    fn a_click_off_the_widget_hits_nothing() {
        let g = build(
            &eye_looking_down_neg_z(),
            (800.0, 600.0),
            HudAnchor::TopLeft,
            50.0,
            16.0,
            None,
        );
        assert!(g.ball_at((400.0, 300.0)).is_none());
        assert!(!g.contains((400.0, 300.0)));
        assert!(g.contains((g.center[0], g.center[1])));
    }
}
