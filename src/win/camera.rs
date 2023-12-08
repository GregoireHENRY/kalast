use std::fmt::Debug;

use crate::util::*;

use downcast_rs::{impl_downcast, DowncastSync};
use dyn_clone::DynClone;
use sdl2::keyboard::Keycode;

pub const ORIGIN: Vec3 = Vec3::new(0.0, 0.0, 0.0);
pub const SIDE: Float = 1.0;
pub const CLOSE: Float = 1e-4;
pub const FAR: Float = 2.0;
pub const ASPECT: Float = 1.0;
pub const FOVY: Float = 30.0 * RPD;
pub const SPEED: Float = 0.5;

#[derive(Debug, Clone, Copy)]
pub enum MovementMethod {
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
            Keycode::Up => Direction::Up,
            Keycode::Left => Direction::Left,
            Keycode::Down => Direction::Down,
            Keycode::Right => Direction::Right,
            _ => panic!(
                "Keycode {:?} should not be used for Direction Enum creation.",
                keycode
            ),
        }
    }
}

pub trait Projection: DowncastSync + Debug + DynClone {
    fn new() -> Self
    where
        Self: Sized;
    fn build(&self) -> Mat4;
}

impl_downcast!(sync Projection);
dyn_clone::clone_trait_object!(Projection);

#[derive(Debug, Clone)]
pub struct Orthographic {
    pub aspect: Float,
    pub side: Float,
    pub znear: Float,
    pub zfar: Float,
}

impl Default for Orthographic {
    fn default() -> Self {
        Self {
            aspect: ASPECT,
            side: SIDE,
            znear: CLOSE,
            zfar: FAR,
        }
    }
}

impl Projection for Orthographic {
    fn new() -> Self {
        Self::default()
    }

    fn build(&self) -> Mat4 {
        glm::ortho(
            self.side * self.aspect,
            self.side * self.aspect,
            self.side,
            self.side,
            self.znear,
            self.zfar,
        )
    }
}

#[derive(Debug, Clone)]
pub struct Perspective {
    pub aspect: Float,
    pub fovy: Float,
    pub znear: Float,
    pub zfar: Float,
}

impl Default for Perspective {
    fn default() -> Self {
        Self {
            aspect: ASPECT,
            fovy: SIDE,
            znear: CLOSE,
            zfar: FAR,
        }
    }
}

impl Projection for Perspective {
    fn new() -> Self {
        Self::default()
    }

    fn build(&self) -> Mat4 {
        glm::perspective(self.aspect, self.fovy, self.znear, self.zfar)
    }
}

#[derive(Debug, Clone)]
pub enum ProjectionMode {
    Orthographic,
    Perspective,
}

#[derive(Debug, Clone)]
pub struct Camera {
    pub up: Vec3,
    pub direction: Vec3,
    pub position: Vec3,
    pub origin: Vec3,
    pub projection: Box<dyn Projection>,
    speed: Float,
    movement_method: MovementMethod,
}

impl Camera {
    pub fn new(up: Vec3, direction: Vec3, position: Vec3) -> Self {
        Self {
            up,
            direction,
            position,
            origin: ORIGIN,
            projection: Box::new(Perspective::new()),
            speed: SPEED,
            movement_method: MovementMethod::Rotate,
        }
    }

    pub fn target(&self) -> Vec3 {
        self.position + self.direction
    }

    pub fn front(&self) -> Vec3 {
        self.direction.clone()
    }

    pub fn back(&self) -> Vec3 {
        -self.front()
    }

    pub fn left(&self) -> Vec3 {
        glm::normalize(&glm::cross(&self.front(), &self.up))
    }

    pub fn right(&self) -> Vec3 {
        -self.left()
    }

    pub fn get_look_at_matrix(&self) -> Mat4 {
        glm::look_at(&self.position, &self.target(), &self.up)
    }

    pub fn reset_origin(&mut self) {
        self.origin = Vec3::zeros()
    }

    pub fn target_origin(&mut self) {
        self.direction = (self.origin - self.position).normalize();
    }

    pub fn move_command(&mut self, direction: Direction, delta_time: Float) {
        match self.movement_method {
            MovementMethod::Rotate => self.rotate_around_origin(direction, delta_time),
            MovementMethod::Strafe => self.strafe(direction, delta_time),
        };
    }

    pub fn toggle_move_method(&mut self) {
        self.movement_method = match self.movement_method {
            MovementMethod::Rotate => MovementMethod::Strafe,
            MovementMethod::Strafe => MovementMethod::Rotate,
        };
    }

    pub fn set_speed(&mut self, speed: Float) {
        self.speed = speed;
    }

    pub fn change_speed(&mut self, direction: Direction, delta_time: Float) {
        let delta_speed = match direction {
            Direction::Up => 0.1,
            Direction::Down => -0.1,
            Direction::Left | Direction::Right => {
                unreachable!("Camera::change_speed match unreachable.")
            }
        } * self.speed
            * delta_time as Float;

        self.speed += delta_speed;
    }

    pub fn rotate_around_origin(&mut self, direction: Direction, delta_time: Float) {
        let speed = self.speed * delta_time;
        let mut sphericals = cartesian_to_spherical(&self.position);

        let (delta_theta, delta_phi) = match direction {
            Direction::Up => (0.0, 1.0 * RPD),
            Direction::Left => (-1.0 * RPD, 0.0),
            Direction::Down => (0.0, -1.0 * RPD),
            Direction::Right => (1.0 * RPD, 0.0),
        };

        let mut new_phi = sphericals[2] + (delta_phi * speed);

        if (new_phi < -PI / 2.0) || (new_phi > PI / 2.0) {
            new_phi = sphericals[2];
        }

        sphericals[1] += delta_theta * speed;
        sphericals[2] = new_phi;

        self.position = self.origin + spherical_to_cartesian(&sphericals);
        self.target_origin();
    }

    pub fn strafe(&mut self, direction: Direction, delta_time: Float) {
        let delta_position = (match direction {
            Direction::Up => self.front(),
            Direction::Left => self.left() * 0.1,
            Direction::Down => self.back(),
            Direction::Right => self.right() * 0.1,
        }) * self.speed
            * 0.01
            * delta_time
            * self.position.magnitude();

        self.position += delta_position;
    }
}
