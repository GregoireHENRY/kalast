use crate::util::*;

use na::Rotation3;
use sdl2::keyboard::Keycode;
use serde::{Deserialize, Serialize};

pub const UP: Vec3 = Vec3::new(0.0, 0.0, 1.0);
pub const DIRECTION: Vec3 = Vec3::new(-1.0, 0.0, 0.0);
pub const POSITION: Vec3 = Vec3::new(5.0, 0.0, 0.0);
pub const ANCHOR: Vec3 = Vec3::new(0.0, 0.0, 0.0);
pub const NEAR_FACTOR: Float = 1e-5;
pub const FAR_FACTOR: Float = 2.0;
pub const FOVY: Float = 30.0;

pub const SENSITIVITY: Float = 1.0;
pub const SENSITIVITY_CORRECTION: Float = 1e-3;
pub const SENSITIVITY_ROTATE_MOUSEWHEEL_CORRECTION: Float = 1e1;

pub const FREE_SENSITIVITY_CORRECTION: Float = 1e-1;
pub const FREE_KEYBOARD_SENSITIVITY_CORRECTION: Float = 1e-3;

pub const SPEED: Float = 1.0;
pub const SPEED_FAST_FACTOR: Float = 10.0;

pub const KEY_FORWARD: Keycode = Keycode::W;
pub const KEY_LEFT: Keycode = Keycode::A;
pub const KEY_BACKWARD: Keycode = Keycode::S;
pub const KEY_RIGHT: Keycode = Keycode::D;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MovementMode {
    Lock,
    Free,
}

#[derive(Clone, Debug, Copy, Serialize, Deserialize, PartialEq)]
pub enum ProjectionMode {
    #[serde(rename = "orthographic")]
    Orthographic,

    #[serde(rename = "perspective")]
    Perspective(Float), // fovy
}

impl Default for ProjectionMode {
    fn default() -> Self {
        Self::Perspective(FOVY)
    }
}

impl ProjectionMode {
    pub fn matrix(&self, near: Float, far: Float, aspect: Float) -> Mat4 {
        match self {
            ProjectionMode::Orthographic => {
                let side = far;
                glm::ortho(side * aspect, side * aspect, side, side, near, far)
            }
            ProjectionMode::Perspective(fovy) => glm::perspective(aspect, *fovy, near, far),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Camera {
    pub up: Vec3,
    pub direction: Vec3,
    pub position: Vec3,
    pub anchor: Vec3, // rotation is around anchor, in case need to rotate around some point different than origin
    pub projection: ProjectionMode,
    pub movement_mode: MovementMode,
    pub up_world: Vec3,
    pub near: Option<Float>,
    pub far: Option<Float>,
}

impl Camera {
    pub fn new() -> Self {
        Self {
            up: UP,
            direction: DIRECTION,
            position: POSITION,
            anchor: ANCHOR,
            projection: ProjectionMode::default(),
            movement_mode: MovementMode::Lock,
            up_world: UP,
            near: None,
            far: None,
        }
    }

    pub fn target(&self) -> Vec3 {
        self.position + self.direction
    }

    pub fn up(&self) -> Vec3 {
        self.up
    }

    pub fn bottom(&self) -> Vec3 {
        -self.up
    }

    pub fn front(&self) -> Vec3 {
        self.direction
    }

    pub fn back(&self) -> Vec3 {
        -self.direction
    }

    pub fn right(&self) -> Vec3 {
        glm::normalize(&glm::cross(&self.direction, &self.up))
    }

    pub fn left(&self) -> Vec3 {
        -self.right()
    }

    pub fn matrix_lookat(&self) -> Mat4 {
        glm::look_at(&self.position, &self.target(), &self.up)
    }

    pub fn mat(&self) -> Mat3 {
        Mat3::from_rows(&[
            self.direction.transpose(),
            self.right().transpose(),
            self.up.transpose(),
        ])
    }

    pub fn matrix_projection(&self, aspect: Float) -> Mat4 {
        let distance = self.position.magnitude();

        let near = if let Some(near) = self.near {
            near
        } else {
            distance * NEAR_FACTOR
        };

        let far = if let Some(far) = self.far {
            far
        } else {
            distance * FAR_FACTOR
        };

        self.projection.matrix(near, far, aspect)
    }

    pub fn reset_anchor(&mut self) {
        self.anchor = Vec3::zeros()
    }

    pub fn fix_up(&mut self) {
        self.up = glm::normalize(&glm::cross(&self.right(), &self.direction));
    }

    pub fn target_anchor(&mut self) {
        self.direction = (self.anchor - self.position).normalize();
        self.fix_up();
    }

    pub fn toggle_movement_mode(&mut self) -> MovementMode {
        self.movement_mode = match self.movement_mode {
            MovementMode::Lock => MovementMode::Free,
            MovementMode::Free => MovementMode::Lock,
        };
        self.movement_mode
    }

    pub fn lock_rotate(&mut self, x: Float, y: Float) {
        let correction = SENSITIVITY_CORRECTION;

        let m1 = Rotation3::new(self.up_world * x * correction);
        let m2 = Rotation3::new(self.right() * y * correction);
        let m = m1 * m2;

        self.position = self.anchor + m * (self.position - self.anchor);
        self.up = m * self.up;

        self.target_anchor();
    }

    pub fn free_movement(&mut self, x: Float, y: Float) {
        let correction = FREE_KEYBOARD_SENSITIVITY_CORRECTION;
        self.position += (y * self.front() + x * self.right()) * correction;
    }

    pub fn free_rotate(&mut self, x: Float, y: Float) {
        let correction = SENSITIVITY_CORRECTION * FREE_SENSITIVITY_CORRECTION;

        let m1 = Rotation3::new(self.up * x * correction);
        let m2 = Rotation3::new(self.right() * y * correction);
        let m = m1 * m2;

        self.up = m * self.up;
        self.direction = m * self.direction;
    }
}
