use crate::util::*;

// Sun distance in AU.
pub fn effective_temperature(
    sun_distance: Float,
    albedo: Float,
    emissivity: Float,
    ratio_areas: Float,
) -> Float {
    (ratio_areas * SOLAR_CONSTANT * (1.0 - albedo)
        / (emissivity * STEFAN_BOLTZMANN * sun_distance.powi(2)))
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
    let mut temperatures = temperatures.clone_owned();

    loop {
        let f = newton_method_temperature_function(
            &temperatures,
            fluxes,
            emissivities,
            conductivities,
            subsurface_temperatures,
            depth,
        );

        let df = newton_method_temperature_derivative(
            &temperatures,
            emissivities,
            conductivities,
            depth,
        );

        let delta = (-&f).component_div(&df);
        temperatures += &delta;

        /*
        println!(
            "Newton f: {:.1}±({:.1})/{:.1}/{:.1} | df: {:.1}±({:.1})/{:.1}/{:.1} | dT: {:.1}±({:.1})/{:.1}/{:.1}",
            f.mean(),
            f.variance().sqrt(),
            f.max(),
            f.min(),
            df.mean(),
            df.variance().sqrt(),
            df.max(),
            df.min(),
            delta.mean(),
            delta.variance().sqrt(),
            delta.max(),
            delta.min(),
        );
         */

        if index > NUMBER_ITERATION_FAIL {
            panic!("Newton method never converged.");
        }

        if delta.abs().iter().all(|&dt| dt < NEWTON_METHOD_THRESHOLD) {
            // println!("index: {}", index);
            return temperatures;
        }

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
        ) / (2.0 * depth)
}

pub fn newton_method_temperature_derivative(
    temperatures: &DRVector<Float>,
    emissivities: &DRVector<Float>,
    conductivities: &DRVector<Float>,
    depth: Float,
) -> DRVector<Float> {
    -4.0 * STEFAN_BOLTZMANN * emissivities.component_mul(&temperatures.map(|t| t.powi(3)))
        - 3.0 / (2.0 * depth) * conductivities
}
