use crate::util::*;

use serde::{Deserialize, Serialize};

pub const DEFAULT_ALBEDO: Float = 0.0;
pub const DEFAULT_EMISSIVITY: Float = 1.0;
pub const DEFAULT_THERMAL_INERTIA: Float = 500.0;
pub const DEFAULT_DENSITY: Float = 2000.0;
pub const DEFAULT_HEAT_CAPACITY: Float = 600.0;

pub fn thermal_skin_depth_one(diffusivity: Float, period: Float) -> Float {
    (diffusivity * period / PI).sqrt()
}

pub fn thermal_skin_depth_two_pi(diffusivity: Float, period: Float) -> Float {
    (4.0 * PI * diffusivity * period).sqrt()
}

/// units: W.m^{-1}.K^{-1}
pub fn conductivity(thermal_inertia: Float, density: Float, heat_capacity: Float) -> Float {
    thermal_inertia.powi(2) / (density * heat_capacity)
}

pub fn diffusivity(conductivity: Float, density: Float, heat_capacity: Float) -> Float {
    conductivity / (density * heat_capacity)
}

/**
# Configuration of Material for Surface of Body

## Default

```yaml
albedo: 0.0
emissivity: 1.0
thermal_inertia: 0.0
density: 0.0
heat_capacity: 0.0
```

*/
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Material {
    /// The surface albedo defines the capacity to reflect the light.
    #[serde(default)]
    pub albedo: Float,

    /// Surface emissivity. For a black body it is 1.0.
    #[serde(default)]
    pub emissivity: Float,
    // pub emissivity: Option<Float>,
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

impl Material {
    pub fn albedo(&self) -> Float {
        self.albedo
    }

    pub fn emissivity(&self) -> Float {
        self.emissivity
        // self.emissivity.unwrap_or(DEFAULT_EMISSIVITY)
    }

    pub fn thermal_inertia(&self) -> Float {
        self.thermal_inertia
    }

    pub fn density(&self) -> Float {
        self.density
    }

    pub fn heat_capacity(&self) -> Float {
        self.heat_capacity
    }

    pub fn conductivity(&self) -> Float {
        conductivity(self.thermal_inertia(), self.density(), self.heat_capacity())
    }

    pub fn diffusivity(&self) -> Float {
        diffusivity(self.conductivity(), self.density(), self.heat_capacity())
    }

    pub fn thermal_skin_depth_one(&self, period: Float) -> Float {
        thermal_skin_depth_one(self.diffusivity(), period)
    }

    pub fn thermal_skin_depth_two_pi(&self, period: Float) -> Float {
        thermal_skin_depth_two_pi(self.diffusivity(), period)
    }
}
