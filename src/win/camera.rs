use crate::util::*;

use na::Rotation3;
use sdl2::keyboard::Keycode;
use serde::{Deserialize, Serialize};

pub const DEFAULT_CAMERA_UP: Vec3 = Vec3::new(0.0, 0.0, 1.0);
pub const DEFAULT_CAMERA_DIRECTION: Vec3 = Vec3::new(-1.0, 0.0, 0.0);
pub const DEFAULT_CAMERA_POSITION: Vec3 = Vec3::new(5.0, 0.0, 0.0);
pub const DEFAULT_CAMERA_ANCHOR: Vec3 = Vec3::new(0.0, 0.0, 0.0);
pub const DEFAULT_NEAR_FACTOR: Float = 1e-4;
pub const DEFAULT_FAR_FACTOR: Float = 2.0;
pub const DEFAULT_FOVY: Float = 40.0 * RPD;
pub const DEFAULT_SENSITIVITY: Float = 1.0;
pub const SENSITIVITY_CORRECTION: Float = 1e-4;
pub const DEFAULT_SPEED: Float = 1.0;
pub const SPEED_CORRECTION: Float = 1.0;
pub const KEYBOARD_SENSITIVITY_CORRECTION: Float = 1e-2;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MovementMode {
    Rotate,
    Strafe,
}

#[derive(Debug, Clone, Copy)]
pub enum Direction {
    Up,
    Left,
    Down,
    Right,
}

impl Direction {
    pub fn from_keycode(keycode: Keycode) -> Self {
        match keycode {
            Keycode::Up => Self::Up,
            Keycode::Left => Self::Left,
            Keycode::Down => Self::Down,
            Keycode::Right => Self::Right,
            _ => panic!("Not a valid keycode for direction: {:?}.", keycode),
        }
    }

    pub fn to_xy(&self) -> (i32, i32) {
        match self {
            Self::Up => (0, 1),
            Self::Left => (-1, 0),
            Self::Down => (0, -1),
            Self::Right => (1, 0),
        }
    }
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
        Self::Perspective(DEFAULT_FOVY)
    }
}

impl ProjectionMode {
    pub fn matrix(&self, distance: Float, aspect: Float) -> Mat4 {
        let side = distance;
        let zfar = distance * DEFAULT_FAR_FACTOR;
        let znear = zfar * DEFAULT_NEAR_FACTOR;

        match self {
            ProjectionMode::Orthographic => {
                glm::ortho(side * aspect, side * aspect, side, side, znear, zfar)
            }
            ProjectionMode::Perspective(fovy) => glm::perspective(aspect, *fovy, znear, zfar),
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
}

impl Camera {
    pub fn new() -> Self {
        Self {
            up: DEFAULT_CAMERA_UP,
            direction: DEFAULT_CAMERA_DIRECTION,
            position: DEFAULT_CAMERA_POSITION,
            anchor: DEFAULT_CAMERA_ANCHOR,
            projection: ProjectionMode::default(),
            movement_mode: MovementMode::Rotate,
            up_world: DEFAULT_CAMERA_UP,
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
        self.projection.matrix(self.position.magnitude(), aspect)
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

    pub fn move_command(&mut self, keycode: Keycode, delta_time: Float) {
        let direction = Direction::from_keycode(keycode);
        match self.movement_mode {
            MovementMode::Rotate => {
                let (x, y) = direction.to_xy();
                self.rotate_around_anchor(x, y, delta_time)
            }
            MovementMode::Strafe => self.strafe(direction, delta_time),
        };
    }

    pub fn toggle_movement_mode(&mut self) -> MovementMode {
        self.movement_mode = match self.movement_mode {
            MovementMode::Rotate => MovementMode::Strafe,
            MovementMode::Strafe => MovementMode::Rotate,
        };
        self.movement_mode
    }

    /*
    pub fn change_speed(&mut self, direction: Direction, delta_time: Float) {
        let delta_speed = match direction {
            Direction::Up => 1.0,
            Direction::Down => -1.0,
            Direction::Left | Direction::Right => {
                unreachable!("Camera::change_speed match unreachable.")
            }
        } * DEFAULT_SENSITIVITY
            * DEFAULT_SENSITIVITY_CORRECTION
            * delta_time as Float;

        // self.speed += delta_speed;
    }
    */

    pub fn rotate_around_anchor(&mut self, x: i32, y: i32, delta_time: Float) {
        let sensitivity = DEFAULT_SENSITIVITY * KEYBOARD_SENSITIVITY_CORRECTION * delta_time;

        let m1 = Rotation3::new(self.up_world * x as Float * sensitivity);
        let m2 = Rotation3::new(-self.right() * y as Float * sensitivity);
        let m = m1 * m2;

        self.position = self.anchor + m * (self.position - self.anchor);
        self.up = m * self.up;

        self.target_anchor();
    }

    pub fn strafe(&mut self, direction: Direction, delta_time: Float) {
        let sensitivity =
            DEFAULT_SPEED * KEYBOARD_SENSITIVITY_CORRECTION * SPEED_CORRECTION * delta_time;

        let delta_position = (match direction {
            Direction::Up => self.front(),
            Direction::Left => self.left(),
            Direction::Down => self.back(),
            Direction::Right => self.right(),
        }) * sensitivity;

        self.position += delta_position;
    }

    pub fn rotate_strafe(&mut self, x: i32, y: i32, delta_time: Float) {
        let sensitivity = DEFAULT_SENSITIVITY * SENSITIVITY_CORRECTION * delta_time;
        let mut sphericals = cartesian_to_spherical(&self.direction);

        sphericals[1] -= x as Float * sensitivity;
        sphericals[2] = (sphericals[2] - y as Float * sensitivity).clamp(-PI / 2.0, PI / 2.0);

        self.direction = spherical_to_cartesian(&sphericals);
        self.fix_up();
    }
}
