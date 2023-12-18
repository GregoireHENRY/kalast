use crate::util::*;

use sdl2::keyboard::Keycode;

pub const ORIGIN: Vec3 = Vec3::new(0.0, 0.0, 0.0);
pub const SIDE: Float = 1.0;
pub const CLOSE: Float = 1e-4;
pub const FAR: Float = 3.0;
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

#[derive(Debug, Clone)]
pub struct FrustrumBase {
    pub aspect: Float,
    pub znear: Float,
    pub zfar: Float,
}

impl Default for FrustrumBase {
    fn default() -> Self {
        Self {
            aspect: ASPECT,
            znear: CLOSE,
            zfar: FAR,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ProjectionMode {
    Orthographic(Float), // side
    Perspective(Float),  // fovy
}

#[derive(Debug, Clone)]
pub struct Projection {
    pub base: FrustrumBase,
    pub mode: ProjectionMode,
}

impl Default for Projection {
    fn default() -> Self {
        Self::perspective()
    }
}

impl Projection {
    pub fn orthographic() -> Self {
        Self {
            base: FrustrumBase::default(),
            mode: ProjectionMode::Orthographic(SIDE),
        }
    }

    pub fn perspective() -> Self {
        Self {
            base: FrustrumBase::default(),
            mode: ProjectionMode::Perspective(FOVY),
        }
    }

    pub fn update_distance(&mut self, distance: Float) {
        self.base.zfar = distance * FAR;
        self.base.znear = distance * CLOSE;

        match &mut self.mode {
            ProjectionMode::Orthographic(d) => *d = distance,
            _ => {}
        };
    }

    pub fn build(&self) -> Mat4 {
        match self.mode {
            ProjectionMode::Orthographic(side) => glm::ortho(
                side * self.base.aspect,
                side * self.base.aspect,
                side,
                side,
                self.base.znear,
                self.base.zfar,
            ),
            ProjectionMode::Perspective(fovy) => {
                glm::perspective(self.base.aspect, fovy, self.base.znear, self.base.zfar)
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct Camera {
    pub up: Vec3,
    pub direction: Vec3,
    pub position: Vec3,
    pub origin: Vec3,
    pub projection: Projection,
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
            projection: Projection::perspective(),
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

    pub fn right(&self) -> Vec3 {
        glm::normalize(&glm::cross(&self.front(), &self.up))
    }

    pub fn left(&self) -> Vec3 {
        -self.right()
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
