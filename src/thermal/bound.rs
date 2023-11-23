use crate::util::*;

pub fn effective_temperature(
    position_sun: &Vec3,
    albedo: Float,
    emissivity: Float,
    ratio_areas: Float,
) -> Float {
    (ratio_areas * SOLAR_CONSTANT * (1.0 - albedo)
        / (emissivity * STEFAN_BOLTZMANN * (position_sun.magnitude() / AU).powi(2)))
    .powf(1.0 / 4.0)
}

pub fn newton_method_temperature(
    temperatures: DRVectorView<Float>,
    fluxes: &DRVector<Float>,
    emissivities: &DRVector<Float>,
    conductivities: &DRVector<Float>,
    subsurface_temperatures: DMatrix2xXView<Float>,
    depth: Float,
) -> DRVector<Float> {
    let mut index = 0;
    let mut old_temperatures = temperatures.clone_owned();

    loop {
        let new_temperatures = &old_temperatures
            - newton_method_temperature_function(
                &old_temperatures,
                fluxes,
                emissivities,
                conductivities,
                subsurface_temperatures,
                depth,
            )
            .component_div(&newton_method_temperature_derivative(
                &old_temperatures,
                emissivities,
                conductivities,
                depth,
            ));

        if index > NUMBER_ITERATION_FAIL
            || (&new_temperatures - &old_temperatures)
                .abs()
                .iter()
                .all(|&dt| dt < NEWTON_METHOD_THRESHOLD)
        {
            // println!("index: {}", index);
            return new_temperatures;
        }

        old_temperatures = new_temperatures;
        index += 1;
    }
}

pub fn newton_method_temperature_function(
    temperatures: &DRVector<Float>,
    fluxes: &DRVector<Float>,
    emissivities: &DRVector<Float>,
    conductivities: &DRVector<Float>,
    subsurface_temperatures: DMatrix2xXView<Float>,
    depth: Float,
) -> DRVector<Float> {
    fluxes - STEFAN_BOLTZMANN * emissivities.component_mul(&temperatures.map(|t| t.powi(4)))
        + conductivities.component_mul(
            &(-subsurface_temperatures.row(1) + 4.0 * subsurface_temperatures.row(0)
                - 3.0 * temperatures),
        ) / depth
}

pub fn newton_method_temperature_derivative(
    temperatures: &DRVector<Float>,
    emissivities: &DRVector<Float>,
    conductivities: &DRVector<Float>,
    depth: Float,
) -> DRVector<Float> {
    -4.0 * STEFAN_BOLTZMANN * emissivities.component_mul(&temperatures.map(|t| t.powi(3)))
        - 3.0 / depth * conductivities
}