use crate::prelude::*;
use serde::{Deserialize, Serialize};

#[allow(unused)]
fn thermal_skin_depth(diffusivity: Float, period: Float) -> Float {
    (diffusivity * period / PI).sqrt()
}

/// units: W.m^{-1}.K^{-1}
fn conductivity(thermal_inertia: Float, density: Float, heat_capacity: Float) -> Float {
    thermal_inertia.powi(2) / (density * heat_capacity)
}

fn diffusivity(conductivity: Float, density: Float, heat_capacity: Float) -> Float {
    conductivity / (density * heat_capacity)
}

/// Material properties.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Material {
    /// The surface albedo defines the capacity to reflect the light.
    #[serde(default)]
    pub albedo: Float,

    /// Surface emissivity. For a black body it is 1.0.
    #[serde(default = "default_emissivity")]
    pub emissivity: Float,

    /// The thermal inertia characterizes the sensitivity to temperature changes.
    /// units: kg/s^{5/2}/K or J/m^2/s^0.5/K
    /// dimensions: M.T^{-5/2}.Θ^{-1}).
    #[serde(default)]
    pub thermal_inertia: Float,

    /// Material density.
    /// units: kg/m^3
    /// dimensions: M.L^{-3}.
    #[serde(default)]
    pub density: Float,

    /// Heat capacity.
    /// units: m^2/s^2/K or J/K/kg
    /// dimensions: L^{2}.T^{-2}.Θ^{-1}).
    #[serde(default)]
    pub heat_capacity: Float,
}

impl Default for Material {
    fn default() -> Self {
        Self {
            albedo: 0.0,
            emissivity: default_emissivity(),
            thermal_inertia: 0.0,
            density: 0.0,
            heat_capacity: 0.0,
        }
    }
}

fn default_emissivity() -> Float {
    1.0
}

impl Material {
    pub fn conductivity(&self) -> Float {
        conductivity(self.thermal_inertia, self.density, self.heat_capacity)
    }

    pub fn diffusivity(&self) -> Float {
        diffusivity(self.conductivity(), self.density, self.heat_capacity)
    }

    pub fn thermal_skin_depth(&self, period: Float) -> Float {
        thermal_skin_depth(self.diffusivity(), period)
    }
}
