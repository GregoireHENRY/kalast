use crate::prelude::*;

pub fn cosine_angle(direction: &Vec3, normals: &Matrix3xX<Float>) -> DRVector<Float> {
    (-(-direction.transpose() * normals)).map(|x| x.max(0.0))
}

pub fn flux_solar_radiation(
    cos_incidences: &DRVector<Float>,
    albedos: &DRVector<Float>,
    sundist: Float,
) -> DRVector<Float> {
    SOLAR_CONSTANT / (sundist / AU).powi(2) * albedos.map(|a| 1.0 - a).component_mul(cos_incidences)
}

/**
Compute the diffuse solar radiation with another body.

## Expression

The diffuse solar radiation contribution from all $N$ facets $i$ the other body onto the
facet $j$ of the body is defined as,

$$W_{i}=\sum_{\substack{j \\ j\neq i}}^{N}V_{ij}\frac{S_\odot A\cos\varsigma_j\left(t\right)}{r_H^2\left(t\right)}$$

where $V_{ij}$ is the view factor describing the fraction of energy emitted from one facet
$i$ towards the facet $j$, $S_\odot$ is [Solar Constant][SOLAR_CONSTANT], $A$ the albedo,
$\varsigma_j$ the illumination angle of the facet $j$, and $r_H$ the heliocentric distance
in [AU][ASTRONOMICAL_UNIT].
*/
#[allow(unused)]
pub fn diffuse_solar_radiation(
    view_factor: &DMatrix<Float>,
    other_solar_fluxes: &DRVector<Float>,
    other_albedos: &DRVector<Float>,
) -> DRVector<Float> {
    (view_factor
        * other_solar_fluxes
            .component_mul(&other_albedos.map(|a| a / (1.0 - a)))
            .transpose())
    .transpose()
}

/*
Compute the direct thermal heating from another body of the system.

## Expression

The direct thermal heating contribution from all $N$ facets $i$ of the other body onto the
facet $j$ of the body is defined as,

$$u_{j}=\sum_{i\cancel{=}j}^{N}V_{ij}\epsilon\sigma T_{i}^4$$

where $V_{ij}$ is the view factor describing the fraction of energy emitted from one facet
$i$ towards the facet $j$, $\epsilon$ the emissivity, $\sigma$ the
[Stefan-Boltzmann constant][STEFAN_BOLTZMANN], and $T_i$ the temperature of the facet $i$.
*/
#[allow(unused)]
pub fn direct_thermal_heating(
    view_factor: &DMatrix<Float>,
    other_temperatures: &DRVector<Float>,
    other_emissivities: &DRVector<Float>,
) -> DRVector<Float> {
    (view_factor
        * other_emissivities
            .component_mul(&other_temperatures.map(|t| t.powi(4) * STEFAN_BOLTZMANN))
            .transpose())
    .transpose()
}

pub fn plank_law(wavelength: Float, temperature: Float) -> Float {
    2.0 * PLANK_CONSTANT * SPEED_LIGHT.powi(2)
        / wavelength.powi(5)
        / ((PLANK_CONSTANT * SPEED_LIGHT / (wavelength * BOLTZMANN_CONSTANT * temperature)).exp()
            - 1.0)
}
