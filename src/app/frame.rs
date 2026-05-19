use crate::{Float, Mat3, Mat4, Vec3};

pub const SENSITIVITY_MOVE: Float = 0.5;
pub const SENSITIVITY_LOOK: Float = 0.1;
pub const SENSITIVITY_ROTATE: Float = 0.1;
pub const SENSITIVITY_ZOOM: Float = 1.0e1;

#[derive(Debug, Clone)]
pub struct Projection {
    pub mode: ProjectionMode,
    pub fovy: Float, // radian
    pub near: Float,
    pub far: Float,
    pub side: Float,
}

impl Projection {
    pub fn new() -> Self {
        Self {
            mode: ProjectionMode::Perspective,
            fovy: 0.5236, // ~45 degrees
            near: 0.01,
            far: 100.0,
            side: 5.0,
        }
    }

    // right-handed, Z axis points out of the screen
    // aspect: window width / height
    pub fn mat(&self, aspect: Float) -> Mat4 {
        match self.mode {
            ProjectionMode::Orthographic => {
                let half_height = self.side;
                let half_width = half_height * aspect;

                Mat4::orthographic_rh(
                    -half_width,
                    half_width,
                    -half_height,
                    half_height,
                    self.near,
                    self.far,
                )
            }
            ProjectionMode::Perspective => {
                Mat4::perspective_rh(self.fovy, aspect, self.near, self.far)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProjectionMode {
    Orthographic,

    Perspective,
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

    pub fn arcball_rotate(&mut self, ctrl: &mut Controller, dt: Float) {
        // zoom
        self.pos += self.dir
            * ctrl.zoom
            * ctrl.sensitivity_zoom
            * SENSITIVITY_ZOOM
            * dt
            * self.distance_anchor();

        // calc matrices
        let m1 = Mat3::from_axis_angle(
            self.up_world,
            ctrl.horizontal * ctrl.sensitivity_rotate * SENSITIVITY_ROTATE * dt,
        );
        let m2 = Mat3::from_axis_angle(
            self.right(),
            ctrl.vertical * ctrl.sensitivity_rotate * SENSITIVITY_ROTATE * dt,
        );
        let m = m1 * m2;

        // update movement and cam dir
        self.pos = self.anchor + m * (self.pos - self.anchor);
        self.up = m * self.up;
        self.look_anchor();
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
            Control::Arcball => self.arcball_rotate(ctrl, dt),
            Control::WASD => self.wasd_with_conroller(ctrl, dt),
            Control::None => {}
        };

        // reset mouse amounts
        ctrl.horizontal = 0.0;
        ctrl.vertical = 0.0;
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
    pub zoom: Float,
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
            zoom: 0.0,
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

    pub fn mouse_motion(&mut self, dx: Float, dy: Float) {
        self.horizontal = dx;
        self.vertical = dy;
    }

    pub fn zoom(&mut self, delta: Float) {
        self.zoom = delta;
    }
}
