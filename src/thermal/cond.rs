use crate::prelude::*;

pub fn heat_conduction_1d(
    slice: &[Float],
    delta_time: Float,
    depth_steps: &[Float],
    diffusivities: &[Float],
) -> Vec<Float> {
    let it_prev = slice.clone().iter();
    let it_cur = it_prev.clone().skip(1);
    let it_next = it_prev.clone().skip(2);

    izip!(it_prev, it_cur, it_next, depth_steps, diffusivities)
        .map(|(p, c, n, dx, diffusivity)| {
            c + diffusivity * delta_time / dx.powi(2) * (p - 2. * c + n)
        })
        .collect()
}

pub fn one_layer_flux_conduction(
    temperature: DRVectorRef<Float>,
    heat_flux: &DRVector<Float>,
    conductivity: &DRVector<Float>,
    distance: Float,
) -> DRVector<Float> {
    temperature + distance * heat_flux.component_div(conductivity)
}
