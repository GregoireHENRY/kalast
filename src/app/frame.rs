use crate::{Float, Mat3, Mat4, Vec3};

pub const SENSITIVITY_MOVE: Float = 0.5;
pub const SENSITIVITY_LOOK: Float = 0.1;
pub const SENSITIVITY_ROTATE: Float = 0.1;
pub const SENSITIVITY_ZOOM: Float = 1.0e1;

// Arcball constants are per *event*, not per second, so they are deliberately
// not scaled by dt like the WASD ones. A mouse delta is a displacement the
// user already made; multiplying it by frame time makes the same drag turn
// the camera further on a slow machine than on a fast one.
//
// Radians per pixel of drag: ~200 px for a right angle.
pub const SENSITIVITY_ORBIT: Float = 0.008;
// Fraction of the anchor distance per wheel notch, applied geometrically.
pub const SENSITIVITY_ZOOM_WHEEL: Float = 0.12;
// Fraction of the anchor distance per pixel of pan.
pub const SENSITIVITY_PAN: Float = 0.0015;

/// The plane distances actually handed to the projection matrix.
///
/// Kept separate from the user-facing `Option` fields so `mat()` -- which is
/// also exposed to Python -- always has concrete numbers to work with,
/// whether they came from the user or from the automatic fit.
#[derive(Debug, Clone, Copy)]
pub struct Resolved {
    pub near: Float,
    pub far: Float,
    pub side: Float,

    /// Where the orthographic box sits, in view-space x/y, relative to the
    /// view axis. Zero for a scene centred on what the frame looks at;
    /// non-zero when it is not, which is the normal case for a binary --
    /// see the note in `fit_projection`. Unused by the perspective path.
    pub offset: [Float; 2],
}

#[derive(Debug, Clone)]
pub struct Projection {
    pub mode: ProjectionMode,
    pub fovy: Float, // radian

    // None means "fit to the scene automatically" (the default). Setting one
    // of these pins that single plane and leaves the others automatic, so
    // manual control stays available for debugging without forcing the user
    // to supply all of them.
    pub near: Option<Float>,
    pub far: Option<Float>,
    pub side: Option<Float>,

    resolved: Resolved,
}

impl Projection {
    pub fn new() -> Self {
        Self {
            mode: ProjectionMode::Perspective,
            fovy: 0.5236, // ~45 degrees
            near: None,
            far: None,
            side: None,
            // Only used until the first fit runs, or if there is no geometry
            // to fit against.
            resolved: Resolved {
                near: 0.01,
                far: 100.0,
                side: 5.0,
                offset: [0.0, 0.0],
            },
        }
    }

    pub fn resolved(&self) -> Resolved {
        self.resolved
    }

    /// Applies a fitted result, letting any user-set value win over it.
    pub fn resolve_with(&mut self, fit: Resolved) {
        self.resolved = Resolved {
            near: self.near.unwrap_or(fit.near),
            far: self.far.unwrap_or(fit.far),
            side: self.side.unwrap_or(fit.side),
            // Not user-overridable: it is a consequence of where the geometry
            // is, not a preference. A pinned `side` still keeps its offset.
            offset: fit.offset,
        };
    }

    /// Applies user values only, for when there is no geometry to fit
    /// against -- keeps whatever was last resolved for the rest.
    pub fn resolve_manual(&mut self) {
        let current = self.resolved;
        self.resolve_with(current);
    }

    // right-handed, Z axis points out of the screen
    // aspect: window width / height
    pub fn mat(&self, aspect: Float) -> Mat4 {
        let Resolved {
            near,
            far,
            side,
            offset,
        } = self.resolved;

        match self.mode {
            ProjectionMode::Orthographic => {
                let half_height = side;
                let half_width = half_height * aspect;

                // Off-centre: the box is `side` wide but sits where the
                // geometry is, which need not be on the view axis.
                Mat4::orthographic_rh(
                    offset[0] - half_width,
                    offset[0] + half_width,
                    offset[1] - half_height,
                    offset[1] + half_height,
                    near,
                    far,
                )
            }
            ProjectionMode::Perspective => Mat4::perspective_rh(self.fovy, aspect, near, far),
        }
    }
}

/// A world axis, for the plane views.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
    Z,
}

impl Axis {
    /// Parsed from Python, where these are plain strings. Accepts the axis
    /// looked along ("z") or the plane looked at ("xy"), since both get typed
    /// and they mean the same view.
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "x" | "yz" | "zy" => Some(Self::X),
            "y" | "zx" | "xz" => Some(Self::Y),
            "z" | "xy" | "yx" => Some(Self::Z),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProjectionMode {
    Orthographic,

    Perspective,
}

/// Shadow depth-comparison constants derived from the fitted light frustum.
#[derive(Debug, Clone, Copy)]
pub struct ShadowFit {
    pub normal_offset_scale: f32,
    pub bias_scale: f32,
    pub bias_minimum: f32,
}

/// Derives shadow bias and normal offset from the light's fitted orthographic
/// frustum and the shadow map size.
///
/// Everything is expressed relative to one shadow texel, which is what makes
/// these scale-free: the same scene at metres or at kilometres, or at a
/// different `shadow_resolution`, gets consistent results without retuning.
/// Hand-picked constants cannot do that -- values tuned for a 780 m body seen
/// from 25 km are meaningless for anything else.
pub fn fit_shadow(light: &Resolved, shadow_resolution: u32) -> ShadowFit {
    let resolution = shadow_resolution.max(1) as Float;

    // One shadow texel, in world units.
    let world_per_texel = 2.0 * light.side / resolution;

    // The depth the shadow map stores is normalised over near..far, so a
    // world-space offset has to be divided by the depth range to become a
    // comparable bias.
    let depth_range = (light.far - light.near).abs().max(Float::EPSILON);
    let texel_depth = world_per_texel / depth_range;

    ShadowFit {
        // Push the sample about one texel diagonal along the normal, which is
        // the worst-case in-texel distance a surface can span.
        normal_offset_scale: (world_per_texel * std::f64::consts::SQRT_2 as Float) as f32,
        // Slope term: at grazing angles a texel covers far more depth, and
        // this is multiplied by (1 - N.L)^2 in the shader.
        bias_scale: (texel_depth * 10.0) as f32,
        // Floor for surfaces facing the light head-on.
        bias_minimum: texel_depth as f32,
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Control {
    Arcball,
    WASD,
    None,
}

impl Control {
    pub fn toggle(&mut self) {
        *self = match self {
            Self::Arcball | Self::None => Self::WASD,
            Self::WASD => Self::Arcball,
        };
    }
}

// if unit vectors are not normalized, results are gonna be wrong
#[derive(Debug, Clone)]
pub struct Eye {
    pub pos: Vec3,
    pub dir: Vec3, // unit vector
    pub up: Vec3,  // unit vector
    pub anchor: Vec3,

    /// Track a body instead of a fixed point: when set, `anchor` is refreshed
    /// from that body's transform every frame.
    ///
    /// Assigning `anchor` from a body's matrix is a snapshot, and goes stale
    /// the moment the body moves -- correct for one static frame, silently
    /// wrong in any animation, and the caller has to remember to redo it every
    /// step. `None` keeps `anchor` exactly as set.
    pub anchor_body: Option<usize>,
    pub up_world: Vec3, // unit vector
    pub projection: Projection,
    pub control: Control,
}

impl Eye {
    pub fn new() -> Self {
        Self {
            pos: Vec3::new(0.0, 0.0, 0.0),
            dir: Vec3::new(1.0, 0.0, 0.0),
            up: Vec3::new(0.0, 0.0, 1.0),
            anchor: Vec3::new(0.0, 0.0, 0.0),
            anchor_body: None,
            up_world: Vec3::new(0.0, 0.0, 1.0),
            projection: Projection::new(),
            control: Control::Arcball,
        }
    }

    pub fn target(&self) -> Vec3 {
        self.pos + self.dir
    }

    pub fn up(&self) -> Vec3 {
        self.up
    }

    pub fn bottom(&self) -> Vec3 {
        -self.up
    }

    pub fn front(&self) -> Vec3 {
        self.dir
    }

    pub fn back(&self) -> Vec3 {
        -self.dir
    }

    /// Unit vector to the camera's right.
    ///
    /// Normalised because this is used as a rotation axis: `dir x up` is only
    /// unit-length when `up` is exactly perpendicular to `dir`, and a script
    /// is free to assign one that is not. Feeding a short axis to
    /// `Mat3::from_axis_angle` yields a matrix that scales as well as
    /// rotates, so orbiting would shrink the anchor distance a little on
    /// every event until the camera collapsed onto the anchor and appeared
    /// stuck.
    pub fn right(&self) -> Vec3 {
        let right = self.dir.cross(self.up);
        if right.length_squared() > 1e-12 {
            return right.normalize();
        }

        // dir and up are parallel: any axis not parallel to dir will do.
        for candidate in [self.up_world, Vec3::Z, Vec3::Y, Vec3::X] {
            let right = self.dir.cross(candidate);
            if right.length_squared() > 1e-12 {
                return right.normalize();
            }
        }
        Vec3::X
    }

    pub fn left(&self) -> Vec3 {
        -self.right()
    }

    pub fn distance_anchor(&self) -> Float {
        (self.anchor - self.pos).length()
    }

    pub fn lookto(&self) -> anyhow::Result<Mat4> {
        if !self.dir.is_normalized() {
            return Err(anyhow::anyhow!("Camera dir {} is not normalized", self.dir));
        }
        if !self.up.is_normalized() {
            return Err(anyhow::anyhow!("Camera up {} is not normalized", self.up));
        }

        Ok(Mat4::look_to_rh(self.pos, self.dir, self.up))
    }

    pub fn view_proj(&self, aspect: Float) -> anyhow::Result<Mat4> {
        Ok(self.projection.mat(aspect) * self.lookto()?)
    }

    pub fn mat(&self) -> Mat3 {
        Mat3::from_cols(self.dir, self.right(), self.up)
    }

    /// Rebuilds `up` perpendicular to `dir`, preserving roll where possible.
    ///
    /// Falls back through `up_world` and the world axes when the current `up`
    /// is parallel to `dir`: there `dir x up` is the zero vector, and
    /// normalising it yields NaN, which then propagates into `pos` and `dir`
    /// and leaves the camera permanently stuck. A script is free to assign
    /// any `up` it likes -- including one parallel to `dir` -- so this cannot
    /// assume the caller supplied a usable basis.
    pub fn fix_up(&mut self) {
        let up = self.right().cross(self.dir);
        if up.is_finite() && up.length_squared() > 1e-12 {
            self.up = up.normalize();
        }
    }

    /// Forces `pos`/`dir`/`up` into a usable state before the controller acts
    /// on them, so that whatever a script assigned -- an unnormalised `dir`,
    /// an `up` parallel to it, a NaN left over from an earlier frame -- the
    /// camera still responds instead of silently locking up.
    pub fn sanitize_basis(&mut self) {
        if !self.pos.is_finite() {
            self.pos = Vec3::ZERO;
        }
        if !self.anchor.is_finite() {
            self.anchor = Vec3::ZERO;
        }
        if !self.up_world.is_finite() || self.up_world.length_squared() < 1e-12 {
            self.up_world = Vec3::Z;
        }

        if !self.dir.is_finite() || self.dir.length_squared() < 1e-12 {
            // Prefer facing the anchor; fall back to a fixed axis when the
            // eye sits exactly on it.
            let to_anchor = self.anchor - self.pos;
            self.dir = if to_anchor.length_squared() > 1e-12 {
                to_anchor.normalize()
            } else {
                Vec3::X
            };
        } else {
            self.dir = self.dir.normalize();
        }

        // Unconditionally, not just when degenerate: an `up` that is merely
        // off-perpendicular is enough to make `right()` non-unit, which is
        // what breaks orbiting.
        self.fix_up();
    }

    pub fn look_anchor(&mut self) {
        self.dir = (self.anchor - self.pos).normalize();
        self.fix_up();
    }

    pub fn set_target(&mut self, target: Vec3) {
        self.anchor = target;
        self.dir = (self.anchor - self.pos).normalize();
        self.fix_up();
    }

    /// Looks straight down one axis at `bounds`, the way a plot does.
    ///
    /// `axis` is the axis looked *along*, so `Z` gives the XY plane face-on.
    /// The eye is placed on the positive side and backed off far enough for
    /// the frustum fit to have something to work with; the fit then sizes the
    /// projection, so the distance itself does not set the framing.
    ///
    /// `orthographic` is the reason this exists. A crater profile read off a
    /// perspective view is not measurable -- near rim and far rim are at
    /// different scales -- which is why every published figure of this kind is
    /// orthographic. It is not forced, because a perspective axis view is
    /// still useful for looking around.
    pub fn view_along(&mut self, axis: Axis, bounds: &crate::mesh::Aabb, orthographic: bool) {
        let centre = bounds.center();
        // Any offset works; the projection is fitted afterwards. Tie it to the
        // scene so it is never inside the geometry.
        let back = (bounds.radius() * 4.0).max(Float::EPSILON);

        let (eye_dir, up) = match axis {
            // Up is chosen so the remaining two axes read left-to-right and
            // bottom-to-top, matching how the same plane is drawn in a plot.
            Axis::X => (Vec3::NEG_X, Vec3::Z),
            Axis::Y => (Vec3::NEG_Y, Vec3::Z),
            Axis::Z => (Vec3::NEG_Z, Vec3::Y),
        };

        self.pos = centre - eye_dir * back;
        self.dir = eye_dir;
        self.up = up;
        self.anchor = centre;
        // A plane view is about the scene, not about whatever body the anchor
        // was following, so stop tracking rather than have it snap back.
        self.anchor_body = None;
        self.fix_up();

        self.projection.mode = if orthographic {
            ProjectionMode::Orthographic
        } else {
            ProjectionMode::Perspective
        };
    }

    /// Fits this eye's frustum planes around `bounds`, leaving any plane the
    /// user pinned untouched.
    ///
    /// Both modes work in eye space, where the eye looks down -Z, so a point's
    /// distance in front of the eye is `-z`.
    ///
    /// `shadow_texels`, when given, snaps an orthographic fit to whole
    /// shadow-map texels. Without that the fitted box slides continuously as
    /// the light moves and the shadow edge crawls between texels from frame to
    /// frame -- the stabilisation step from the standard shadow-map recipe.
    pub fn fit_projection(&mut self, bounds: &crate::mesh::Aabb, shadow_texels: Option<u32>) {
        if bounds.is_empty() {
            self.projection.resolve_manual();
            return;
        }

        let Ok(view) = self.lookto() else {
            self.projection.resolve_manual();
            return;
        };

        let mut min_d = Float::INFINITY;
        let mut max_d = Float::NEG_INFINITY;
        let mut half_w: Float = 0.0;
        let mut half_h: Float = 0.0;

        for corner in bounds.corners() {
            let p = view.transform_point3(corner);
            let d = -p.z;

            min_d = min_d.min(d);
            max_d = max_d.max(d);
            half_w = half_w.max(p.x.abs());
            half_h = half_h.max(p.y.abs());
        }

        // A little slack so geometry never lands exactly on a clip plane.
        let margin = 1.05;

        let fit = match self.projection.mode {
            ProjectionMode::Orthographic => {
                // Size from the bounding sphere, not the projected extent:
                // the sphere radius does not change as the light rotates, so
                // the shadow map keeps a constant world-per-texel scale
                // instead of breathing every frame.
                //
                // The sphere is centred on the *bounds*, though, while the
                // view axis passes through whatever the frame is aimed at.
                // For a binary those differ: with the light aimed at the
                // primary, Dimorphos sits up to 1.15 km off-axis and its far
                // edge reached 1.246 km against a half-width of 1.056 km, so
                // part of it fell outside the shadow map. Geometry outside
                // the frustum is clipped and never writes depth, and samples
                // outside read as shadowed -- which both drew a hard false
                // terminator across the secondary and, because
                // `facet_shadow` reads this same map, fed a wrong lit
                // fraction to the thermophysical model. It affected 28% of
                // epochs over one Dimorphos orbit.
                //
                // Fixed by offsetting the box rather than enlarging it.
                // Growing `side` to `offset + radius` also covers the scene
                // but more than doubles the world-per-texel at quadrature,
                // coarsening the shadow for *both* bodies -- measured as up
                // to 8 K on Didymos, which was never clipped in the first
                // place. An off-centre box keeps the resolution.
                let centre = view.transform_point3(bounds.center());

                let mut side = bounds.radius() * margin;
                let mut offset = [centre.x, centre.y];

                if let Some(texels) = shadow_texels.filter(|t| *t > 0) {
                    // Quantise the size too -- snapping the offset alone is
                    // pointless if the texel size itself keeps changing.
                    let quantum = 2.0 * side / texels as Float;
                    side = (side / quantum).ceil() * quantum;
                    // Snap the offset to whole texels, so the shadow map does
                    // not shimmer as the box slides with the geometry.
                    let texel = 2.0 * side / texels as Float;
                    offset = [
                        (offset[0] / texel).round() * texel,
                        (offset[1] / texel).round() * texel,
                    ];
                }

                // Keep the whole scene in depth even when the light sits
                // inside the bounds; near may go negative for an ortho
                // projection, which is fine and avoids clipping casters
                // behind the light's nominal position.
                Resolved {
                    near: min_d - bounds.radius() * margin,
                    far: max_d + bounds.radius() * margin,
                    side,
                    offset,
                }
            }
            ProjectionMode::Perspective => {
                // Near must stay positive and not absurdly small, or depth
                // precision collapses. Tie its floor to the scene scale
                // rather than a fixed epsilon so this works at any units.
                let far = (max_d * margin).max(Float::EPSILON);
                let near = (min_d / margin).max(far * 1e-5);

                Resolved {
                    near,
                    far,
                    side: half_w.max(half_h) * margin,
                    offset: [0.0, 0.0],
                }
            }
        };

        self.projection.resolve_with(fit);
    }

    /// Orbit, pan and zoom from accumulated pointer input.
    ///
    /// Deliberately takes no `dt`: every term here is driven by a pointer
    /// displacement the user has already made, so scaling by frame time would
    /// make the same physical gesture do different amounts of work depending
    /// on the frame rate.
    pub fn arcball_update(&mut self, ctrl: &mut Controller) {
        // A script may have assigned pos/dir/up this frame (the Hera examples
        // do, from spice). Repair the basis before orbiting rather than
        // trusting it.
        self.sanitize_basis();

        // Geometric zoom: each notch scales the distance by a constant
        // factor, so zooming never overshoots through the anchor the way a
        // linear step does, and feels the same at any scale.
        if ctrl.zoom != 0.0 {
            let factor =
                (-ctrl.zoom * ctrl.sensitivity_zoom * SENSITIVITY_ZOOM_WHEEL).exp() as Float;
            let distance = (self.distance_anchor() * factor).max(Float::EPSILON);
            self.pos = self.anchor - self.dir * distance;
        }

        // Pan moves the anchor with the eye, so the thing being orbited stays
        // the thing under the cursor.
        if ctrl.pan_horizontal != 0.0 || ctrl.pan_vertical != 0.0 {
            let scale = ctrl.sensitivity_move * SENSITIVITY_PAN * self.distance_anchor();
            let offset = self.right() * (-ctrl.pan_horizontal * scale)
                + self.up * (ctrl.pan_vertical * scale);

            self.pos += offset;
            self.anchor += offset;
        }

        // Orbiting rotates `pos` about `anchor`; with the eye sitting on the
        // anchor there is no radius to rotate and the camera would appear
        // frozen, so leave it alone rather than pretending to move.
        if (ctrl.horizontal != 0.0 || ctrl.vertical != 0.0)
            && self.distance_anchor() > 1e-9
        {
            let m1 = Mat3::from_axis_angle(
                self.up_world,
                -ctrl.horizontal * ctrl.sensitivity_rotate * SENSITIVITY_ORBIT,
            );
            let m2 = Mat3::from_axis_angle(
                self.right(),
                -ctrl.vertical * ctrl.sensitivity_rotate * SENSITIVITY_ORBIT,
            );
            let m = m1 * m2;

            // update movement and cam dir
            self.pos = self.anchor + m * (self.pos - self.anchor);
            self.up = m * self.up;
            self.look_anchor();
        }
    }

    pub fn wasd_with_conroller(&mut self, ctrl: &mut Controller, dt: Float) {
        // movement
        self.pos += (self.dir * (ctrl.forward - ctrl.backward)
            + self.right() * (ctrl.right - ctrl.left)
            + self.up * (ctrl.up - ctrl.down))
            * ctrl.sensitivity_move
            * SENSITIVITY_MOVE
            * dt
            * self.distance_anchor();

        // look around
        let m1 = Mat3::from_axis_angle(
            self.up,
            -ctrl.horizontal * ctrl.sensitivity_look * SENSITIVITY_LOOK * dt,
        );
        let m2 = Mat3::from_axis_angle(
            self.right(),
            -ctrl.vertical * ctrl.sensitivity_look * SENSITIVITY_LOOK * dt,
        );
        let m = m1 * m2;
        self.up = m * self.up;
        self.dir = m * self.dir;
    }

    pub fn update_with_controller(&mut self, ctrl: &mut Controller, dt: Float) {
        match self.control {
            Control::Arcball => self.arcball_update(ctrl),
            Control::WASD => self.wasd_with_conroller(ctrl, dt),
            Control::None => {}
        };

        // reset mouse amounts
        ctrl.horizontal = 0.0;
        ctrl.vertical = 0.0;
        ctrl.pan_horizontal = 0.0;
        ctrl.pan_vertical = 0.0;
        ctrl.zoom = 0.0;
    }
}

#[derive(Debug)]
pub struct Controller {
    pub left: Float,
    pub right: Float,
    pub forward: Float,
    pub backward: Float,
    pub up: Float,
    pub down: Float,
    pub horizontal: Float, // radian
    pub vertical: Float,   // radian
    pub pan_horizontal: Float,
    pub pan_vertical: Float,
    pub zoom: Float,

    // Pointer state, so a drag can be distinguished from a bare move.
    // Orbiting on button-drag is what makes the arcball usable with a mouse;
    // before this it was driven by scroll events, which a wheel can only
    // deliver on one axis and in coarse notches.
    pub middle_pressed: bool,
    pub shift_pressed: bool,
    pub left_pressed: bool,
    pub alt_pressed: bool,

    /// Treat alt + left-drag as a middle-drag, for hardware with no middle
    /// button -- a trackpad. Blender calls this "Emulate 3 Button Mouse".
    pub emulate_middle_button: bool,

    pub sensitivity_move: Float,
    pub sensitivity_look: Float,
    pub sensitivity_rotate: Float,
    pub sensitivity_zoom: Float,
}

impl Controller {
    pub fn new(
        sensitivity_move: Float,
        sensitivity_look: Float,
        sensitivity_rotate: Float,
        sensitivity_zoom: Float,
    ) -> Self {
        Self {
            left: 0.0,
            right: 0.0,
            forward: 0.0,
            backward: 0.0,
            up: 0.0,
            down: 0.0,
            horizontal: 0.0,
            vertical: 0.0,
            pan_horizontal: 0.0,
            pan_vertical: 0.0,
            zoom: 0.0,
            middle_pressed: false,
            shift_pressed: false,
            left_pressed: false,
            alt_pressed: false,
            emulate_middle_button: cfg!(target_os = "macos"),
            sensitivity_move,
            sensitivity_look,
            sensitivity_rotate,
            sensitivity_zoom,
        }
    }

    pub fn handle_key(&mut self, key: winit::keyboard::KeyCode, is_pressed: bool) -> bool {
        let amount = if is_pressed { 1.0 } else { 0.0 };

        match key {
            winit::keyboard::KeyCode::KeyW => {
                self.forward = amount;
            }
            winit::keyboard::KeyCode::KeyS => {
                self.backward = amount;
            }
            winit::keyboard::KeyCode::KeyA => {
                self.left = amount;
            }
            winit::keyboard::KeyCode::KeyD => {
                self.right = amount;
            }
            winit::keyboard::KeyCode::Space => {
                self.up = amount;
            }
            winit::keyboard::KeyCode::ShiftLeft => {
                self.down = amount;
            }
            _ => {
                return false;
            }
        }

        true
    }

    // All three accumulate rather than assign: several input events can
    // arrive between two frames, and overwriting threw away everything but
    // the last one, which made fast drags lose motion.
    pub fn mouse_motion(&mut self, dx: Float, dy: Float) {
        self.horizontal += dx;
        self.vertical += dy;
    }

    pub fn pan(&mut self, dx: Float, dy: Float) {
        self.pan_horizontal += dx;
        self.pan_vertical += dy;
    }

    pub fn zoom(&mut self, delta: Float) {
        self.zoom += delta;
    }

    /// Whether the pointer is currently in an orbit/pan drag: the middle
    /// button, or alt + left when middle-button emulation is on.
    pub fn is_dragging(&self) -> bool {
        self.middle_pressed || (self.emulate_middle_button && self.left_pressed && self.alt_pressed)
    }

    /// Routes a pointer drag to orbit or pan depending on the shift key.
    pub fn drag(&mut self, dx: Float, dy: Float) {
        if self.shift_pressed {
            self.pan(dx, dy);
        } else {
            self.mouse_motion(dx, dy);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn controller() -> Controller {
        Controller::new(1.0, 1.0, 1.0, 1.0)
    }

    fn eye_at_distance(d: Float) -> Eye {
        let mut eye = Eye::new();
        eye.pos = Vec3::new(0.0, -d, 0.0);
        eye.anchor = Vec3::ZERO;
        eye.look_anchor();
        eye
    }

    /// The bug this guards: arcball input used to be multiplied by frame
    /// time, so the same physical drag rotated the camera ~8x further at
    /// 30 fps than at 240 fps. Pointer deltas are displacements, not rates.
    #[test]
    fn arcball_orbit_is_frame_rate_independent() {
        let mut slow = eye_at_distance(10.0);
        let mut fast = eye_at_distance(10.0);

        let mut ctrl = controller();
        ctrl.mouse_motion(50.0, 0.0);
        slow.update_with_controller(&mut ctrl, 1.0 / 30.0);

        let mut ctrl = controller();
        ctrl.mouse_motion(50.0, 0.0);
        fast.update_with_controller(&mut ctrl, 1.0 / 240.0);

        assert!(
            (slow.pos - fast.pos).length() < 1e-6,
            "same drag gave different results at 30 vs 240 fps: {} vs {}",
            slow.pos,
            fast.pos
        );
    }

    /// Several events can land between two frames; they must all count.
    #[test]
    fn arcball_input_accumulates_within_a_frame() {
        let mut once = eye_at_distance(10.0);
        let mut split = eye_at_distance(10.0);

        let mut ctrl = controller();
        ctrl.mouse_motion(40.0, 0.0);
        once.update_with_controller(&mut ctrl, 1.0 / 60.0);

        let mut ctrl = controller();
        ctrl.mouse_motion(10.0, 0.0);
        ctrl.mouse_motion(30.0, 0.0);
        split.update_with_controller(&mut ctrl, 1.0 / 60.0);

        assert!(
            (once.pos - split.pos).length() < 1e-6,
            "split events lost motion: {} vs {}",
            once.pos,
            split.pos
        );
    }

    #[test]
    fn arcball_orbit_keeps_distance_and_moves_sideways() {
        let mut eye = eye_at_distance(10.0);
        let start = eye.pos;

        let mut ctrl = controller();
        ctrl.mouse_motion(50.0, 0.0);
        eye.update_with_controller(&mut ctrl, 1.0 / 60.0);

        assert!(
            (eye.distance_anchor() - 10.0).abs() < 1e-4,
            "orbit changed the anchor distance: {}",
            eye.distance_anchor()
        );
        assert!(
            (eye.pos - start).length() > 1e-3,
            "orbit did not move the camera"
        );
        assert!(eye.pos.x < start.x, "horizontal drag orbited the wrong way");
    }

    /// Geometric zoom cannot step through the anchor however hard it is
    /// pushed, unlike the old linear `pos += dir * k * distance`.
    #[test]
    fn arcball_zoom_is_geometric_and_never_crosses_the_anchor() {
        let mut eye = eye_at_distance(10.0);

        let mut ctrl = controller();
        ctrl.zoom(1.0);
        eye.update_with_controller(&mut ctrl, 1.0 / 60.0);
        let after_one = eye.distance_anchor();
        assert!(after_one < 10.0, "zooming in did not get closer");

        // A huge scroll burst still lands in front of the anchor.
        let mut eye = eye_at_distance(10.0);
        let mut ctrl = controller();
        ctrl.zoom(1000.0);
        eye.update_with_controller(&mut ctrl, 1.0 / 60.0);

        assert!(
            eye.distance_anchor() > 0.0,
            "zoom crossed through the anchor"
        );
        assert!(eye.dir.dot(eye.anchor - eye.pos) > 0.0, "camera flipped");
    }

    #[test]
    fn arcball_pan_moves_eye_and_anchor_together() {
        let mut eye = eye_at_distance(10.0);
        let start_distance = eye.distance_anchor();

        let mut ctrl = controller();
        ctrl.shift_pressed = true;
        ctrl.drag(50.0, 0.0);
        eye.update_with_controller(&mut ctrl, 1.0 / 60.0);

        assert!(eye.anchor.length() > 1e-3, "pan did not move the anchor");
        assert!(
            (eye.distance_anchor() - start_distance).abs() < 1e-4,
            "pan changed the anchor distance"
        );
    }

    /// Shift is what separates pan from orbit, so the routing matters.
    #[test]
    fn drag_routes_on_shift() {
        let mut ctrl = controller();
        ctrl.drag(5.0, 7.0);
        assert_eq!((ctrl.horizontal, ctrl.vertical), (5.0, 7.0));
        assert_eq!((ctrl.pan_horizontal, ctrl.pan_vertical), (0.0, 0.0));

        let mut ctrl = controller();
        ctrl.shift_pressed = true;
        ctrl.drag(5.0, 7.0);
        assert_eq!((ctrl.horizontal, ctrl.vertical), (0.0, 0.0));
        assert_eq!((ctrl.pan_horizontal, ctrl.pan_vertical), (5.0, 7.0));
    }

    /// A trackpad has no middle button, so alt + left has to stand in for it
    /// -- otherwise the arcball cannot be orbited at all on that hardware,
    /// which is exactly what regressed when drag-to-orbit was introduced.
    #[test]
    fn alt_left_drag_substitutes_for_the_middle_button() {
        let mut ctrl = controller();
        ctrl.emulate_middle_button = true;

        assert!(!ctrl.is_dragging(), "idle pointer must not orbit");

        ctrl.left_pressed = true;
        assert!(!ctrl.is_dragging(), "plain left-drag must not orbit");

        ctrl.alt_pressed = true;
        assert!(ctrl.is_dragging(), "alt + left must orbit when emulating");

        // The real middle button keeps working regardless of alt.
        let mut ctrl = controller();
        ctrl.emulate_middle_button = true;
        ctrl.middle_pressed = true;
        assert!(ctrl.is_dragging());
    }

    /// A script assigning `up` parallel to `dir` used to make `fix_up`
    /// normalise a zero vector, poisoning the camera with NaN for the rest of
    /// the run -- the reported "orbit gets stuck".
    #[test]
    fn parallel_up_and_dir_do_not_poison_the_camera() {
        let mut eye = Eye::new();
        // Off the up_world axis, or a horizontal orbit would correctly be a
        // no-op and the test would prove nothing.
        eye.pos = Vec3::new(10.0, 0.0, 0.0);
        eye.anchor = Vec3::ZERO;
        eye.dir = Vec3::new(-1.0, 0.0, 0.0);
        eye.up = Vec3::new(-1.0, 0.0, 0.0); // degenerate: parallel to dir

        eye.sanitize_basis();
        assert!(eye.up.is_finite(), "up went NaN: {}", eye.up);
        assert!(
            eye.dir.cross(eye.up).length_squared() > 1e-6,
            "up must end up perpendicular to dir"
        );

        let mut ctrl = controller();
        ctrl.mouse_motion(30.0, 0.0);
        eye.arcball_update(&mut ctrl);

        assert!(eye.pos.is_finite() && eye.dir.is_finite() && eye.up.is_finite());
        assert!(
            (eye.pos - Vec3::new(10.0, 0.0, 0.0)).length() > 1e-3,
            "camera should have orbited, stayed at {}",
            eye.pos
        );
    }

    /// The Hera examples assign pos/dir/up from spice every frame. Orbiting
    /// has to work from whatever basis that leaves behind.
    #[test]
    fn orbit_works_from_a_script_assigned_basis() {
        let mut eye = Eye::new();
        // Roughly the AFC case: far from the body, boresight pointing back at
        // it, up taken from the instrument frame rather than the world.
        eye.pos = Vec3::new(20.0, 12.0, -6.0);
        eye.anchor = Vec3::ZERO;
        eye.dir = (eye.anchor - eye.pos).normalize();
        eye.up = Vec3::new(0.3, -0.9, 0.31).normalize();

        let start = eye.pos;
        let start_distance = eye.distance_anchor();

        let mut ctrl = controller();
        ctrl.mouse_motion(25.0, 10.0);
        eye.arcball_update(&mut ctrl);

        assert!(eye.pos.is_finite());
        assert!(
            (eye.pos - start).length() > 1e-3,
            "orbit did nothing from a custom basis"
        );
        assert!(
            (eye.distance_anchor() - start_distance).abs() < 1e-4,
            "orbit must preserve the anchor distance"
        );
    }

    /// Guard, not a reproduction: `look_anchor` re-orthonormalises `up` after
    /// every event, so a non-unit rotation axis only ever distorted the first
    /// one and never accumulated. This pins the invariant anyway, since the
    /// orbit is a rigid rotation about the anchor and any future change that
    /// makes it scale would be a bug.
    #[test]
    fn repeated_orbit_does_not_drift_the_anchor_distance() {
        let mut eye = Eye::new();
        eye.pos = Vec3::new(25.8, 4.0, -3.0);
        eye.anchor = Vec3::ZERO;
        eye.dir = (eye.anchor - eye.pos).normalize();
        // Deliberately not perpendicular to dir, as a script assigning an
        // instrument-frame up may well leave it.
        eye.up = Vec3::new(0.1, 0.2, 0.97).normalize();

        let start_distance = eye.distance_anchor();

        for _ in 0..200 {
            let mut ctrl = controller();
            ctrl.mouse_motion(7.0, 3.0);
            eye.arcball_update(&mut ctrl);
        }

        let drift = (eye.distance_anchor() - start_distance).abs() / start_distance;
        assert!(
            drift < 1e-3,
            "anchor distance drifted {:.3}% over 200 orbits ({} -> {})",
            100.0 * drift,
            start_distance,
            eye.distance_anchor()
        );
        assert!(eye.pos.is_finite() && eye.up.is_finite());
    }

    /// Eye sitting exactly on the anchor: there is no radius to orbit, which
    /// must leave the camera untouched rather than emit NaN.
    #[test]
    fn orbit_on_top_of_the_anchor_is_inert_not_nan() {
        let mut eye = Eye::new();
        eye.pos = Vec3::new(3.0, 3.0, 3.0);
        eye.anchor = eye.pos;

        let mut ctrl = controller();
        ctrl.mouse_motion(20.0, 20.0);
        eye.arcball_update(&mut ctrl);

        assert!(eye.pos.is_finite() && eye.up.is_finite() && eye.dir.is_finite());
        assert_eq!(eye.pos, Vec3::new(3.0, 3.0, 3.0));
    }

    /// With emulation off, alt + left must stay inert, or a user who wants
    /// alt for something else silently loses it.
    #[test]
    fn alt_left_drag_is_inert_without_emulation() {
        let mut ctrl = controller();
        ctrl.emulate_middle_button = false;
        ctrl.left_pressed = true;
        ctrl.alt_pressed = true;
        assert!(!ctrl.is_dragging());

        ctrl.middle_pressed = true;
        assert!(ctrl.is_dragging(), "middle button must still work");
    }
}
