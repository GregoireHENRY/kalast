use crate::util::*;

use sdl2::keyboard::Keycode;

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
pub struct Camera {
    pub up: Vec3,
    pub target: Vec3,
    pub position: Vec3,
    pub speed: Float,
    pub movement_method: MovementMethod,
}

impl Camera {
    pub fn new(up: Vec3, target: Vec3, position: Vec3, speed: Float) -> Self {
        Self {
            up,
            target,
            position,
            speed,
            movement_method: MovementMethod::Rotate,
        }
    }

    pub fn front(&self) -> Vec3 {
        glm::normalize(&(self.target - self.position))
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

    pub fn look_at(&self) -> Mat4 {
        glm::look_at(&self.position, &self.target, &self.up)
    }

    pub fn target_origin(&mut self) {
        self.target = glm::vec3(0.0, 0.0, 0.0);
    }

    pub fn move_command(&mut self, direction: Direction, delta_time: Float) {
        match self.movement_method {
            MovementMethod::Rotate => self.rotate_around(direction, delta_time),
            MovementMethod::Strafe => self.strafe(direction, delta_time),
        };
    }

    pub fn change_move_method(&mut self) {
        self.movement_method = match self.movement_method {
            MovementMethod::Rotate => MovementMethod::Strafe,
            MovementMethod::Strafe => MovementMethod::Rotate,
        };
    }

    pub fn change_speed(&mut self, direction: Direction, delta_time: Float) {
        // Change 1% of the speed.

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

    pub fn rotate_around(&mut self, direction: Direction, delta_time: Float) {
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

        self.position = spherical_to_cartesian(&sphericals);
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
        self.target += delta_position;
    }
}
