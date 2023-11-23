use crate::{NEWTON_METHOD_THRESHOLD, NUMBER_ITERATION_FAIL, util::*};

pub fn mean_angular_motion(gravitational_parameter: Float, semi_major_axis: Float) -> Float {
    (gravitational_parameter / semi_major_axis.powi(3)).sqrt()
}

pub fn mean_anomaly(mean_angular_motion: Float, time: Float, time_at_periapsis: Float) -> Float {
    mean_angular_motion * (time - time_at_periapsis)
}

pub fn mean_anomaly_from_eccentric(eccentric_anomaly: Float, eccentricity: Float) -> Float {
    eccentric_anomaly - eccentricity * eccentric_anomaly.sin()
}

pub fn eccentric_anomaly(mean_anomaly: Float, eccentricity: Float) -> Float {
    let mut index = 0;
    let mut eccentric_anomaly_guess: Float = 0.0;

    'convergence: loop {
        // Get new value.
        let value_function =
            eccentric_anomaly_guess - eccentricity * eccentric_anomaly_guess.sin() - mean_anomaly;
        let value_derivative = 1.0 - eccentricity * eccentric_anomaly_guess.cos();
        let eccentric_anomaly = eccentric_anomaly_guess - value_function / value_derivative;

        // Break conditions.
        if index > NUMBER_ITERATION_FAIL
            || (eccentric_anomaly - eccentric_anomaly_guess).abs() < NEWTON_METHOD_THRESHOLD
        {
            eccentric_anomaly_guess = eccentric_anomaly;
            break 'convergence;
        }

        // Prepare for next iteration if break condition didn't pass.
        eccentric_anomaly_guess = eccentric_anomaly;
        index += 1;
    }

    eccentric_anomaly_guess
}

pub fn true_anomaly_from_eccentric_anomaly(eccentricity: Float, eccentric_anomaly: Float) -> Float {
    2.0 * (((1.0 + eccentricity) / (1.0 - eccentricity)).sqrt() * (eccentric_anomaly / 2.0).tan())
        .atan()
}

pub fn true_anomaly(
    semi_major_axis: Float,
    eccentricity: Float,
    time: Float,
    time_at_periapsis: Float,
    gravitational_parameter: Float,
) -> Float {
    let mean_angular_motion = mean_angular_motion(gravitational_parameter, semi_major_axis);
    let mean_anomaly = mean_anomaly(mean_angular_motion, time, time_at_periapsis);
    let eccentric_anomaly = eccentric_anomaly(mean_anomaly, eccentricity);

    true_anomaly_from_eccentric_anomaly(eccentricity, eccentric_anomaly)
}

pub fn radius(semi_major_axis: Float, eccentricity: Float, eccentric_anomaly: Float) -> Float {
    semi_major_axis * (1.0 - eccentricity * eccentric_anomaly.cos())
}

pub fn radius_from_true_anomaly(
    semi_major_axis: Float,
    eccentricity: Float,
    true_anomaly: Float,
) -> Float {
    semi_major_axis * (1.0 - eccentricity.powi(2)) / (1.0 + eccentricity * true_anomaly.cos())
}

pub fn parameter_ellipse(semi_major_axis: Float, eccentricity: Float) -> Float {
    semi_major_axis * (1.0 - eccentricity.powi(2))
}

pub fn angular_momentum(
    gravitational_parameter: Float,
    semi_major_axis: Float,
    eccentricity: Float,
) -> Float {
    (gravitational_parameter * semi_major_axis * (1.0 - eccentricity.powi(2))).sqrt()
}

pub fn position(
    radius: Float,
    longitude_ascending_node: Float,
    argument_periapsis: Float,
    true_anomaly: Float,
    inclination: Float,
) -> Vec3 {
    vec3(
        radius
            * (longitude_ascending_node.cos() * (argument_periapsis + true_anomaly).cos()
                - longitude_ascending_node.sin()
                    * (argument_periapsis + true_anomaly).sin()
                    * inclination.cos()),
        radius
            * (longitude_ascending_node.sin() * (argument_periapsis + true_anomaly).cos()
                + longitude_ascending_node.cos()
                    * (argument_periapsis + true_anomaly).sin()
                    * inclination.cos()),
        radius * (inclination.sin() * (argument_periapsis * true_anomaly).sin()),
    )
}

pub fn velocity(
    position: &Vec3,
    angular_momentum: Float,
    eccentricity: Float,
    radius: Float,
    parameter_ellipse: Float,
    true_anomaly: Float,
    longitude_ascending_node: Float,
    argument_periapsis: Float,
    inclination: Float,
) -> Vec3 {
    vec3(
        (position.x * angular_momentum * eccentricity) / (radius * parameter_ellipse)
            * true_anomaly.sin()
            - angular_momentum / radius
                * (longitude_ascending_node.cos() * (argument_periapsis + true_anomaly).sin()
                    + longitude_ascending_node.sin()
                        * (argument_periapsis + true_anomaly).cos()
                        * inclination.cos()),
        (position.y * angular_momentum * eccentricity) / (radius * parameter_ellipse)
            * true_anomaly.sin()
            - angular_momentum / radius
                * (longitude_ascending_node.sin() * (argument_periapsis + true_anomaly).sin()
                    - longitude_ascending_node.cos()
                        * (argument_periapsis + true_anomaly).cos()
                        * inclination.cos()),
        (position.z * angular_momentum * eccentricity) / (radius * parameter_ellipse)
            * true_anomaly.sin()
            + angular_momentum / radius
                * inclination.sin()
                * (argument_periapsis + true_anomaly).cos(),
    )
}

pub fn elements_to_state(
    semi_major_axis: Float,
    eccentricity: Float,
    inclination: Float,
    argument_periapsis: Float,
    longitude_ascending_node: Float,
    time_at_periapsis: Float,
    gravitational_parameter: Float,
    time: Float,
) -> Vec6 {
    let mean_angular_motion = mean_angular_motion(gravitational_parameter, semi_major_axis);
    let mean_anomaly = mean_anomaly(mean_angular_motion, time, time_at_periapsis);
    let eccentric_anomaly = eccentric_anomaly(mean_anomaly, eccentricity);
    let radius = radius(semi_major_axis, eccentricity, eccentric_anomaly);

    let true_anomaly = true_anomaly_from_eccentric_anomaly(eccentricity, eccentric_anomaly);
    let p = parameter_ellipse(semi_major_axis, eccentricity);
    let angular_momentum = angular_momentum(gravitational_parameter, semi_major_axis, eccentricity);

    let pos = position(
        radius,
        longitude_ascending_node,
        argument_periapsis,
        true_anomaly,
        inclination,
    );
    let vel = velocity(
        &pos,
        angular_momentum,
        eccentricity,
        radius,
        p,
        true_anomaly,
        longitude_ascending_node,
        argument_periapsis,
        inclination,
    );
    Vec6::new(pos.x, pos.y, pos.z, vel.x, vel.y, vel.z)
}

pub fn position_in_perifocal_frame(
    semi_major_axis: Float,
    eccentricity: Float,
    true_anomaly: Float,
) -> Vec3 {
    let p = parameter_ellipse(semi_major_axis, eccentricity);
    let r = p / (1.0 + eccentricity * true_anomaly.cos());

    r * vec3(true_anomaly.cos(), true_anomaly.sin(), 0.0)
}

pub fn position_in_inertial_frame_from_true_anomaly(
    semi_major_axis: Float,
    eccentricity: Float,
    inclination: Float,
    longitude_ascending_node: Float,
    argument_periapsis: Float,
    true_anomaly: Float,
) -> Vec3 {
    let p = parameter_ellipse(semi_major_axis, eccentricity);
    let r = p / (1.0 + eccentricity * true_anomaly.cos());

    r * vec3(
        (true_anomaly + argument_periapsis).cos() * longitude_ascending_node.cos()
            - inclination.cos()
                * (true_anomaly + argument_periapsis).sin()
                * longitude_ascending_node.sin(),
        (true_anomaly + argument_periapsis).cos() * longitude_ascending_node.sin()
            + inclination.cos()
                * (true_anomaly + argument_periapsis).sin()
                * longitude_ascending_node.cos(),
        (true_anomaly + argument_periapsis).sin() * inclination.sin(),
    )
}

pub fn position_in_inertial_frame_from_mean_anomaly(
    semi_major_axis: Float,
    eccentricity: Float,
    inclination: Float,
    longitude_ascending_node: Float,
    argument_periapsis: Float,
    mean_anomaly: Float,
) -> Vec3 {
    #[allow(non_snake_case)]
    let E = eccentric_anomaly(mean_anomaly, eccentricity);
    let v = true_anomaly_from_eccentric_anomaly(eccentricity, E);
    let p = parameter_ellipse(semi_major_axis, eccentricity);
    let r = p / (1.0 + eccentricity * v.cos());

    r * vec3(
        (v + argument_periapsis).cos() * longitude_ascending_node.cos()
            - inclination.cos() * (v + argument_periapsis).sin() * longitude_ascending_node.sin(),
        (v + argument_periapsis).cos() * longitude_ascending_node.sin()
            + inclination.cos() * (v + argument_periapsis).sin() * longitude_ascending_node.cos(),
        (v + argument_periapsis).sin() * inclination.sin(),
    )
}

pub fn position_in_inertial_frame(
    semi_major_axis: Float,
    eccentricity: Float,
    inclination: Float,
    longitude_ascending_node: Float,
    argument_periapsis: Float,
    time: Float,
    time_at_periapsis: Float,
    gravitational_parameter: Float,
) -> Vec3 {
    let v = true_anomaly(
        semi_major_axis,
        eccentricity,
        time,
        time_at_periapsis,
        gravitational_parameter,
    );
    let p = parameter_ellipse(semi_major_axis, eccentricity);
    let r = p / (1.0 + eccentricity * v.cos());

    r * vec3(
        (v + argument_periapsis).cos() * longitude_ascending_node.cos()
            - inclination.cos() * (v + argument_periapsis).sin() * longitude_ascending_node.sin(),
        (v + argument_periapsis).cos() * longitude_ascending_node.sin()
            + inclination.cos() * (v + argument_periapsis).sin() * longitude_ascending_node.cos(),
        (v + argument_periapsis).sin() * inclination.sin(),
    )
}
