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
        let Resolved { near, far, side } = self.resolved;

        match self.mode {
            ProjectionMode::Orthographic => {
                let half_height = side;
                let half_width = half_height * aspect;

                Mat4::orthographic_rh(
                    -half_width,
                    half_width,
                    -half_height,
                    half_height,
                    near,
                    far,
                )
            }
            ProjectionMode::Perspective => Mat4::perspective_rh(self.fovy, aspect, near, far),
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

    pub fn right(&self) -> Vec3 {
        self.dir.cross(self.up)
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

    pub fn fix_up(&mut self) {
        self.up = self.right().cross(self.dir).normalize();
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
                let mut side = bounds.radius() * margin;

                if let Some(texels) = shadow_texels.filter(|t| *t > 0) {
                    // Quantise the size too -- snapping the offset alone is
                    // pointless if the texel size itself keeps changing.
                    let quantum = 2.0 * side / texels as Float;
                    side = (side / quantum).ceil() * quantum;
                }

                // Keep the whole scene in depth even when the light sits
                // inside the bounds; near may go negative for an ortho
                // projection, which is fine and avoids clipping casters
                // behind the light's nominal position.
                Resolved {
                    near: min_d - bounds.radius() * margin,
                    far: max_d + bounds.radius() * margin,
                    side,
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

        if ctrl.horizontal != 0.0 || ctrl.vertical != 0.0 {
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
}
